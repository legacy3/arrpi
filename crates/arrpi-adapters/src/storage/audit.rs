//! Bounded write-attempt history shares the private settings backup unit.

use super::{
    Store,
    settings::{lock_error, sql_error},
};
use arrpi_core::{
    contracts::{AuditEntry, AuditPage},
    error::ApiError,
    executor::AuditEvent,
};
use rusqlite::params;

/// Optional filters applied within one account's retained history.
#[derive(Debug, Default)]
pub struct AuditFilter<'a> {
    /// Fuzzy title, action or source query (at most 128 characters).
    pub query: &'a str,
    /// Exact registered tool name.
    pub tool: Option<&'a str>,
    /// Exact stored outcome.
    pub outcome: Option<&'a str>,
    /// Inclusive UTC RFC3339 boundary with millisecond precision.
    pub since: Option<&'a str>,
}

impl Store {
    /// Commit a bounded batch of whitelisted write-attempt coordinates atomically.
    ///
    /// # Errors
    /// Returns a redacted error for invalid integer bounds or database failure.
    pub fn record_audit(
        &self,
        client: &str,
        actor: Option<u64>,
        events: &[AuditEvent],
        maximum: usize,
    ) -> Result<(), ApiError> {
        if events.is_empty() {
            return Ok(());
        }

        let actor = actor.map(integer).transpose()?;
        let maximum = integer(maximum)?;
        let mut connection = self.connection.lock().map_err(|error| lock_error(&error))?;
        let transaction = connection
            .transaction()
            .map_err(|error| sql_error(&error))?;

        for event in events {
            let target = serde_json::to_string(&event.target)
                .map_err(|error| super::settings::json_error(&error))?;

            transaction.execute("INSERT INTO write_audit (client,actor_id,tool,target,outcome,created_at) VALUES (?1,?2,?3,?4,?5,strftime('%Y-%m-%dT%H:%M:%fZ','now'))",params![client,actor,event.tool,target,event.outcome]).map_err(|error| sql_error(&error))?;
        }

        transaction.execute("DELETE FROM write_audit WHERE client=?1 AND id NOT IN (SELECT id FROM write_audit WHERE client=?1 ORDER BY id DESC LIMIT ?2)",params![client,maximum]).map_err(|error| sql_error(&error))?;
        transaction.commit().map_err(|error| sql_error(&error))?;
        Ok(())
    }

    /// Filter all retained entries, streaming rows and retaining only one result page.
    ///
    /// # Errors
    /// Rejects excessive queries/bounds and redacts storage failures.
    pub fn search_audit(
        &self,
        client: &str,
        offset: usize,
        limit: usize,
        filter: &AuditFilter<'_>,
    ) -> Result<AuditPage, ApiError> {
        use nucleo_matcher::{
            Config, Matcher, Utf32Str,
            pattern::{AtomKind, CaseMatching, Normalization, Pattern},
        };
        let AuditFilter {
            query,
            tool,
            outcome,
            since,
        } = filter;

        if query.chars().count() > 128 || limit == 0 || limit > 1000 {
            return Err(ApiError::BAD_QUERY);
        }

        if query.is_empty() && tool.is_none() && outcome.is_none() && since.is_none() {
            return self.write_audit(client, offset, limit);
        }

        let pattern = Pattern::new(
            query,
            CaseMatching::Ignore,
            Normalization::Smart,
            AtomKind::Fuzzy,
        );
        let mut matcher = Matcher::new(Config::DEFAULT);
        let mut buffer = Vec::new();
        let connection = self.connection.lock().map_err(|error| lock_error(&error))?;
        let mut statement = connection.prepare("SELECT id,actor_id,tool,target,outcome,created_at FROM write_audit WHERE client=?1 AND (?2 IS NULL OR tool=?2) AND (?3 IS NULL OR outcome=?3) AND (?4 IS NULL OR created_at>=?4) ORDER BY id DESC").map_err(|error| sql_error(&error))?;
        let rows = statement
            .query_map(params![client, tool, outcome, since], audit_row)
            .map_err(|error| sql_error(&error))?;
        let mut items = Vec::with_capacity(limit);
        let mut total = 0usize;

        for row in rows {
            let row = row.map_err(|error| sql_error(&error))?;
            let action = match arrpi_core::registry::ToolId::parse(&row.tool) {
                Some(id) => id.title(),
                None => &row.tool,
            };
            let text = format!(
                "{} {} {} {} {} {} {} {} {} {}",
                row.target.title.as_deref().unwrap_or(""),
                action,
                row.tool,
                row.outcome,
                row.target.source.as_deref().unwrap_or(""),
                row.target.tmdb_id.or(row.target.item_id).unwrap_or(0),
                row.target.command_id.unwrap_or(0),
                row.target.request_id.unwrap_or(0),
                row.target.issue_id.unwrap_or(0),
                row.target.season.unwrap_or(0)
            );

            if !query.is_empty()
                && pattern
                    .score(Utf32Str::new(&text, &mut buffer), &mut matcher)
                    .is_none()
            {
                continue;
            }

            if total >= offset && items.len() < limit {
                items.push(row);
            }

            total += 1;
        }

        let next = offset.saturating_add(items.len());

        Ok(AuditPage {
            client: client.to_owned(),
            items,
            total: u64::try_from(total).map_err(|error| {
                tracing::debug!(event="audit_count",category=%error);
                ApiError::INTERNAL
            })?,
            offset,
            next_offset: (next < total).then_some(next),
        })
    }

    /// Page one account's retained write attempts in newest-first order.
    ///
    /// # Errors
    /// Returns a redacted error for invalid bounds, unavailable storage or malformed rows.
    pub fn write_audit(
        &self,
        client: &str,
        offset: usize,
        limit: usize,
    ) -> Result<AuditPage, ApiError> {
        if limit == 0 {
            return Err(ApiError::BAD_QUERY);
        }

        let connection = self.connection.lock().map_err(|error| lock_error(&error))?;
        let total = connection
            .query_row(
                "SELECT COUNT(*) FROM write_audit WHERE client=?1",
                [client],
                |row| super::unsigned(row, 0),
            )
            .map_err(|error| sql_error(&error))?;
        let mut statement = connection.prepare("SELECT id,actor_id,tool,target,outcome,created_at FROM write_audit WHERE client=?1 ORDER BY id DESC LIMIT ?2 OFFSET ?3").map_err(|error| sql_error(&error))?;
        let items = statement
            .query_map(
                params![client, integer(limit)?, integer(offset)?],
                audit_row,
            )
            .map_err(|error| sql_error(&error))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| sql_error(&error))?;
        let next = offset.saturating_add(items.len());

        Ok(AuditPage {
            client: client.to_owned(),
            items,
            total,
            offset,
            next_offset: (u64::try_from(next).unwrap_or(u64::MAX) < total).then_some(next),
        })
    }
}

fn audit_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AuditEntry> {
    let encoded: String = row.get(3)?;
    let target = serde_json::from_str(&encoded).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(error))
    })?;

    Ok(AuditEntry {
        id: super::unsigned(row, 0)?,
        actor_id: super::optional_unsigned(row, 1)?,
        tool: row.get(2)?,
        target,
        outcome: row.get(4)?,
        created_at: row.get(5)?,
    })
}

fn integer<T>(value: T) -> Result<i64, ApiError>
where
    T: TryInto<i64>,
    T::Error: std::fmt::Display,
{
    value.try_into().map_err(|error| {
        tracing::debug!(event="audit_integer",category=%error);
        ApiError::BAD_QUERY
    })
}

#[cfg(test)]
mod tests;

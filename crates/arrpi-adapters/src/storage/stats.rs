//! Aggregate counters contain no tool arguments or credentials.

use super::{
    Store,
    settings::{lock_error, sql_error},
};
use arrpi_core::{
    contracts::{ToolStat, ToolStatsPage},
    error::ApiError,
};
use rusqlite::params;

impl Store {
    /// Persist one completed command observation.
    ///
    /// # Errors
    /// Returns a redacted error if the database or integer capacity is unavailable.
    pub fn record_tool(
        &self,
        client: &str,
        tool: &str,
        outcome: &str,
        duration_ms: u64,
    ) -> Result<(), ApiError> {
        let duration_ms = i64::try_from(duration_ms).map_err(|error| {
            tracing::debug!(event="stats_duration",category=%error);
            ApiError::CAPACITY
        })?;
        let connection = self.connection.lock().map_err(|error| lock_error(&error))?;

        connection.execute("INSERT INTO tool_stats (client,tool,outcome,calls,duration_ms,last_duration_ms,last_used) VALUES (?1,?2,?3,1,?4,?4,strftime('%Y-%m-%dT%H:%M:%fZ','now')) ON CONFLICT(client,tool,outcome) DO UPDATE SET calls=calls+1,duration_ms=duration_ms+excluded.duration_ms,last_duration_ms=excluded.last_duration_ms,last_used=excluded.last_used",params![client,tool,outcome,duration_ms]).map_err(|error| sql_error(&error))?;
        Ok(())
    }

    /// Read only one account's aggregate observations.
    ///
    /// # Errors
    /// Returns a redacted error for unavailable or malformed stored rows.
    pub fn tool_stats(&self, client: &str) -> Result<ToolStatsPage, ApiError> {
        let connection = self.connection.lock().map_err(|error| lock_error(&error))?;
        let mut statement = connection.prepare("SELECT tool,outcome,calls,duration_ms,last_duration_ms,last_used FROM tool_stats WHERE client=?1 ORDER BY tool,outcome").map_err(|error| sql_error(&error))?;
        let items = statement
            .query_map([client], |row| {
                Ok(ToolStat {
                    tool: row.get(0)?,
                    outcome: row.get(1)?,
                    calls: super::unsigned(row, 2)?,
                    duration_ms: super::unsigned(row, 3)?,
                    last_duration_ms: super::unsigned(row, 4)?,
                    last_used: row.get(5)?,
                })
            })
            .map_err(|error| sql_error(&error))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| sql_error(&error))?;

        Ok(ToolStatsPage {
            client: client.to_owned(),
            items,
        })
    }
}

//! Independent failures and explicit completeness for operational snapshots.
use crate::{
    Commands,
    arguments::Fresh,
    error::ApiError,
    source::{Principal, ReadMode},
    summaries::{Summary, SummaryKind, SummaryRow},
};

impl Commands {
    /// Read only the summary capabilities this principal is explicitly allowed to see.
    ///
    /// # Errors
    /// Returns forbidden if no authorized source implements this projection.
    pub async fn summaries(
        &self,
        principal: &Principal,
        kind: SummaryKind,
        args: Fresh,
    ) -> Result<Summary, ApiError> {
        self.selected_summaries(principal, None, kind, args).await
    }

    /// Read one source without querying or returning unrelated connections.
    ///
    /// # Errors
    /// Returns forbidden unless this exact source grants the requested projection.
    pub async fn source_summary(
        &self,
        principal: &Principal,
        source: &str,
        kind: SummaryKind,
        args: Fresh,
    ) -> Result<Summary, ApiError> {
        self.selected_summaries(principal, Some(source), kind, args)
            .await
    }

    async fn selected_summaries(
        &self,
        principal: &Principal,
        selected: Option<&str>,
        kind: SummaryKind,
        args: Fresh,
    ) -> Result<Summary, ApiError> {
        let mut sources = Vec::with_capacity(if selected.is_some() {
            1
        } else {
            self.sources.len()
        });

        for (name, source) in &self.sources {
            if selected.is_some_and(|selected| selected != name) {
                continue;
            }

            let Some(summary) = source.summaries().filter(|summary| summary.supports(kind)) else {
                continue;
            };

            if !principal.allows(name, kind.grant()) {
                continue;
            }

            let value = match summary
                .summary(
                    kind,
                    if args.no_cache {
                        ReadMode::Fresh
                    } else {
                        ReadMode::Cached
                    },
                )
                .await
            {
                Ok(read) => SummaryRow::Ok {
                    source: name.to_owned(),
                    kind: source.descriptor().kind.to_owned(),
                    data: Box::new(read.data),
                    freshness: read.freshness.document(args.no_cache),
                },

                Err(error) => SummaryRow::Unavailable {
                    source: name.to_owned(),
                    kind: source.descriptor().kind.to_owned(),
                    code: error.code().to_owned(),
                },
            };

            sources.push(value);
        }

        if sources.is_empty() {
            return Err(ApiError::FORBIDDEN);
        }

        Ok(Summary {
            complete: sources.iter().all(SummaryRow::is_complete),
            sources,
        })
    }
}

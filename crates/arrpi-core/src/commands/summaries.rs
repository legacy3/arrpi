//! Independent failures and explicit completeness for operational snapshots.
use crate::{
    Commands,
    arguments::Fresh,
    error::ApiError,
    source::{Principal, ReadMode},
    summaries::SummaryKind,
};
use serde_json::{Value, json};

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
    ) -> Result<Value, ApiError> {
        let mut sources = Vec::with_capacity(self.sources.len());

        for (name, source) in &self.sources {
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
                Ok(read) => {
                    json!({"source":name,"kind":source.descriptor().kind,"status":"ok","data":read.data,"freshness":read.freshness.public(args.no_cache)})
                }

                Err(error) => {
                    json!({"source":name,"kind":source.descriptor().kind,"status":"unavailable","code":error.code()})
                }
            };

            sources.push(value);
        }

        if sources.is_empty() {
            return Err(ApiError::FORBIDDEN);
        }

        Ok(
            json!({"complete":sources.iter().all(|source| source["status"] == "ok" && source["data"]["complete"] == true && source["freshness"]["cache"] != "stale"),"sources":sources}),
        )
    }
}

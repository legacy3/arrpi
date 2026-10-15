//! Source health is capability-based and never promotes cached failures to live reachability.

use crate::{
    arguments::Fresh,
    error::ApiError,
    source::{Observation, Principal, ReadMode, Sources},
};
use serde_json::{Value, json};

pub(super) async fn status(
    sources: &Sources,
    principal: &Principal,
    args: Fresh,
) -> Result<Value, ApiError> {
    let mut output = Vec::with_capacity(sources.len());

    for (name, source) in sources {
        let authorized = (source.summaries().is_some() && principal.allows(name, "status-view"))
            || (source.library().is_some() && principal.allows(name, "library-view"))
            || (source.calendar().is_some() && principal.allows(name, "calendar-view"))
            || (source.accounts().is_some()
                && (principal.binding.is_some()
                    && (principal.allows("catalog", "view")
                        || principal.allows("requests", "view"))));

        if !authorized {
            continue;
        }

        let value = match source
            .health(
                principal,
                if args.no_cache {
                    ReadMode::Fresh
                } else {
                    ReadMode::Cached
                },
            )
            .await
        {
            Ok(health) => {
                let mut value = json!({"source":name,"kind":source.descriptor().kind,"reachable":health.reachable,"needs_account":health.needs_account});

                if let Some(version) = health.version {
                    value["version"] = json!(version);
                }

                if let Some(issues) = health.health_issue_count {
                    value["health_issue_count"] = json!(issues);
                }

                if let Some(compatible) = health.writes_supported {
                    value["writes_supported"] = json!(compatible);
                }

                if let Some(freshness) = health.freshness {
                    if matches!(freshness.cache, Observation::NotCached) {
                        value["freshness"] = freshness.public(args.no_cache);
                        value["freshness"]["bypass_requested"] = json!(args.no_cache);
                    } else {
                        value["cache"] = json!(freshness.cache);
                        value["age_seconds"] = json!(freshness.age_seconds.unwrap_or(0));
                    }
                }

                value
            }

            Err(error) => {
                json!({"source":name,"kind":source.descriptor().kind,"reachable":false,"code":error.code()})
            }
        };

        output.push(value);
    }

    Ok(json!({"complete":output.iter().all(|source| source["reachable"] == true),"sources":output}))
}

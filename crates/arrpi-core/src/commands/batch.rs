//! Ordered bounded batches share admission, stop and receipt handling.

use super::{Caller, Commands, profiles};
use crate::{
    arguments as args, error::ApiError, policy::Binding, registry::ToolId, source::ApiSource,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use tokio::time::Instant;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy)]
enum Mode {
    Read,
    Write,
}

impl Mode {
    const fn timeout(self) -> ApiError {
        match self {
            Self::Read => ApiError::BATCH_TIMEOUT,
            Self::Write => ApiError::UNKNOWN_OUTCOME,
        }
    }

    fn stops(self, result: &Result<Value, ApiError>) -> bool {
        match result {
            Err(
                ApiError::BUSY
                | ApiError::CAPACITY
                | ApiError::QUEUE_TIMEOUT
                | ApiError::UPSTREAM_RATE_LIMITED,
            ) => true,

            Err(ApiError::UNAVAILABLE | ApiError::TIMEOUT | ApiError::UNSUPPORTED_VERSION) => {
                matches!(self, Self::Write)
            }

            _ => false,
        }
    }
}

pub(super) async fn details(
    commands: &Commands,
    caller: &Caller<'_>,
    source: &dyn ApiSource,
    binding: &Binding,
    args: args::BatchDetails,
) -> Result<Value, ApiError> {
    validate(
        args.items
            .iter()
            .map(|item| (item.media_type.name(), item.tmdb_id)),
        args.items.len(),
        commands.policy.limits.max_batch_items,
    )?;
    let deadline = deadline(commands);
    let catalog = source.catalog().ok_or(ApiError::FORBIDDEN)?;
    let settings = tokio::time::timeout_at(deadline, async {
        let actor = source
            .accounts()
            .ok_or(ApiError::FORBIDDEN)?
            .identity(binding)
            .await?;
        let checked = Caller {
            actor: Some(actor),
            ..*caller
        };

        if !commands.allowed(&checked, ToolId::MediaDetails) {
            return Err(ApiError::FORBIDDEN);
        }

        Ok(catalog.public_settings(binding).await)
    })
    .await
    .map_err(|error| {
        tracing::debug!(event = "read_batch_preparation_timeout", category = %error);
        ApiError::BATCH_TIMEOUT
    })??;

    ordered(
        commands.clock,
        args.items,
        deadline,
        Mode::Read,
        |mut item| {
            item.no_cache |= args.no_cache;
            let settings = &settings;

            async move { catalog.details(binding, item, settings).await }
        },
    )
    .await
}

pub(super) async fn requests(
    commands: &Commands,
    caller: &Caller<'_>,
    source: &dyn ApiSource,
    binding: &Binding,
    args: args::BatchRequests,
) -> Result<Value, ApiError> {
    validate(
        args.items
            .iter()
            .map(|item| (item.media_type.name(), item.tmdb_id)),
        args.items.len(),
        commands.policy.limits.max_batch_items,
    )?;

    for item in &args.items {
        args::validate_seasons(item.media_type, &item.seasons)?;
        profiles::validate_name(item.profile.as_deref())?;
    }

    ordered(
        commands.clock,
        args.items,
        deadline(commands),
        Mode::Write,
        |item| async move {
            let leaf = if item.media_type == args::MediaType::Movie {
                ToolId::RequestMovie
            } else {
                ToolId::RequestSeries
            };

            if !commands.allowed(caller, leaf) {
                return Err(ApiError::FORBIDDEN);
            }

            profiles::submit(commands, source, binding, item).await
        },
    )
    .await
}

pub(super) async fn issues(
    commands: &Commands,
    caller: &Caller<'_>,
    source: &dyn ApiSource,
    binding: &Binding,
    args: args::BatchIssues,
) -> Result<Value, ApiError> {
    validate(
        args.issue_ids.iter().map(|id| ("issue", *id)),
        args.issue_ids.len(),
        commands.policy.limits.max_batch_items,
    )?;

    ordered(
        commands.clock,
        args.issue_ids,
        deadline(commands),
        Mode::Write,
        |id| async move {
            let actor = source
                .accounts()
                .ok_or(ApiError::FORBIDDEN)?
                .identity(binding)
                .await?;
            let checked = Caller {
                actor: Some(actor),
                ..*caller
            };

            if !commands.allowed(&checked, ToolId::CloseIssue) {
                return Err(ApiError::FORBIDDEN);
            }

            source
                .issues()
                .ok_or(ApiError::FORBIDDEN)?
                .close(binding, id)
                .await
        },
    )
    .await
}

// Stop on rate limits or exhausted infrastructure; remaining items were not dispatched.
// Each named choice is still freshly verified because native settings can change mid-batch.
async fn ordered<T, F, R>(
    clock: fn() -> Instant,
    items: Vec<T>,
    deadline: Instant,
    mode: Mode,
    mut submit: F,
) -> Result<Value, ApiError>
where
    T: BatchItem,
    F: FnMut(T) -> R,
    R: Future<Output = Result<Value, ApiError>>,
{
    let mut stopped = false;
    let mut output = Vec::with_capacity(items.len());

    for item in items {
        let mut target = item.coordinate();

        if stopped || clock() >= deadline {
            target["ok"] = json!(false);
            target["code"] = json!("not_attempted");
            target["error"] = json!(
                "Batch stopped after its time budget or an infrastructure failure; this item was not started."
            );
            output.push(target);
            continue;
        }

        let result = if let Ok(result) = tokio::time::timeout_at(deadline, submit(item)).await {
            result
        } else {
            stopped = true;
            Err(mode.timeout())
        };

        stopped |= mode.stops(&result);
        output.push(receipt(target, result));
    }

    Ok(summary(&output))
}

fn deadline(commands: &Commands) -> Instant {
    (commands.clock)()
        + std::time::Duration::from_secs(commands.policy.limits.batch_timeout_seconds)
}

fn validate<'a>(
    items: impl Iterator<Item = (&'a str, u64)>,
    length: usize,
    maximum: usize,
) -> Result<(), ApiError> {
    if length == 0 || length > maximum {
        return Err(ApiError::BAD_QUERY);
    }

    let mut seen = BTreeSet::new();

    for item in items {
        args::positive(item.1)?;

        if !seen.insert(item) {
            return Err(ApiError::BAD_QUERY);
        }
    }

    Ok(())
}

fn receipt(mut target: Value, result: Result<Value, ApiError>) -> Value {
    match result {
        Ok(result) => {
            target["ok"] = json!(true);
            target["result"] = result;
        }

        Err(error) => {
            if let (Some(target), Some(error)) =
                (target.as_object_mut(), error.payload().as_object())
            {
                target.extend(error.clone());
            }

            target["ok"] = json!(false);
        }
    }

    target
}

fn summary(items: &[Value]) -> Value {
    let succeeded = items.iter().filter(|item| item["ok"] == true).count();

    json!({"succeeded":succeeded,"failed":items.len()-succeeded,"items":items})
}

trait BatchItem {
    fn coordinate(&self) -> Value;
}
impl BatchItem for args::Details {
    fn coordinate(&self) -> Value {
        json!({"tmdb_id":self.tmdb_id,"media_type":self.media_type.name()})
    }
}
impl BatchItem for args::MediaRequest {
    fn coordinate(&self) -> Value {
        json!({"tmdb_id":self.tmdb_id,"media_type":self.media_type.name()})
    }
}
impl BatchItem for u64 {
    fn coordinate(&self) -> Value {
        json!({"issue_id":self})
    }
}

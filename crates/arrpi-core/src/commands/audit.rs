//! Audit coordinates are typed and omit messages, credentials and arbitrary inputs.

use crate::{
    arguments::MediaType, contracts::AuditTarget, error::ApiError, executor::AuditEvent,
    registry::ToolId,
};
use serde_json::Value;

#[derive(Debug)]
pub(super) struct Capture {
    single: AuditTarget,
    items: Vec<AuditTarget>,
}

pub(super) fn capture(input: &Value) -> Capture {
    Capture {
        single: target(input),
        items: input
            .get("items")
            .and_then(Value::as_array)
            .map_or_else(Vec::new, |items| items.iter().map(target).collect()),
    }
}

pub(super) fn events(
    tool: ToolId,
    input: &Capture,
    result: &Result<Value, ApiError>,
) -> Vec<AuditEvent> {
    if matches!(tool, ToolId::RequestMediaBatch | ToolId::CloseIssuesBatch)
        && let Ok(value) = result
        && let Some(rows) = value.get("items").and_then(Value::as_array)
    {
        return rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row["code"] != "not_attempted")
            .map(|(index, row)| {
                let leaf = if tool == ToolId::CloseIssuesBatch {
                    ToolId::CloseIssue
                } else if row["media_type"] == "tv" {
                    ToolId::RequestSeries
                } else {
                    ToolId::RequestMovie
                };
                let receipt = if row["ok"] == true {
                    &row["result"]
                } else {
                    row
                };

                AuditEvent {
                    tool: leaf.name().to_owned(),
                    target: merge(
                        input
                            .items
                            .get(index)
                            .cloned()
                            .unwrap_or_else(|| target(row)),
                        receipt,
                    ),
                    outcome: if row["ok"] == true {
                        outcome(receipt)
                    } else {
                        row["code"].as_str().unwrap_or("failed").to_owned()
                    },
                }
            })
            .collect();
    }

    let failure;
    let reported = match result {
        Ok(value) => value,

        Err(error) => {
            failure = error.payload();
            &failure
        }
    };

    vec![AuditEvent {
        tool: tool.name().to_owned(),
        target: merge(input.single.clone(), reported),
        outcome: result
            .as_ref()
            .map_or_else(|error| error.code().to_owned(), outcome),
    }]
}

fn outcome(value: &Value) -> String {
    value
        .get("status_name")
        .or_else(|| value.get("status"))
        .and_then(Value::as_str)
        .unwrap_or("ok")
        .to_owned()
}

fn number(value: &Value, key: &str) -> Option<u64> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .filter(|id| u32::try_from(*id).is_ok())
}

fn target(value: &Value) -> AuditTarget {
    AuditTarget {
        tmdb_id: number(value, "tmdb_id"),
        request_id: number(value, "request_id"),
        issue_id: number(value, "issue_id"),
        season: number(value, "season").and_then(|id| u32::try_from(id).ok()),
        episode: number(value, "episode").and_then(|id| u32::try_from(id).ok()),
        media_type: match value.get("media_type").and_then(Value::as_str) {
            Some("movie") => Some(MediaType::Movie),
            Some("tv") => Some(MediaType::Tv),
            _ => None,
        },
        seasons: value
            .get("seasons")
            .and_then(Value::as_array)
            .filter(|items| items.len() <= 50)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_u64)
                    .filter(|season| (1..=1000).contains(season))
                    .filter_map(|season| u32::try_from(season).ok())
                    .collect()
            }),
    }
}

fn merge(mut coordinate: AuditTarget, receipt: &Value) -> AuditTarget {
    let other = target(receipt);

    coordinate.tmdb_id = coordinate.tmdb_id.or(other.tmdb_id);
    coordinate.request_id = coordinate.request_id.or(other.request_id);
    coordinate.issue_id = coordinate.issue_id.or(other.issue_id);
    coordinate.season = coordinate.season.or(other.season);
    coordinate.episode = coordinate.episode.or(other.episode);
    coordinate.media_type = coordinate.media_type.or(other.media_type);
    coordinate
}

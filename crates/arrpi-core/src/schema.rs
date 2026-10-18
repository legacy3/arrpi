//! Static public contracts exported without opening configuration or application state.

use crate::contracts::{
    AuditPage, Calendar, ConfigUpdateResult, ConnectionHealth, McpStatus, SettingsEnvelope, Status,
    ToolStatsPage,
};
use schemars::JsonSchema;
use serde_json::{Value, json};

#[derive(JsonSchema, serde::Serialize)]
struct Contracts {
    settings: SettingsEnvelope,
    accounts: crate::contracts::LinkedAccountPage,
    status: Status,
    calendar: Calendar,
    mcp: McpStatus,
    stats: ToolStatsPage,
    audit: AuditPage,
    updated: ConfigUpdateResult,
    connection_health: ConnectionHealth,
    summary: crate::summaries::Summary,
    commands: crate::registry::CommandCatalog,
    tracking: crate::tracking::Status,
    tracking_authorization: crate::tracking::Authorization,
    tracking_poll: crate::tracking::Poll,
}

/// Export public DTO schemas as `OpenAPI` components for client generation.
#[must_use]
pub fn document() -> Value {
    let settings = schemars::generate::SchemaSettings::draft2020_12().with(|settings| {
        settings.definitions_path = "/components/schemas".into();
        settings.contract = schemars::generate::Contract::Serialize;
    });
    let schema = settings
        .into_generator()
        .into_root_schema_for::<Contracts>();
    let mut schema = Value::from(schema);
    let components = schema
        .get_mut("components")
        .map_or_else(|| json!({}), Value::take);

    json!({
        "openapi":"3.1.0",
        "info":{"title":"ArrPi public API","version":env!("CARGO_PKG_VERSION")},
        "paths":{},
        "components":components
    })
}

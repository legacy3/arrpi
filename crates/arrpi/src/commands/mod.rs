//! Authenticated HTTP entry points for the protocol-independent command executor.

use crate::{
    auth,
    error::{ApiError, HttpError},
    proxy::Gateway,
};
use arrpi_core::registry::{CommandCatalog, CommandDescriptor, ToolId};
use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use serde_json::Value;
use std::sync::Arc;

pub(crate) async fn http_list(
    State(gateway): State<Arc<Gateway>>,
    headers: HeaderMap,
) -> Result<Json<CommandCatalog>, HttpError> {
    let principal = auth::authenticate(&headers, &gateway.clients)?;
    let definitions = gateway.commands.list(principal).await?;
    let broker = gateway.commands.sources.account_source();
    let tools = definitions
        .into_iter()
        .map(|definition| {
            let account = ToolId::parse(&definition.name).is_some_and(|tool| !tool.is_native());
            let mut descriptor = CommandDescriptor::from(definition);

            if let Some(tool) = ToolId::parse(&descriptor.name) {
                descriptor.sources = gateway
                    .commands
                    .command_sources(principal, tool)
                    .map(str::to_owned)
                    .collect();
            }

            descriptor.source = broker
                .filter(|_| account)
                .map(|source| source.descriptor().name.clone());
            descriptor
        })
        .collect();

    Ok(Json(CommandCatalog {
        tools,
        account: principal.binding.as_ref().map(|_| principal.name.clone()),
    }))
}

pub(crate) async fn http_execute(
    State(gateway): State<Arc<Gateway>>,
    Path(name): Path<String>,
    headers: HeaderMap,
    Json(input): Json<Value>,
) -> Result<Json<Value>, HttpError> {
    let principal = auth::authenticate(&headers, &gateway.clients)?;
    let tool = ToolId::parse(&name).ok_or(ApiError::NOT_FOUND)?;

    Ok(Json(
        gateway.commands.execute(principal, tool, input).await?,
    ))
}

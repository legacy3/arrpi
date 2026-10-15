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
    let tools = definitions
        .into_iter()
        .map(CommandDescriptor::from)
        .collect();

    Ok(Json(CommandCatalog { tools }))
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

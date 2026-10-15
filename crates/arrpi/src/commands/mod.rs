//! Authenticated HTTP entry points for the protocol-independent command executor.

use crate::{
    auth,
    error::{ApiError, HttpError},
    proxy::Gateway,
};
use arrpi_core::registry::ToolId;
use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use serde_json::{Value, json};
use std::sync::Arc;

pub(crate) async fn http_list(
    State(gateway): State<Arc<Gateway>>,
    headers: HeaderMap,
) -> Result<Json<Value>, HttpError> {
    let principal = auth::authenticate(&headers, &gateway.clients)?;
    let definitions = gateway.commands.list(principal).await?;
    let tools: Vec<_> = definitions.into_iter().map(arrpi_mcp::describe).collect();

    Ok(Json(json!({"tools":tools})))
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

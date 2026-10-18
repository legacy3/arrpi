//! Protected account chooser reads existing Plex-linked Seerr users only.
use super::{Runtime, lock_error};
use crate::{
    auth,
    error::{ApiError, HttpError},
};
use arrpi_core::contracts::LinkedAccountPage;
use axum::{
    Json,
    extract::{Query, State},
    http::HeaderMap,
};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Page {
    #[serde(default)]
    offset: usize,
}

pub(super) async fn read(
    State(runtime): State<Arc<Runtime>>,
    Query(page): Query<Page>,
    headers: HeaderMap,
) -> Result<Json<LinkedAccountPage>, HttpError> {
    let (gateway, principal) = {
        let live = runtime.live.read().map_err(|error| lock_error(&error))?;
        let principal = (**auth::authenticate(&headers, &live.clients)?).clone();

        (Arc::clone(&live.gateway), principal)
    };
    let source = gateway
        .commands
        .sources
        .account_source()
        .and_then(arrpi_core::source::ApiSource::accounts)
        .ok_or(ApiError::NOT_FOUND)?;
    let result = source.linked_accounts(&principal, page.offset, 100).await?;

    Ok(Json(result))
}

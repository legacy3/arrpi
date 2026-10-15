//! Settings edits replace the same router used by HTTP, CLI and web consumers.
mod health;

use std::{
    path::PathBuf,
    sync::{Arc, RwLock},
};

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Query, Request, State},
    http::{HeaderMap, header},
    middleware::{self, Next},
    response::Response,
    routing::get,
};
use serde::Deserialize;
use serde_json::{Value, json};
use tower::ServiceExt as _;

use super::Store;
use crate::{
    auth::{self, Access},
    config::Config,
    error::{ApiError, HttpError},
    proxy::{self, Gateway},
};
use arrpi_adapters::storage;
use arrpi_core::contracts::{
    AuditPage, ConfigUpdateResult, McpStatus, SettingsEnvelope, ToolStatsPage,
};

type Build = fn(&Config, &std::path::Path, Store) -> Result<Gateway, ApiError>;

#[derive(Debug)]
struct Live {
    router: Router,
    clients: Vec<Access>,
    gateway: Arc<Gateway>,
    _maintenance: arrpi_adapters::cache::Maintenance,
    _oauth_maintenance: Option<arrpi_adapters::oauth::Maintenance>,
}

#[derive(Debug)]
pub(crate) struct Runtime {
    pub(crate) store: Arc<Store>,
    live: RwLock<Live>,
    database: PathBuf,
    build: Build,
    editing: tokio::sync::Mutex<()>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Update {
    revision: i64,
    settings: Value,
}

impl Runtime {
    pub(crate) fn new(
        store: Arc<Store>,
        database: PathBuf,
        build: Build,
    ) -> Result<Self, ApiError> {
        let (_, config) = store.load()?.ok_or(ApiError::INTERNAL)?;
        let live = make_live(&config, &database, build, &store, None)?;

        Ok(Self {
            store,
            live: RwLock::new(live),
            database,
            build,
            editing: tokio::sync::Mutex::new(()),
        })
    }

    pub(crate) fn router(self) -> Router {
        let state = Arc::new(self);
        let settings = Router::new()
            .route("/v1/config", get(read).put(write))
            .route("/v1/config/connections/{source}/health", get(health::read))
            .route("/v1/mcp/status", get(mcp_status))
            .route("/v1/config/mcp/stats", get(mcp_stats))
            .route("/v1/config/mcp/audit", get(mcp_audit))
            .route("/v1/mcp/token", get(mcp_token))
            .route_layer(middleware::from_fn_with_state(
                Arc::clone(&state),
                authorize,
            ));

        Router::new()
            .merge(settings)
            .fallback(forward)
            .layer(DefaultBodyLimit::max(128 * 1024))
            .layer(middleware::map_response(safe_headers))
            .with_state(state)
    }

    pub(crate) async fn snapshot(self: &Arc<Self>) -> Result<(i64, Config), ApiError> {
        let store = Arc::clone(&self.store);

        tokio::task::spawn_blocking(move || store.load()?.ok_or(ApiError::INTERNAL)).await?
    }
}

fn make_live(
    config: &Config,
    database: &std::path::Path,
    build: Build,
    store: &Store,
    previous: Option<&arrpi_adapters::oauth::Verifier>,
) -> Result<Live, ApiError> {
    let mut gateway = build(config, database, store.clone())?;

    if let (Some(current), Some(previous)) = (&mut gateway.oauth, previous) {
        current.reuse_keys(previous);
    }

    let gateway = Arc::new(gateway);
    let maintenance = gateway.pruner.start().map_err(|error| {
        tracing::debug!(
            event = "settings_boundary_failure",
            category = std::any::type_name_of_val(&error)
        );
        ApiError::INTERNAL
    })?;
    let clients = config
        .clients
        .iter()
        .map(Access::new)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            tracing::debug!(event="settings_clients", category=%error);
            ApiError::BAD_QUERY
        })?;

    Ok(Live {
        router: proxy::router_shared(Arc::clone(&gateway))?,
        _maintenance: maintenance,
        _oauth_maintenance: gateway
            .oauth
            .as_ref()
            .map(arrpi_adapters::oauth::Verifier::maintain)
            .transpose()?,
        gateway,
        clients,
    })
}

async fn authorize(
    State(runtime): State<Arc<Runtime>>,
    request: Request,
    next: Next,
) -> Result<Response, HttpError> {
    {
        let live = runtime.live.read().map_err(|error| lock_error(&error))?;

        if !auth::authenticate(request.headers(), &live.clients)?.allows("config", "manage") {
            return Err(ApiError::FORBIDDEN.into());
        }
    }
    Ok(next.run(request).await)
}

async fn forward(
    State(runtime): State<Arc<Runtime>>,
    request: Request,
) -> Result<Response, HttpError> {
    let router = runtime
        .live
        .read()
        .map_err(|error| lock_error(&error))?
        .router
        .clone();

    match router.oneshot(request).await {
        Ok(response) => Ok(response),
        Err(never) => match never {},
    }
}

async fn read(
    State(runtime): State<Arc<Runtime>>,
) -> Result<(HeaderMap, Json<SettingsEnvelope>), HttpError> {
    let (revision, config) = runtime.snapshot().await?;
    let headers = HeaderMap::from_iter([(
        header::CACHE_CONTROL,
        "private, no-store".parse().map_err(|error| {
            tracing::debug!(event="settings_headers", category=%error);
            ApiError::INTERNAL
        })?,
    )]);

    Ok((
        headers,
        Json(SettingsEnvelope {
            revision: revision_number(revision)?,
            settings: config,
        }),
    ))
}

async fn write(
    State(runtime): State<Arc<Runtime>>,
    headers: HeaderMap,
    Json(update): Json<Update>,
) -> Result<Json<ConfigUpdateResult>, HttpError> {
    let _editing = runtime.editing.lock().await;

    {
        let live = runtime.live.read().map_err(|error| lock_error(&error))?;

        if !auth::authenticate(&headers, &live.clients)?.allows("config", "manage") {
            return Err(ApiError::FORBIDDEN.into());
        }
    }
    let (revision, current) = runtime.snapshot().await?;

    if revision != update.revision {
        return Err(ApiError::CONFLICT.into());
    }

    let mut config: Config = serde_json::from_value(update.settings).map_err(|error| {
        tracing::debug!(event="settings_input", category=?error.classify());
        ApiError::BAD_QUERY
    })?;

    storage::restore_secrets(&mut config, &current)?;
    config.validate().map_err(|error| {
        tracing::debug!(event="settings_validation", category=%error);
        ApiError::configuration(error)
    })?;

    if !config.clients.iter().any(|client| {
        client
            .permissions
            .iter()
            .any(|permission| matches!(permission.as_ref(), "config:manage" | "admin"))
    }) {
        return Err(ApiError::BAD_QUERY.into());
    }

    let restart_required = config.listen != current.listen
        || config.cache.max_connections != current.cache.max_connections
        || config.cache.connection_idle_seconds != current.cache.connection_idle_seconds
        || config.cache.connection_lifetime_seconds != current.cache.connection_lifetime_seconds;
    let task_runtime = Arc::clone(&runtime);
    let updated = tokio::task::spawn_blocking(move || {
        let previous_oauth = task_runtime
            .live
            .read()
            .map_err(|error| lock_error(&error))?
            .gateway
            .oauth
            .clone();
        let live = make_live(
            &config,
            &task_runtime.database,
            task_runtime.build,
            &task_runtime.store,
            previous_oauth.as_ref(),
        )?;
        let mut current = task_runtime
            .live
            .write()
            .map_err(|error| lock_error(&error))?;
        let updated = task_runtime.store.save(revision, &config)?;

        let previous = std::mem::replace(&mut *current, live);

        drop(current);
        drop(previous);
        Ok::<_, HttpError>(updated)
    })
    .await
    .map_err(ApiError::from)??;

    Ok(Json(ConfigUpdateResult {
        revision: revision_number(updated)?,
        restart_required,
    }))
}

pub(super) fn lock_error<T>(error: &std::sync::PoisonError<T>) -> ApiError {
    tracing::debug!(event="runtime_lock", poisoned=%error);
    ApiError::INTERNAL
}

async fn safe_headers(mut response: Response) -> Response {
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("private, no-store"),
    );
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        axum::http::HeaderValue::from_static("nosniff"),
    );
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        axum::http::HeaderValue::from_static("no-referrer"),
    );
    response
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClientQuery {
    client: String,
}

async fn mcp_status(
    State(runtime): State<Arc<Runtime>>,
    Query(query): Query<ClientQuery>,
) -> Result<Json<McpStatus>, HttpError> {
    let (_, config) = runtime.snapshot().await?;
    let client = config
        .clients
        .iter()
        .find(|c| c.name == query.client)
        .ok_or(ApiError::NOT_FOUND)?;
    let gateway = Arc::clone(
        &runtime
            .live
            .read()
            .map_err(|error| lock_error(&error))?
            .gateway,
    );

    let access = Access::new(client).map_err(|error| {
        tracing::debug!(
            event = "settings_boundary_failure",
            category = std::any::type_name_of_val(&error)
        );
        ApiError::INTERNAL
    })?;

    Ok(Json(gateway.commands.inspect(&access).await?))
}

async fn mcp_token(
    State(runtime): State<Arc<Runtime>>,
    Query(query): Query<ClientQuery>,
) -> Result<Json<Value>, HttpError> {
    let (_, config) = runtime.snapshot().await?;
    let client = config
        .clients
        .iter()
        .find(|c| {
            c.name == query.client
                && c.seerr_user.is_some()
                && !c
                    .permissions
                    .iter()
                    .any(|permission| matches!(permission.as_ref(), "admin" | "config:manage"))
        })
        .ok_or(ApiError::NOT_FOUND)?;

    Ok(Json(json!({"token":client.key.value()})))
}

async fn mcp_stats(
    State(runtime): State<Arc<Runtime>>,
    Query(query): Query<ClientQuery>,
) -> Result<Json<ToolStatsPage>, HttpError> {
    let (_, config) = runtime.snapshot().await?;

    if !config
        .clients
        .iter()
        .any(|client| client.name == query.client)
    {
        return Err(ApiError::NOT_FOUND.into());
    }

    let store = Arc::clone(&runtime.store);

    Ok(Json(
        tokio::task::spawn_blocking(move || store.tool_stats(&query.client))
            .await
            .map_err(ApiError::from)??,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AuditQuery {
    client: String,
    #[serde(default)]
    offset: usize,
    #[serde(default)]
    limit: Option<usize>,
}

async fn mcp_audit(
    State(runtime): State<Arc<Runtime>>,
    Query(query): Query<AuditQuery>,
) -> Result<Json<AuditPage>, HttpError> {
    let (_, config) = runtime.snapshot().await?;

    if !config.clients.iter().any(|c| c.name == query.client) {
        return Err(ApiError::NOT_FOUND.into());
    }

    let limit = query
        .limit
        .unwrap_or_else(|| 20.min(config.mcp.limits.max_page_items));

    if !(1..=config.mcp.limits.max_page_items).contains(&limit)
        || i64::try_from(query.offset).is_err()
    {
        return Err(ApiError::BAD_QUERY.into());
    }

    let store = Arc::clone(&runtime.store);

    Ok(Json(
        tokio::task::spawn_blocking(move || store.write_audit(&query.client, query.offset, limit))
            .await
            .map_err(ApiError::from)??,
    ))
}

fn revision_number(value: i64) -> Result<u64, ApiError> {
    u64::try_from(value).map_err(|error| {
        tracing::debug!(
            event = "settings_boundary_failure",
            category = std::any::type_name_of_val(&error)
        );
        ApiError::INTERNAL
    })
}

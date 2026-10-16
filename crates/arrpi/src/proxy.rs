//! HTTP authentication, admission and routing around the shared use cases.

use std::sync::Arc;

use arrpi_adapters::{build, cache::Pruner};
use arrpi_core::{
    Commands,
    config::{CachePolicy, Config, ConfigError, Feed},
    contracts::{Counters, SourceInfo, Status},
    executor::{CommandExecutor, Events, Executor},
    registry::{ToolDefinition, ToolId},
    source::{Body as SourceBody, SourceRead},
};
use async_trait::async_trait;
use axum::{
    Json, Router,
    body::Body,
    extract::{Path, RawQuery, State},
    http::{HeaderMap, HeaderValue, header},
    middleware::{self, Next},
    response::{IntoResponse as _, Response},
    routing::get,
};
use serde_json::Value;
use tokio::sync::Semaphore;

use crate::{
    auth::{self, Access},
    calendar,
    error::{ApiError, HttpError},
    query,
};

#[derive(Debug)]
pub(crate) struct Gateway {
    pub(crate) clients: Vec<Access>,
    pub(crate) oauth: Option<arrpi_adapters::oauth::Verifier>,
    pub(crate) policy: CachePolicy,
    pub(crate) feed: Feed,
    pub(crate) subscriptions: Option<Arc<crate::subscriptions::Store>>,
    pub(crate) commands: Commands,
    pub(crate) pruner: Pruner,
    proxy_host: Option<String>,
    pub(crate) requests: Arc<Semaphore>,
}

impl Gateway {
    pub(crate) fn new(
        config: &Config,
        clock: fn() -> tokio::time::Instant,
    ) -> Result<Self, ConfigError> {
        Self::with_state(config, clock, None, None, None)
    }

    pub(crate) fn with_state(
        config: &Config,
        clock: fn() -> tokio::time::Instant,
        events: Option<Events>,
        subscriptions: Option<Arc<crate::subscriptions::Store>>,
        proxy_host: Option<String>,
    ) -> Result<Self, ConfigError> {
        let clients = config
            .clients
            .iter()
            .map(Access::new)
            .collect::<Result<Vec<_>, _>>()?;

        for (index, client) in clients.iter().enumerate() {
            if clients
                .iter()
                .skip(index + 1)
                .any(|other| client.same_credential(other))
            {
                return Err(ConfigError::invalid("client credentials must be distinct"));
            }
        }

        let built = build(config, clock)?;

        Ok(Self {
            clients,
            oauth: arrpi_adapters::oauth::Verifier::new(config)?,
            policy: config.cache,
            feed: config.feed.clone(),
            subscriptions,
            commands: Commands::new(
                config,
                built.sources,
                arrpi_core::executor::Recording::new(events, clock),
                built.normalizations,
            ),
            pruner: built.pruner,
            proxy_host,
            requests: Arc::new(Semaphore::new(config.cache.max_requests)),
        })
    }

    pub(crate) async fn read(
        &self,
        source: &str,
        endpoint: &str,
        query: &str,
        mode: arrpi_core::source::ReadMode,
    ) -> Result<SourceRead<SourceBody>, ApiError> {
        self.commands
            .sources
            .source(source)
            .and_then(|source| source.native())
            .ok_or(ApiError::NOT_FOUND)?
            .read(endpoint, query, mode)
            .await
    }
}

#[derive(Debug)]
struct LocalExecutor {
    gateway: Arc<Gateway>,
}

#[async_trait]
impl CommandExecutor for LocalExecutor {
    async fn list(&self, credential: &str) -> Result<Vec<ToolDefinition>, ApiError> {
        self.gateway
            .commands
            .list(auth::authenticate_key(credential, &self.gateway.clients)?)
            .await
    }

    async fn execute(
        &self,
        credential: &str,
        tool: ToolId,
        arguments: Value,
    ) -> Result<Value, ApiError> {
        self.gateway
            .commands
            .execute(
                auth::authenticate_key(credential, &self.gateway.clients)?,
                tool,
                arguments,
            )
            .await
    }

    fn definition(&self, _credential: Option<&str>, tool: ToolId) -> Option<ToolDefinition> {
        Some(self.gateway.commands.definition(tool).clone())
    }
}

#[cfg(test)]
pub(crate) fn router(gateway: Gateway) -> Router {
    router_shared(Arc::new(gateway)).expect("valid test routing")
}

pub(crate) fn router_shared(gateway: Arc<Gateway>) -> Result<Router, ApiError> {
    let protected = Router::new()
        .route("/{source}/api/v3/{endpoint}", get(native))
        .route("/summaries/{kind}", get(summary))
        .route("/sources/{source}/summaries/{kind}", get(source_summary))
        .route("/calendar", get(calendar::combined))
        .route("/calendar.ics", get(crate::ical::export))
        .route("/calendar.rss", get(crate::rss::export));
    let protected = protected
        .route("/tracking", get(crate::tracking::status))
        .route(
            "/tracking/{action}",
            axum::routing::post(crate::tracking::action),
        );
    let protected = protected
        .route(
            "/subscription",
            axum::routing::post(crate::subscriptions::issue).delete(crate::subscriptions::revoke),
        )
        .route(
            "/subscription/rotate",
            axum::routing::post(crate::subscriptions::rotate),
        )
        .route("/tools", get(crate::commands::http_list))
        .route(
            "/tools/{name}",
            axum::routing::post(crate::commands::http_execute),
        )
        .route("/status", get(status))
        .fallback(|| async { HttpError(ApiError::NOT_FOUND) })
        .layer(middleware::from_fn_with_state(Arc::clone(&gateway), guard))
        .with_state(Arc::clone(&gateway));
    let mut router = Router::new().nest("/v1", protected);

    if gateway.commands.ready() {
        let executor = Executor::new(LocalExecutor {
            gateway: Arc::clone(&gateway),
        });

        // The process-level proxy authority is deployment metadata, never a requester credential.
        match arrpi_mcp::HttpEndpoint::new(
            executor,
            gateway.commands.policy.public_url.as_deref(),
            gateway.proxy_host.as_deref(),
        ) {
            Ok(endpoint) => {
                let mcp: Router = endpoint.into();

                router = router.merge(mcp.route_layer(middleware::from_fn_with_state(
                    Arc::clone(&gateway),
                    auth::oauth::guard,
                )));
            }

            Err(error) => {
                tracing::error!(event="mcp_router_invalid",category=%error);

                return Err(ApiError::BAD_QUERY);
            }
        }
    }

    if let Some(oauth) = &gateway.oauth {
        let document = oauth.metadata().clone();
        let root_document = document.clone();

        router = router
            .route(
                oauth.metadata_path(),
                get(move || async move { Json(document.clone()) }),
            )
            .route(
                "/.well-known/oauth-protected-resource",
                get(move || async move { Json(root_document.clone()) }),
            );
    }

    let public = Router::new()
        .route(
            "/health",
            get(|| async { Json(serde_json::json!({"status":"ok"})) }),
        )
        .route(
            "/feeds/{token}/{format}",
            get(crate::subscriptions::public_feed)
                .route_layer(middleware::from_fn_with_state(Arc::clone(&gateway), admit)),
        )
        .route(
            "/metrics",
            get(metrics).route_layer(middleware::from_fn_with_state(Arc::clone(&gateway), guard)),
        )
        .with_state(gateway);

    Ok(router.merge(public))
}

async fn guard(
    State(gateway): State<Arc<Gateway>>,
    mut request: axum::extract::Request,
    next: Next,
) -> Result<Response, HttpError> {
    if let Some(value) = request.headers_mut().get_mut(header::AUTHORIZATION) {
        value.set_sensitive(true);
    }

    if auth::authenticate(request.headers(), &gateway.clients).is_err() {
        if !matches!(request.uri().path(), "/calendar.rss" | "/calendar.ics") {
            return Err(ApiError::UNAUTHORIZED.into());
        }

        crate::subscriptions::grants(&gateway, request.headers()).await?;
    }

    admit(State(gateway), request, next).await
}

pub(crate) async fn admit(
    State(gateway): State<Arc<Gateway>>,
    request: axum::extract::Request,
    next: Next,
) -> Result<Response, HttpError> {
    let permit = Arc::clone(&gateway.requests)
        .try_acquire_owned()
        .map_err(ApiError::from)?;

    Ok(admit_reserved(permit, request, next).await)
}

pub(crate) async fn admit_reserved(
    permit: tokio::sync::OwnedSemaphorePermit,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    let response = next.run(request).await;
    let (mut parts, inner) = response.into_parts();

    parts
        .headers
        .insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    parts.headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    Response::from_parts(
        parts,
        Body::new(crate::transport::LeasedBody {
            inner,
            _permit: permit,
        }),
    )
}

async fn native(
    State(gateway): State<Arc<Gateway>>,
    Path((source, endpoint)): Path<(String, String)>,
    RawQuery(raw): RawQuery,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    let access = auth::authenticate(&headers, &gateway.clients)?;

    if !access.allows(&source, &endpoint) {
        return Err(ApiError::FORBIDDEN.into());
    }

    let query = query::canonical(&endpoint, raw.as_deref())?;
    let cached = gateway
        .read(&source, &endpoint, &query, query::read_mode(&headers))
        .await?;
    let freshness = cached.freshness;
    let mut response = Response::new(Body::from(bytes::Bytes::from(cached.data)));

    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    response.headers_mut().insert(
        "x-arrpi-cache",
        HeaderValue::from_static(match freshness.cache {
            arrpi_core::source::Observation::Hit => "hit",
            arrpi_core::source::Observation::Miss => "miss",
            arrpi_core::source::Observation::Stale => "stale",
            arrpi_core::source::Observation::NotCached => "not_cached",
        }),
    );
    response.headers_mut().insert(
        header::AGE,
        HeaderValue::from(freshness.age_seconds.unwrap_or(0)),
    );
    Ok(response)
}

async fn metrics(
    State(gateway): State<Arc<Gateway>>,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    if !auth::authenticate(&headers, &gateway.clients)?.observes_metrics() {
        return Err(ApiError::FORBIDDEN.into());
    }

    let aggregate = gateway
        .commands
        .sources
        .iter()
        .map(|(_, source)| source.counters())
        .fold(Counters::default(), |mut total, item| {
            total.hits = total.hits.saturating_add(item.hits);
            total.misses = total.misses.saturating_add(item.misses);
            total.stale = total.stale.saturating_add(item.stale);
            total.upstream_failures = total
                .upstream_failures
                .saturating_add(item.upstream_failures);
            total
        });

    Ok(([(header::CONTENT_TYPE,"text/plain; version=0.0.4; charset=utf-8"),(header::CACHE_CONTROL,"private, no-store")],format!("arrpi_cache_hits_total {}\narrpi_cache_misses_total {}\narrpi_cache_stale_total {}\narrpi_upstream_failures_total {}\n",aggregate.hits,aggregate.misses,aggregate.stale,aggregate.upstream_failures)).into_response())
}

async fn status(
    State(gateway): State<Arc<Gateway>>,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    let access = auth::authenticate(&headers, &gateway.clients)?;
    let sources = gateway
        .commands
        .sources
        .iter()
        .filter(|(name, source)| {
            access.visible(name)
                || (source.accounts().is_some()
                    && (access.allows("config", "manage")
                        || (access.binding.is_some()
                            && ["catalog", "requests", "issues"]
                                .iter()
                                .any(|operation| access.allows(operation, "view")))))
        })
        .map(|(name, source)| SourceInfo {
            name: name.to_owned(),
            kind: source.descriptor().kind.to_owned(),
            account: source.accounts().is_some(),
            tracking: source.tracking().is_some()
                && access
                    .tracking
                    .as_ref()
                    .is_some_and(|binding| binding.source == name)
                && access.allows(name, "tracking-view"),
            endpoints: source
                .descriptor()
                .endpoints
                .iter()
                .filter(|endpoint| access.allows(name, endpoint.name()))
                .copied()
                .collect(),
            calendar: source.calendar().is_some() && access.allows(name, "calendar-view"),
            summaries: arrpi_core::summaries::SummaryKind::ALL
                .iter()
                .filter(|kind| {
                    source
                        .summaries()
                        .is_some_and(|summary| summary.supports(**kind))
                        && access.allows(name, kind.grant())
                })
                .copied()
                .collect(),
            counters: source.counters(),
        })
        .collect();

    Ok((
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(Status {
            config_manage: access.allows("config", "manage"),
            version: env!("CARGO_PKG_VERSION"),
            cache: gateway.policy,
            sources,
        }),
    )
        .into_response())
}

async fn summary(
    State(gateway): State<Arc<Gateway>>,
    Path(kind): Path<arrpi_core::summaries::SummaryKind>,
    axum::extract::Query(mut args): axum::extract::Query<arrpi_core::arguments::Fresh>,
    headers: HeaderMap,
) -> Result<Json<arrpi_core::summaries::Summary>, HttpError> {
    let principal = auth::authenticate(&headers, &gateway.clients)?;

    args.no_cache |= query::read_mode(&headers) == arrpi_core::source::ReadMode::Fresh;

    Ok(Json(
        gateway.commands.summaries(principal, kind, args).await?,
    ))
}

async fn source_summary(
    State(gateway): State<Arc<Gateway>>,
    Path((source, kind)): Path<(String, arrpi_core::summaries::SummaryKind)>,
    axum::extract::Query(mut args): axum::extract::Query<arrpi_core::arguments::Fresh>,
    headers: HeaderMap,
) -> Result<Json<arrpi_core::summaries::Summary>, HttpError> {
    let principal = auth::authenticate(&headers, &gateway.clients)?;

    args.no_cache |= query::read_mode(&headers) == arrpi_core::source::ReadMode::Fresh;

    Ok(Json(
        gateway
            .commands
            .source_summary(principal, &source, kind, args)
            .await?,
    ))
}

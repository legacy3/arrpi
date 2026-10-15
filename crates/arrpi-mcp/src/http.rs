//! Stateless HTTP routing retains SDK origin and host enforcement.

use std::sync::Arc;

use arrpi_core::{config::ConfigError, executor::Executor};
use axum::{
    Json, Router,
    extract::Request,
    http::{StatusCode, header, uri::Authority},
    middleware::{self, Next},
    response::{IntoResponse as _, Response},
};
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};

use crate::handler::{Handler, bearer};

/// The exact MCP HTTP route, with protocol implementation details kept private.
#[derive(Clone, Debug)]
pub struct HttpEndpoint(Router);

impl HttpEndpoint {
    /// Bind an executor and narrowly extend the SDK's host/origin allowlists.
    ///
    /// # Errors
    /// Returns a redacted configuration error for an invalid public endpoint or proxy authority.
    pub fn new(
        executor: Executor,
        public_url: Option<&str>,
        proxy_host: Option<&str>,
    ) -> Result<Self, ConfigError> {
        let mut config = StreamableHttpServerConfig::default().enforce_origin_validation();

        config.legacy_session_mode = false;
        config.json_response = true;
        config.max_request_body_bytes = 128 * 1024;
        configure_public(&mut config, public_url)?;

        if let Some(host) = proxy_host {
            config.allowed_hosts.push(proxy_authority(host)?);
        }

        let service = StreamableHttpService::new(
            move || Ok(Handler::http(executor.clone())),
            Arc::new(LocalSessionManager::default()),
            config,
        );

        Ok(Self(
            Router::new()
                .route_service("/v1/mcp", service)
                .layer(middleware::from_fn(require_bearer)),
        ))
    }
}

impl From<HttpEndpoint> for Router {
    fn from(endpoint: HttpEndpoint) -> Self {
        endpoint.0
    }
}

fn configure_public(
    config: &mut StreamableHttpServerConfig,
    value: Option<&str>,
) -> Result<(), ConfigError> {
    let Some(value) = value else {
        return Ok(());
    };
    let endpoint = arrpi_core::config::https_endpoint(value, "/v1/mcp").map_err(|error| {
        tracing::debug!(event = "invalid_mcp_endpoint", reason = error.reason());
        ConfigError::invalid("invalid MCP public endpoint")
    })?;

    config.allowed_hosts.push(endpoint.authority().to_owned());
    config.allowed_origins.push(endpoint.origin().to_owned());
    Ok(())
}

fn proxy_authority(value: &str) -> Result<String, ConfigError> {
    let authority: Authority = value.parse().map_err(|error| {
        tracing::debug!(event = "mcp_proxy_authority", category = ?error);
        ConfigError::invalid("invalid MCP proxy authority")
    })?;

    if value.trim() != value
        || value
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || b"@/?#*\\".contains(&byte))
        || authority.host().is_empty()
        || authority.port_u16().is_none()
    {
        return Err(ConfigError::invalid(
            "MCP proxy authority requires an exact host and numeric port",
        ));
    }

    Ok(authority.to_string())
}

async fn require_bearer(request: Request, next: Next) -> Response {
    if request
        .extensions()
        .get::<crate::handler::AuthenticatedCredential>()
        .is_some()
    {
        return next.run(request).await;
    }

    match bearer(request.headers()) {
        Ok(_) => next.run(request).await,

        Err(error) => (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Bearer")],
            Json(error.payload()),
        )
            .into_response(),
    }
}

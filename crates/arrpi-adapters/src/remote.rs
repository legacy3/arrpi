//! Credential-free remote clients; every operation receives its caller's key.
use crate::http::{Http, HttpOptions, Intent, bounded, transport_error};
use arrpi_core::{
    config::{ConfigError, base_url},
    error::ApiError,
    executor::CommandExecutor,
    registry::{ToolDefinition, ToolId},
};
use async_trait::async_trait;
use reqwest::{Method, StatusCode};
use serde::Deserialize;
use serde_json::{Map, Value};
use sha2::{Digest as _, Sha256};
use std::{sync::RwLock, time::Duration};
use url::Url;

const MAX_RESPONSE: usize = 8 * 1024 * 1024;

/// Remote MCP command execution without a stored or fallback credential.
#[derive(Debug)]
pub struct RemoteApi {
    api: ApiClient,
    discovery: RwLock<Option<Discovery>>,
}

struct Discovery {
    caller: [u8; 32],
    tools: Vec<ToolDefinition>,
}

impl std::fmt::Debug for Discovery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Discovery")
            .field("tools", &self.tools.len())
            .finish_non_exhaustive()
    }
}

/// Bounded CLI API transport without native upstream credentials or stdout.
pub struct ApiClient {
    base: Url,
    http: Http,
    no_cache: bool,
}

/// Native JSON plus optional cache provenance returned by the central API.
pub struct ApiResponse {
    pub data: Value,
    pub cache_state: Option<String>,
    pub age_seconds: Option<u64>,
}

struct Operation<'a> {
    method: Method,
    path: &'a str,
    query: &'a str,
    body: Option<&'a [u8]>,
    intent: Intent,
}

#[derive(Debug, Deserialize)]
struct ToolList {
    tools: Vec<RemoteTool>,
}

#[derive(Debug, Deserialize)]
struct RemoteTool {
    name: String,
    description: Option<String>,
    #[serde(rename = "inputSchema")]
    input_schema: Map<String, Value>,
}

impl RemoteApi {
    /// Connect over HTTPS, or plaintext HTTP for loopback only.
    ///
    /// # Errors
    /// Rejects unsafe URLs and invalid transport configuration.
    pub fn new(url: &str, no_cache: bool) -> Result<Self, ConfigError> {
        let api = ApiClient::new(url, no_cache)?;
        let loopback = match api.base.host() {
            Some(url::Host::Domain("localhost")) => true,
            Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
            Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
            _ => false,
        };

        if api.base.scheme() != "https" && !loopback {
            return Err(ConfigError::invalid("MCP needs HTTPS except for loopback"));
        }

        Ok(Self {
            api,
            discovery: RwLock::new(None),
        })
    }
}

impl ApiClient {
    /// Use a configured HTTP(S) server URL, including private-network HTTP.
    ///
    /// # Errors
    /// Rejects credential-bearing URLs, queries and fragments.
    pub fn new(url: &str, no_cache: bool) -> Result<Self, ConfigError> {
        Ok(Self {
            base: base_url(url)?.into(),
            http: Http::new(HttpOptions {
                timeout: Duration::from_mins(1),
                connect_timeout: Duration::from_secs(3),
                pool_idle_per_host: 4,
            })?,
            no_cache,
        })
    }

    /// Read one central API resource without exposing any mutation method.
    ///
    /// # Errors
    /// Rejects unsafe relative paths, invalid credentials and upstream failures.
    pub async fn get(
        &self,
        credential: Option<&str>,
        path: &str,
        query: &str,
    ) -> Result<ApiResponse, ApiError> {
        self.send(
            credential,
            Operation {
                method: Method::GET,
                path,
                query,
                body: None,
                intent: Intent::Read,
            },
        )
        .await
    }

    /// Apply a complete revisioned settings document through the admin API.
    ///
    /// # Errors
    /// Returns argument/policy failures or indeterminate dispatched updates.
    pub async fn put_config(&self, credential: &str, body: &[u8]) -> Result<ApiResponse, ApiError> {
        if body.len() > 128 * 1024 {
            return Err(ApiError::BAD_QUERY);
        }

        self.send(
            Some(credential),
            Operation {
                method: Method::PUT,
                path: "v1/config",
                query: "",
                body: Some(body),
                intent: Intent::Write,
            },
        )
        .await
    }

    async fn send(
        &self,
        credential: Option<&str>,
        operation: Operation<'_>,
    ) -> Result<ApiResponse, ApiError> {
        let Operation {
            method,
            path,
            query,
            body,
            intent,
        } = operation;
        let mut url = self.base.join(path).map_err(|error| {
            tracing::debug!(
                event = "remote_path_invalid",
                category = std::any::type_name_of_val(&error)
            );
            ApiError::BAD_QUERY
        })?;

        if url.origin() != self.base.origin()
            || !url.path().starts_with(self.base.path())
            || url.query().is_some()
            || url.fragment().is_some()
            || path.starts_with('/')
        {
            return Err(ApiError::BAD_QUERY);
        }

        url.set_query((!query.is_empty()).then_some(query));
        let mut request = self
            .http
            .client()
            .request(method, url)
            .header("Accept", "application/json");

        if let Some(credential) = credential {
            validate_key(credential)?;
            request = request.bearer_auth(credential);
        }

        if self.no_cache {
            request = request.header("Cache-Control", "no-cache");
        }

        if let Some(body) = body {
            request = request
                .header("Content-Type", "application/json")
                .body(body.to_vec());
        }

        let response = request
            .send()
            .await
            .map_err(|error| intent.failure(transport_error(&error)))?;
        let status = response.status();
        let cache_state = response
            .headers()
            .get("x-arrpi-cache")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let age_seconds = response
            .headers()
            .get("age")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse().ok());
        let bytes = bounded(response, MAX_RESPONSE, intent, None)
            .await
            .map_err(|error| {
                if error == ApiError::RESPONSE_TOO_LARGE {
                    intent.failure(ApiError::INVALID_RESPONSE)
                } else {
                    error
                }
            })?;
        let value = serde_json::from_slice::<Value>(&bytes);

        if !status.is_success() {
            return Err(value
                .as_ref()
                .ok()
                .and_then(ApiError::from_payload)
                .unwrap_or_else(|| fallback(status, intent)));
        }

        if status != StatusCode::OK {
            return Err(intent.failure(ApiError::INVALID_RESPONSE));
        }

        let data = value.map_err(|error| {
            tracing::debug!(
                event = "remote_response_json",
                category = std::any::type_name_of_val(&error)
            );
            intent.failure(ApiError::INVALID_RESPONSE)
        })?;

        Ok(ApiResponse {
            data,
            cache_state,
            age_seconds,
        })
    }
}

fn fallback(status: StatusCode, intent: Intent) -> ApiError {
    let error = match status.as_u16() {
        401 => ApiError::UNAUTHORIZED,
        400 => ApiError::BAD_QUERY,
        _ => ApiError::upstream_read(status.as_u16()),
    };

    intent.failure(error)
}

fn validate_key(key: &str) -> Result<(), ApiError> {
    if key.is_empty() || key.len() > 128 || !key.bytes().all(|byte| (0x21..=0x7e).contains(&byte)) {
        Err(ApiError::UNAUTHORIZED)
    } else {
        Ok(())
    }
}

#[async_trait]
impl CommandExecutor for RemoteApi {
    async fn list(&self, credential: &str) -> Result<Vec<ToolDefinition>, ApiError> {
        *self.discovery.write().map_err(|error| {
            tracing::debug!(event = "remote_discovery_lock", category = %error);
            ApiError::INTERNAL
        })? = None;
        let response = self.api.get(Some(credential), "v1/tools", "").await?;
        let list: ToolList = serde_json::from_value(response.data).map_err(|error| {
            tracing::debug!(
                event = "remote_tools_invalid",
                category = std::any::type_name_of_val(&error)
            );
            ApiError::INVALID_RESPONSE
        })?;

        let mut tools = Vec::with_capacity(list.tools.len().min(ToolId::ALL.len()));

        for tool in list.tools {
            if let Some(id) = ToolId::parse(&tool.name) {
                if tools
                    .iter()
                    .any(|known: &ToolDefinition| known.name == tool.name)
                {
                    return Err(ApiError::INVALID_RESPONSE);
                }

                tools.push(ToolDefinition::remote(
                    id,
                    tool.description,
                    tool.input_schema.into(),
                ));
            }
        }

        *self.discovery.write().map_err(|error| {
            tracing::debug!(event = "remote_discovery_lock", category = %error);
            ApiError::INTERNAL
        })? = Some(Discovery {
            caller: Sha256::digest(credential.as_bytes()).into(),
            tools: tools.clone(),
        });
        Ok(tools)
    }

    async fn execute(
        &self,
        credential: &str,
        tool: ToolId,
        mut arguments: Value,
    ) -> Result<Value, ApiError> {
        if self.api.no_cache && tool.accepts_fresh() {
            arguments
                .as_object_mut()
                .ok_or(ApiError::BAD_QUERY)?
                .insert("no_cache".to_owned(), Value::Bool(true));
        }

        let body = serde_json::to_vec(&arguments).map_err(|error| {
            tracing::debug!(
                event = "remote_arguments_invalid",
                category = std::any::type_name_of_val(&error)
            );
            ApiError::BAD_QUERY
        })?;
        let intent = if tool.is_write() {
            Intent::Write
        } else {
            Intent::Read
        };
        let response = self
            .api
            .send(
                Some(credential),
                Operation {
                    method: Method::POST,
                    path: &format!("v1/tools/{}", tool.name()),
                    query: "",
                    body: Some(&body),
                    intent,
                },
            )
            .await?;

        if !response.data.is_object() {
            return Err(intent.failure(ApiError::INVALID_RESPONSE));
        }

        Ok(response.data)
    }

    fn definition(&self, credential: Option<&str>, tool: ToolId) -> Option<ToolDefinition> {
        let caller: [u8; 32] = Sha256::digest(credential?.as_bytes()).into();
        let discovery = self.discovery.read().ok()?;
        let discovery = discovery
            .as_ref()
            .filter(|snapshot| snapshot.caller == caller)?;

        discovery
            .tools
            .iter()
            .find(|definition| definition.name == tool.name())
            .cloned()
    }
}

impl std::fmt::Debug for Operation<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Operation")
            .field("method", &self.method)
            .field("intent", &self.intent)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for ApiClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiClient")
            .field("no_cache", &self.no_cache)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for ApiResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiResponse")
            .field("cache_state", &self.cache_state)
            .field("age_seconds", &self.age_seconds)
            .finish_non_exhaustive()
    }
}

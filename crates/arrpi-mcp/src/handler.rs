//! SDK translation preserves shared schemas and each transport's verified credential.

use arrpi_core::{
    error::ApiError,
    executor::Executor,
    registry::{ToolDefinition, ToolId},
    secret::Secret,
};
use axum::http::{HeaderMap, header, request::Parts};
use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ErrorCode, ListToolsResult,
        PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool, ToolAnnotations,
    },
    service::RequestContext,
};
use serde_json::Value;

/// A credential resolved by trusted HTTP authentication, never from an identity header.
#[derive(Clone, Debug)]
pub struct AuthenticatedCredential(Secret);

impl AuthenticatedCredential {
    /// Inject only after verification and explicit binding to a current local account.
    #[must_use]
    pub const fn new(credential: Secret) -> Self {
        Self(credential)
    }
}

#[derive(Clone, Debug)]
enum Credential {
    Http,
    Stdio(Secret),
}

#[derive(Clone)]
pub(crate) struct Handler {
    executor: Executor,
    credential: Credential,
}

impl Handler {
    pub(crate) const fn http(executor: Executor) -> Self {
        Self {
            executor,
            credential: Credential::Http,
        }
    }

    pub(crate) const fn stdio(executor: Executor, credential: Secret) -> Self {
        Self {
            executor,
            credential: Credential::Stdio(credential),
        }
    }

    fn credential<'a>(
        &'a self,
        context: &'a RequestContext<RoleServer>,
    ) -> Result<&'a str, ApiError> {
        match &self.credential {
            Credential::Stdio(secret) => Ok(secret.value()),

            Credential::Http => {
                let parts = context
                    .extensions
                    .get::<Parts>()
                    .ok_or(ApiError::UNAUTHORIZED)?;

                if let Some(credential) = parts.extensions.get::<AuthenticatedCredential>() {
                    return Ok(credential.0.value());
                }

                bearer(&parts.headers)
            }
        }
    }
}

impl ServerHandler for Handler {
    fn get_info(&self) -> ServerConfig {
        let mut info = ServerConfig::default();

        "arrpi".clone_into(&mut info.server_info.name);
        env!("CARGO_PKG_VERSION").clone_into(&mut info.server_info.version);
        let instance = self.executor.instance();

        info.server_info.title = Some(format!("ArrPi for {}", instance.title));
        info.server_info.description = Some(format!(
            "Media library, requests and maintenance for {}.",
            instance.title
        ));
        info.server_info.website_url = Some(instance.link);
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info.instructions = Some(format!(
            "This is ArrPi for {}. Browse the installed library or search Seerr's catalog by exact TMDB candidates. Requests use your bound account and never request 4K; approval is not download completion. Use no_cache:true for fresh source reads; Seerr's Plex sync may still lag. Paginate using next_offset and check source completeness. request_profiles lists permitted non-4K choices; omit profile to retain Seerr's default. Batches retain the same account permissions. Cancel only owned pending requests and close only owned issues. Reconcile unknown_outcome through media_details, request_status and my_requests before retrying. For existing media, refresh_media updates metadata and scans files; wait with media_command_status, inspect series_episodes, then use search_releases to find missing releases. Queued is acceptance, not completion. Report successful completion only after checking status.",
            instance.title
        ));
        info
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let credential = self.credential(&context).map_err(protocol_error)?;
        let tools = self
            .executor
            .list(credential)
            .await
            .map_err(protocol_error)?;

        Ok(ListToolsResult {
            tools: tools.into_iter().map(sdk_tool).collect(),
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let credential = self.credential(&context).map_err(protocol_error)?;
        let tool = ToolId::parse(&request.name).ok_or_else(|| {
            ErrorData::invalid_params("unknown tool", Some(ApiError::NOT_FOUND.payload()))
        })?;
        let result = self
            .executor
            .execute(
                credential,
                tool,
                Value::Object(request.arguments.unwrap_or_default()),
            )
            .await;

        if let Err(error) = result
            && error == ApiError::UNAUTHORIZED
        {
            return Err(protocol_error(error));
        }

        Ok(match result {
            Ok(value) => CallToolResult::structured(value),
            Err(error) => CallToolResult::structured_error(error.payload()),
        }
        .into())
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        let credential = match &self.credential {
            Credential::Http => None,
            Credential::Stdio(secret) => Some(secret.value()),
        };

        self.executor
            .definition(credential, ToolId::parse(name)?)
            .map(sdk_tool)
    }
}

pub(crate) fn bearer(headers: &HeaderMap) -> Result<&str, ApiError> {
    let mut values = headers.get_all(header::AUTHORIZATION).iter();
    let raw = values
        .next()
        .and_then(|value| value.to_str().ok())
        .ok_or(ApiError::UNAUTHORIZED)?;
    let (scheme, value) = raw.split_once(' ').ok_or(ApiError::UNAUTHORIZED)?;

    if !scheme.eq_ignore_ascii_case("Bearer") || !valid_credential(value) {
        return Err(ApiError::UNAUTHORIZED);
    }

    if values.next().is_some() {
        return Err(ApiError::UNAUTHORIZED);
    }

    Ok(value)
}

pub(crate) use arrpi_core::secret::valid_client_key as valid_credential;

/// Render a shared operation using the MCP SDK's protocol model.
#[must_use]
pub fn sdk_tool(definition: ToolDefinition) -> Tool {
    let schema: std::sync::Arc<rmcp::model::JsonObject> = definition.input_schema().clone().into();
    let read = definition.effects.is_read_only();
    let annotations = ToolAnnotations::new()
        .read_only(read)
        .destructive(definition.effects.is_destructive())
        .idempotent(read)
        .open_world(true);

    Tool::new(definition.name, definition.description, schema)
        .with_title(definition.title)
        .with_annotations(annotations)
}

fn protocol_error(error: ApiError) -> ErrorData {
    if matches!(error, ApiError::UNAUTHORIZED | ApiError::FORBIDDEN) {
        ErrorData::new(ErrorCode(-32001), error.to_string(), Some(error.payload()))
    } else {
        ErrorData::internal_error(error.to_string(), Some(error.payload()))
    }
}

impl std::fmt::Debug for Handler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpHandler").finish_non_exhaustive()
    }
}

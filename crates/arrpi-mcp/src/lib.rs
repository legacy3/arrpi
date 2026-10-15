//! MCP transports over an injected, protocol-neutral command executor.

mod handler;
mod http;

#[cfg(test)]
mod tests;

use arrpi_core::{error::ApiError, executor::Executor, secret::Secret};
use handler::{Handler, valid_credential};
use rmcp::ServiceExt as _;

pub use handler::{AuthenticatedCredential, sdk_tool as describe};
pub use http::HttpEndpoint;

/// Serve stdio with an explicit client credential through the shared command executor.
///
/// # Errors
/// Returns a redacted authentication or transport error; stdout contains protocol frames only.
pub async fn serve_stdio(executor: Executor, credential: Secret) -> Result<(), ApiError> {
    if !valid_credential(credential.value()) {
        return Err(ApiError::UNAUTHORIZED);
    }

    executor.list(credential.value()).await?;

    let server = Handler::stdio(executor, credential)
        .serve(rmcp::transport::stdio())
        .await
        .map_err(|error| {
            tracing::debug!(
                event = "mcp_stdio_start",
                category = std::any::type_name_of_val(&error)
            );
            ApiError::UNAVAILABLE
        })?;

    server.waiting().await.map_err(|error| {
        tracing::debug!(
            event = "mcp_stdio_wait",
            category = std::any::type_name_of_val(&error)
        );
        ApiError::INTERRUPTED
    })?;
    Ok(())
}

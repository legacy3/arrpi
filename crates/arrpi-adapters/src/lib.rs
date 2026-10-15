//! Native source adapters and bounded persistence, independent of app routing and MCP.

pub mod budget;
pub mod cache;
mod discovery;
pub use discovery::Probe;
mod factory;
pub mod http;
pub mod metrics;
mod monitoring;
mod native;
pub mod oauth;
pub mod remote;
pub mod seerr;
pub mod storage;

pub use factory::{BuiltSources, build};

#[cfg(test)]
mod tests;

pub(crate) fn credential(
    value: &str,
) -> Result<secrecy::SecretString, arrpi_core::config::ConfigError> {
    if value.is_empty()
        || value.len() > 128
        || !value.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
    {
        return Err(arrpi_core::config::ConfigError::CREDENTIAL);
    }

    Ok(value.to_owned().into())
}

/// Probe common media ports on 1-64 explicitly selected IP addresses, without credentials.
///
/// # Errors
/// Rejects unsupported schemes, excessive target sets or unavailable local transport.
pub async fn discover(
    addresses: &[std::net::IpAddr],
    scheme: &str,
    clock: fn() -> std::time::Instant,
) -> Result<DiscoveryReport, arrpi_core::config::ConfigError> {
    discovery::discover(addresses, scheme, clock).await
}

/// Bounded credential-free discovery results; unidentified services stay explicit.
#[derive(Debug, serde::Serialize)]
pub struct DiscoveryReport {
    pub complete: bool,
    pub probed: usize,
    pub expected_probes: usize,
    pub results: Vec<Probe>,
}

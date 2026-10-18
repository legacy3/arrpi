//! MCP exposure and explicit Seerr actor bindings, persisted with all other settings.
use crate::config::ConfigError;
use serde::{Deserialize, Serialize};

/// Global command availability and execution bounds.
#[derive(Clone, Debug, Default, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
#[schemars(rename = "McpPolicy")]
pub struct Policy {
    pub enabled: bool,
    pub public_url: Option<String>,
    pub disabled_tools: Vec<Box<str>>,
    pub limits: Limits,
}

/// Configured pagination, batch, write admission and audit retention limits.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Limits {
    #[schemars(range(min = 1, max = 100_000))]
    pub audit_max_entries: usize,
    #[schemars(range(min = 1, max = 100))]
    pub max_batch_items: usize,
    #[schemars(range(min = 1, max = 300))]
    pub batch_timeout_seconds: u64,
    #[schemars(range(min = 1, max = 500))]
    pub max_page_items: usize,
    #[schemars(range(min = 1, max = 10_000))]
    pub max_search_episodes: usize,
    #[schemars(range(min = 1, max = 32))]
    pub max_concurrent_writes: usize,
    #[schemars(range(min = 1, max = 300))]
    pub queue_timeout_seconds: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            audit_max_entries: 10_000,
            max_batch_items: 20,
            batch_timeout_seconds: 45,
            max_page_items: 100,
            max_search_episodes: 1000,
            max_concurrent_writes: 4,
            queue_timeout_seconds: 15,
        }
    }
}

/// Verified Seerr and Plex identities for one `ArrPi` client.
#[derive(Clone, Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub id: u64,
    pub plex_id: u64,
}

impl Policy {
    /// Validate MCP limits, public endpoint and registered tool switches.
    ///
    /// # Errors
    /// Returns a redacted error for invalid bounds, endpoints or tool names.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if !(1..=100_000).contains(&self.limits.audit_max_entries)
            || !(1..=100).contains(&self.limits.max_batch_items)
            || !(1..=300).contains(&self.limits.batch_timeout_seconds)
            || !(1..=500).contains(&self.limits.max_page_items)
            || !(1..=10_000).contains(&self.limits.max_search_episodes)
            || !(1..=32).contains(&self.limits.max_concurrent_writes)
            || !(1..=300).contains(&self.limits.queue_timeout_seconds)
        {
            return Err(ConfigError::invalid("invalid MCP limits"));
        }

        if let Some(value) = &self.public_url {
            crate::config::https_endpoint(value, "/v1/mcp").map_err(|error| {
                tracing::debug!(event = "invalid_command_endpoint", reason = error.reason());
                ConfigError::invalid("MCP public URL must be an HTTPS /v1/mcp endpoint")
            })?;
        }

        let unique = self
            .disabled_tools
            .iter()
            .collect::<std::collections::BTreeSet<_>>();

        if unique.len() != self.disabled_tools.len()
            || unique
                .iter()
                .any(|name| crate::registry::ToolId::parse(name).is_none())
        {
            return Err(ConfigError::invalid(
                "unknown or duplicate disabled MCP tool",
            ));
        }

        Ok(())
    }
}

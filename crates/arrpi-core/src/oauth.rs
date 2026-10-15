//! Explicit external OAuth identities mapped to existing requester accounts.

use serde::{Deserialize, Serialize};

use crate::config::{Config, ConfigError};

/// Optional HTTP MCP resource-server policy. The external server owns login and tokens.
#[derive(Clone, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "OAuthPolicy")]
pub struct Policy {
    #[serde(default)]
    pub enabled: bool,
    pub issuer: String,
    #[serde(default = "scope")]
    pub required_scope: String,
    #[serde(default)]
    pub bindings: Vec<Binding>,
}

/// An administrator-approved identity, never inferred from email or token roles.
#[derive(Clone, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "OAuthIdentityBinding")]
pub struct Binding {
    pub issuer: String,
    pub subject: String,
    pub client: String,
}

impl Policy {
    /// Validate identities against the same settings revision as local grants.
    ///
    /// # Errors
    /// Rejects untrusted origins, ambiguous subjects and privileged or unbound accounts.
    pub fn validate(&self, config: &Config) -> Result<(), ConfigError> {
        validate_issuer(&self.issuer)?;

        validate_scope(&self.required_scope, self.bindings.len())?;

        if self.enabled
            && (!config.mcp.enabled || config.mcp.public_url.is_none() || config.seerr.is_none())
        {
            return Err(ConfigError::invalid(
                "OAuth requires configured Seerr, enabled MCP and its public HTTPS endpoint",
            ));
        }

        let mut subjects = std::collections::BTreeSet::new();

        for binding in &self.bindings {
            if binding.issuer != self.issuer
                || binding.subject.is_empty()
                || binding.subject.len() > 256
                || binding.subject.chars().any(char::is_control)
                || !subjects.insert((&binding.issuer, &binding.subject))
            {
                return Err(ConfigError::invalid(
                    "invalid or duplicate OAuth identity binding",
                ));
            }

            validate_client(binding, config)?;
        }

        Ok(())
    }
}

impl std::fmt::Debug for Policy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OAuthPolicy")
            .field("enabled", &self.enabled)
            .field("bindings", &self.bindings.len())
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for Binding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OAuthBinding { [REDACTED] }")
    }
}

fn scope() -> String {
    "arrpi:mcp".to_owned()
}

fn validate_issuer(value: &str) -> Result<(), ConfigError> {
    let issuer = url::Url::parse(value).map_err(|error| {
        tracing::debug!(
            event = "oauth_issuer",
            category = std::any::type_name_of_val(&error)
        );
        ConfigError::invalid("OAuth issuer needs an absolute HTTPS URL")
    })?;

    let loopback = matches!(issuer.host_str(), Some("127.0.0.1" | "[::1]" | "localhost"));

    if !(issuer.scheme() == "https" || issuer.scheme() == "http" && loopback)
        || issuer.host_str().is_none()
        || !issuer.username().is_empty()
        || issuer.password().is_some()
        || issuer.query().is_some()
        || issuer.fragment().is_some()
        || value.len() > 2048
        || value.trim() != value
        || !value.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
        || !value.starts_with(&format!("{}://", issuer.scheme()))
    {
        return Err(ConfigError::invalid(
            "OAuth issuer needs an exact HTTPS URL",
        ));
    }

    Ok(())
}

fn validate_scope(value: &str, bindings: usize) -> Result<(), ConfigError> {
    if bindings > 64
        || value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| (0x21..=0x7e).contains(&byte) && !matches!(byte, b'"' | b'\\'))
    {
        return Err(ConfigError::invalid("invalid OAuth scope or binding limit"));
    }

    Ok(())
}

fn validate_client(binding: &Binding, config: &Config) -> Result<(), ConfigError> {
    let client = config
        .clients
        .iter()
        .find(|client| client.name == binding.client)
        .ok_or_else(|| ConfigError::invalid("OAuth identity references an unknown client"))?;

    if client.seerr_user.is_none()
        || client
            .permissions
            .iter()
            .any(|permission| matches!(permission.as_ref(), "admin" | "config:manage"))
    {
        return Err(ConfigError::invalid(
            "OAuth requires a bound requester without configuration access",
        ));
    }

    Ok(())
}

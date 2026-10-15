//! Library-backed external OAuth verification with explicit local account binding.

use std::{sync::Arc, time::Duration};

use arrpi_core::{
    config::{Config, ConfigError},
    oauth::Policy,
};
use oauth_resource_server::{KeyNaming, OAuthConfig, OAuthValidator, TokenRejection};
use serde_json::Value;

/// One immutable settings revision's validator and approved subject mappings.
#[derive(Clone)]
pub struct Verifier {
    validator: Arc<OAuthValidator>,
    policy: Policy,
}

/// A protocol refusal without token contents, native URLs or validation internals.
#[derive(Debug)]
pub struct Refusal {
    pub status: u16,
    pub challenge: Option<String>,
}

/// A revision-owned key refresh task; replacing settings stops the old poller.
#[derive(Debug)]
pub struct Maintenance(tokio::task::JoinHandle<()>);

impl Drop for Maintenance {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl Verifier {
    /// Retain verified signing keys when only local account settings changed.
    pub fn reuse_keys(&mut self, previous: &Self) {
        if self.policy.issuer == previous.policy.issuer
            && self.policy.required_scope == previous.policy.required_scope
            && self.metadata()["resource"] == previous.metadata()["resource"]
        {
            self.validator = Arc::clone(&previous.validator);
        }
    }

    /// Refresh signing keys periodically for this live settings revision.
    ///
    /// # Errors
    /// Returns a redacted initialization failure outside an async runtime.
    pub fn maintain(&self) -> Result<Maintenance, arrpi_core::error::ApiError> {
        tokio::runtime::Handle::try_current().map_err(|error| {
            tracing::debug!(
                event = "oauth_runtime",
                category = std::any::type_name_of_val(&error)
            );
            arrpi_core::error::ApiError::INTERNAL
        })?;
        Ok(Maintenance(self.validator.spawn_background_refresh()))
    }

    /// Build without network I/O; discovery and signing keys are fetched on first use.
    ///
    /// # Errors
    /// Rejects invalid policy or unavailable cryptographic/TLS initialization.
    pub fn new(config: &Config) -> Result<Option<Self>, ConfigError> {
        let Some(policy) = config.oauth.as_ref().filter(|policy| policy.enabled) else {
            return Ok(None);
        };

        policy.validate(config)?;
        let resource = config
            .mcp
            .public_url
            .as_deref()
            .ok_or_else(|| ConfigError::invalid("OAuth needs the MCP public endpoint"))?;
        let resolved = OAuthConfig {
            enabled: true,
            issuer: policy.issuer.clone(),
            audience: resource.to_owned(),
            resource: resource.to_owned(),
            required_scope: Some(policy.required_scope.clone()),
            require_at_jwt: true,
            leeway_secs: 15,
            max_token_age_secs: Some(3600),
            ..OAuthConfig::default()
        }
        .resolve(KeyNaming::Dotted("oauth"))
        .map_err(|error| {
            tracing::debug!(
                event = "oauth_policy",
                category = std::any::type_name_of_val(&error)
            );
            ConfigError::invalid("invalid OAuth resource-server policy")
        })?
        .ok_or_else(|| ConfigError::invalid("OAuth resource server is disabled"))?;
        let validator = OAuthValidator::builder(&resolved)
            .fetch_timeout(Duration::from_secs(5))
            .build()
            .map_err(|error| {
                tracing::debug!(
                    event = "oauth_verifier",
                    category = std::any::type_name_of_val(&error)
                );
                ConfigError::invalid("cannot initialize OAuth verifier")
            })?;

        Ok(Some(Self {
            validator: Arc::new(validator),
            policy: policy.clone(),
        }))
    }

    /// Verify an access token, then resolve only a preapproved issuer/subject tuple.
    ///
    /// # Errors
    /// Returns an OAuth challenge for invalid, expired, unscoped or unbound tokens.
    pub async fn client<'a>(&'a self, token: Option<&str>) -> Result<&'a str, Refusal> {
        let token = token.ok_or_else(|| self.refusal(&TokenRejection::Missing))?;
        let verified = self
            .validator
            .validate(token)
            .await
            .map_err(|rejection| self.refusal(&rejection))?;
        let subject = verified
            .subject
            .as_deref()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                self.refusal(&TokenRejection::Invalid(
                    oauth_resource_server::InvalidToken::new(
                        oauth_resource_server::InvalidTokenKind::MissingClaim,
                        "subject required",
                    ),
                ))
            })?;
        let binding = self
            .policy
            .bindings
            .iter()
            .find(|binding| binding.issuer == verified.issuer && subject == binding.subject);

        binding
            .map(|binding| binding.client.as_str())
            .ok_or(Refusal {
                status: 403,
                challenge: None,
            })
    }

    /// Public RFC 9728 metadata produced by the same library that builds challenges.
    #[must_use]
    pub fn metadata(&self) -> &Value {
        self.validator.metadata()
    }

    /// The RFC 9728 path includes the canonical MCP resource path.
    #[must_use]
    pub fn metadata_path(&self) -> &str {
        self.validator.metadata_path()
    }

    /// Reject malformed Authorization headers with the discoverable OAuth challenge.
    #[must_use]
    pub fn missing(&self) -> Refusal {
        self.refusal(&TokenRejection::Missing)
    }

    fn refusal(&self, rejection: &TokenRejection) -> Refusal {
        let reason = match rejection {
            TokenRejection::Missing => "missing",
            TokenRejection::Invalid(invalid) => invalid.kind().as_str(),
            TokenRejection::InsufficientScope => "insufficient_scope",
            _ => "other",
        };

        tracing::debug!(event = "oauth_refused", reason);
        let refusal = oauth_resource_server::refusal(rejection, Some(&self.validator));

        Refusal {
            status: refusal.status,
            challenge: refusal.www_authenticate,
        }
    }
}

impl std::fmt::Debug for Verifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OAuthVerifier").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests;

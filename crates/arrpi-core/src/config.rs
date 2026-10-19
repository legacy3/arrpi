//! Validated settings persisted in the private application database.

use std::{collections::BTreeMap, net::SocketAddr};

use crate::contracts::Endpoint;
use serde::{Deserialize, Serialize};
use url::Url;

/// Protected application settings stored in the single private `SQLite` document.
#[derive(Clone, Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "SettingsDocument")]
pub struct Config {
    pub listen: SocketAddr,
    #[serde(default)]
    pub cache: CachePolicy,
    #[serde(default)]
    pub instance: Instance,
    #[serde(default)]
    pub subscriptions: crate::subscriptions::Policy,
    pub sources: BTreeMap<String, Source>,
    pub clients: Vec<Client>,
    #[serde(default)]
    pub mcp: crate::policy::Policy,
    #[serde(default)]
    pub seerr: Option<SeerrConnection>,
    #[serde(default)]
    pub oauth: Option<crate::oauth::Policy>,
}

/// Public instance identity shared by the UI, MCP and calendar feeds.
#[derive(Clone, Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Instance {
    pub title: String,
    pub link: String,
}

impl Instance {
    /// Validate public identity text and its credential-free website URL.
    ///
    /// # Errors
    /// Rejects oversized text, control characters or unsafe website components.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.title.trim().is_empty()
            || self.title.len() > 256
            || self.title.chars().any(|character| {
                character.is_control() || matches!(character, '\u{fffe}' | '\u{ffff}')
            })
            || self.link.len() > 2048
            || self.link.trim() != self.link
            || self.link.chars().any(|character| {
                character.is_control() || matches!(character, '\u{fffe}' | '\u{ffff}')
            })
        {
            return Err(ConfigError::invalid("invalid instance metadata"));
        }

        let link = Url::parse(&self.link).map_err(|error| {
            tracing::debug!(event = "invalid_feed_link", category = ?error);
            ConfigError::invalid("invalid public instance link")
        })?;

        if !matches!(link.scheme(), "http" | "https")
            || link.host_str().is_none()
            || !link.username().is_empty()
            || link.password().is_some()
            || link.query().is_some()
            || link.fragment().is_some()
        {
            return Err(ConfigError::invalid(
                "instance link needs HTTP(S), without credentials, query or fragment",
            ));
        }

        Ok(())
    }
}

impl Default for Instance {
    fn default() -> Self {
        Self {
            title: "ArrPi".to_owned(),
            link: env!("CARGO_PKG_REPOSITORY").to_owned(),
        }
    }
}

/// Bounds for body retention, normalization, upstream admission and cache freshness.
#[derive(Clone, Copy, Debug, Deserialize, schemars::JsonSchema, serde::Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct CachePolicy {
    #[schemars(range(min = 1, max = 86_400))]
    pub fresh_seconds: u64,
    #[schemars(range(min = 0, max = 86_400))]
    pub stale_seconds: u64,
    #[schemars(range(min = 1, max = 60))]
    pub retry_seconds: u64,
    #[schemars(range(min = 1, max = 60))]
    pub timeout_seconds: u64,
    #[schemars(range(min = 1, max = 1024))]
    pub max_entries: usize,
    #[schemars(range(min = 1024, max = 67_108_864))]
    pub max_body_bytes: usize,
    /// Aggregate backing-buffer budget across all sources and held responses.
    #[schemars(range(min = 1_048_576, max = 2_147_483_648_u64))]
    pub max_retained_bytes: usize,
    /// Maximum simultaneous library/calendar normalization operations.
    #[schemars(range(min = 1, max = 16))]
    pub max_normalizations: usize,
    #[schemars(range(min = 1, max = 64))]
    pub max_upstream_requests: usize,
    #[schemars(range(min = 1, max = 256))]
    pub max_requests: usize,
    #[schemars(range(min = 1, max = 256))]
    pub max_connections: usize,
    #[schemars(range(min = 5, max = 300))]
    pub connection_idle_seconds: u64,
    #[schemars(range(min = 30, max = 3600))]
    pub connection_lifetime_seconds: u64,
}

/// A read-only native source with an optional Seerr service mapping.
#[derive(Clone, Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub kind: SourceKind,
    #[serde(default)]
    pub seerr_server_id: Option<u32>,
    pub url: String,
    pub api_key: crate::secret::Secret,
}

/// Supported native library manager protocols.
#[derive(
    Clone, Copy, Debug, Deserialize, Eq, PartialEq, schemars::JsonSchema, serde::Serialize,
)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    Plex,
    Sonarr,
    Radarr,
    Sabnzbd,
    Cleanuparr,
    Bazarr,
    Maintainerr,
    Crosswatch,
}

/// `ArrPi` client grants and an optional native requester identity binding.
#[derive(Clone, Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Client {
    pub name: String,
    pub key: crate::secret::Secret,
    pub permissions: Vec<Box<str>>,
    #[serde(default)]
    pub seerr_user: Option<crate::policy::Binding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracking_user: Option<crate::tracking::Binding>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
enum ConfigFailure {
    #[error("invalid configuration: {0}")]
    Invalid(&'static str),
    #[error("missing or invalid runtime credential")]
    Credential,
    #[error("cannot initialize upstream HTTP client")]
    Transport,
}

/// A redacted configuration failure without runtime secrets or transport details.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error(transparent)]
pub struct ConfigError(ConfigFailure);

impl ConfigError {
    pub const CREDENTIAL: Self = Self(ConfigFailure::Credential);
    pub const TRANSPORT: Self = Self(ConfigFailure::Transport);

    /// Report a static validation rule rather than the rejected owner value.
    #[must_use]
    pub const fn invalid(message: &'static str) -> Self {
        Self(ConfigFailure::Invalid(message))
    }

    /// Return only a static rule; rejected owner values are never retained.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self.0 {
            ConfigFailure::Invalid(reason) => reason,
            ConfigFailure::Credential => "missing or invalid runtime credential",
            ConfigFailure::Transport => "cannot initialize upstream HTTP client",
        }
    }
}

impl Config {
    /// Parse and validate a protected configuration document.
    ///
    /// # Errors
    /// Returns a redacted failure for invalid JSON, fields or policy.
    pub fn parse(text: &str) -> Result<Self, ConfigError> {
        let config: Self = serde_json::from_str(text).map_err(|error| {
            tracing::debug!(event="config_import_json", category=?error.classify());
            ConfigError::invalid("invalid JSON configuration")
        })?;

        config.validate()?;
        Ok(config)
    }

    /// Validate configuration limits, identities, grants and service bindings.
    ///
    /// # Errors
    /// Returns the violated policy without credentials or rejected owner values.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.sources.len() > 8 || self.clients.is_empty() || self.clients.len() > 64 {
            return Err(ConfigError::invalid(
                "configure 0-8 sources and 1-64 clients",
            ));
        }

        self.cache.validate()?;
        self.instance.validate()?;
        self.mcp.validate()?;

        if let Some(oauth) = &self.oauth {
            oauth.validate(self)?;
        }

        self.seerr
            .as_ref()
            .map(SeerrConnection::validate)
            .transpose()?;

        self.subscriptions.validate()?;

        validate_service_bindings(&self.sources)?;

        for (name, source) in &self.sources {
            if !valid_name(name)
                || matches!(
                    name.as_str(),
                    "calendar"
                        | "seerr"
                        | "catalog"
                        | "requests"
                        | "issues"
                        | "config"
                        | "feeds"
                        | "tracking"
                )
            {
                return Err(ConfigError::invalid("invalid or reserved source name"));
            }

            source.base_url()?;
        }

        validate_client_identities(&self.clients)?;
        validate_tracking_clients(&self.clients)?;

        for client in &self.clients {
            client.validate_binding()?;

            self.validate_tracking(client)?;

            if client.permissions.is_empty() {
                return Err(ConfigError::invalid(
                    "a client needs at least one permission",
                ));
            }

            for permission in &client.permissions {
                if matches!(
                    permission.as_ref(),
                    "metrics"
                        | "feeds:issue"
                        | "config:manage"
                        | "admin"
                        | "catalog:view"
                        | "requests:view"
                        | "requests:create"
                        | "requests:cancel"
                        | "issues:view"
                        | "issues:create"
                ) {
                    continue;
                }

                let Some((source, endpoint)) = permission.split_once(':') else {
                    return Err(ConfigError::invalid("permissions use source:endpoint"));
                };
                let Some(source) = self.sources.get(source) else {
                    return Err(ConfigError::invalid("unknown permission source"));
                };

                if !source.kind.allows_permission(endpoint) {
                    return Err(ConfigError::invalid("unknown permission endpoint"));
                }
            }
        }

        Ok(())
    }

    fn validate_tracking(&self, client: &Client) -> Result<(), ConfigError> {
        if let Some(binding) = &client.tracking_user {
            binding.validate()?;

            if !self
                .sources
                .get(&binding.source)
                .is_some_and(|source| source.kind == SourceKind::Crosswatch)
            {
                return Err(ConfigError::invalid(
                    "tracking binding needs a CrossWatch source",
                ));
            }
        }

        Ok(())
    }
}

fn validate_client_identities(clients: &[Client]) -> Result<(), ConfigError> {
    let mut names = std::collections::BTreeSet::new();
    let mut keys = std::collections::BTreeSet::new();

    for client in clients {
        if !valid_name(&client.name) || !names.insert(&client.name) {
            return Err(ConfigError::invalid("invalid or duplicate client name"));
        }

        if !crate::secret::valid_client_key(client.key.value()) || !keys.insert(client.key.value())
        {
            return Err(ConfigError::CREDENTIAL);
        }
    }

    Ok(())
}

impl CachePolicy {
    fn validate(self) -> Result<(), ConfigError> {
        if !(1..=86_400).contains(&self.fresh_seconds)
            || self.stale_seconds > 86_400
            || !(1..=60).contains(&self.retry_seconds)
            || !(1..=60).contains(&self.timeout_seconds)
            || !(1..=1024).contains(&self.max_entries)
            || !(1024..=64 * 1024 * 1024).contains(&self.max_body_bytes)
            || !(1024 * 1024..=2 * 1024 * 1024 * 1024).contains(&self.max_retained_bytes)
            || self.max_retained_bytes < self.max_body_bytes
            || !(1..=16).contains(&self.max_normalizations)
            || !(1..=64).contains(&self.max_upstream_requests)
            || !(1..=256).contains(&self.max_requests)
            || !(1..=256).contains(&self.max_connections)
            || !(5..=300).contains(&self.connection_idle_seconds)
            || !(30..=3600).contains(&self.connection_lifetime_seconds)
        {
            return Err(ConfigError::invalid(
                "cache or request bounds outside supported range",
            ));
        }

        Ok(())
    }
}

impl Default for CachePolicy {
    fn default() -> Self {
        Self {
            fresh_seconds: 60,
            stale_seconds: 900,
            retry_seconds: 5,
            timeout_seconds: 10,
            max_entries: 32,
            max_body_bytes: 1024 * 1024,
            max_retained_bytes: 128 * 1024 * 1024,
            max_normalizations: 4,
            max_upstream_requests: 4,
            max_requests: 64,
            max_connections: 64,
            connection_idle_seconds: 30,
            connection_lifetime_seconds: 300,
        }
    }
}

impl Source {
    /// Validate this source origin without provider-specific assumptions.
    ///
    /// # Errors
    /// Returns a redacted error for unsupported origins or embedded credentials.
    pub fn base_url(&self) -> Result<BaseUrl, ConfigError> {
        base_url(&self.url)
    }
}

/// Validate and normalize an HTTP API base without embedding provider identity.
///
/// # Errors
/// Returns a redacted error for unsupported origins, queries or credentials.
pub fn base_url(value: &str) -> Result<BaseUrl, ConfigError> {
    let mut url = Url::parse(value).map_err(|error| {
        tracing::debug!(event = "invalid_origin", category = ?error);
        ConfigError::invalid("invalid upstream URL")
    })?;

    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.cannot_be_a_base()
    {
        return Err(ConfigError::invalid(
            "upstream URL needs HTTP(S), without credentials, query or fragment",
        ));
    }

    if !url.path().ends_with('/') {
        url.set_path(&format!("{}/", url.path()));
    }

    Ok(BaseUrl(url))
}

/// A validated public endpoint's exact authority and serialized HTTPS origin.
#[derive(Clone)]
pub struct HttpsEndpoint {
    authority: String,
    origin: String,
}

impl HttpsEndpoint {
    /// Exact host and optional explicit port for a host allowlist.
    #[must_use]
    pub fn authority(&self) -> &str {
        &self.authority
    }

    /// Serialized HTTPS origin, without path, query or credentials.
    #[must_use]
    pub fn origin(&self) -> &str {
        &self.origin
    }
}

impl std::fmt::Debug for HttpsEndpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HttpsEndpoint { [REDACTED] }")
    }
}

/// Validate a public HTTPS endpoint with one exact path and no URL decorations.
///
/// # Errors
/// Returns a static reason without including the supplied endpoint.
pub fn https_endpoint(value: &str, exact_path: &str) -> Result<HttpsEndpoint, ConfigError> {
    let url = Url::parse(value).map_err(|error| {
        tracing::debug!(
            event = "invalid_public_endpoint",
            category = std::any::type_name_of_val(&error)
        );
        ConfigError::invalid("invalid public endpoint")
    })?;
    let host = url
        .host_str()
        .ok_or_else(|| ConfigError::invalid("invalid public endpoint"))?;

    if url.scheme() != "https"
        || url.path() != exact_path
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(ConfigError::invalid("invalid public endpoint"));
    }

    Ok(HttpsEndpoint {
        authority: url
            .port()
            .map_or_else(|| host.to_owned(), |port| format!("{host}:{port}")),
        origin: url.origin().ascii_serialization(),
    })
}

impl SourceKind {
    /// Return the allowlisted native resources for this library app.
    #[must_use]
    pub const fn endpoints(self) -> &'static [Endpoint] {
        match self {
            Self::Sonarr => &[
                Endpoint::Calendar,
                Endpoint::Series,
                Endpoint::Queue,
                Endpoint::Health,
                Endpoint::Qualityprofile,
            ],

            Self::Radarr => &[
                Endpoint::Calendar,
                Endpoint::Movie,
                Endpoint::Queue,
                Endpoint::Health,
                Endpoint::Qualityprofile,
            ],

            Self::Sabnzbd
            | Self::Plex
            | Self::Cleanuparr
            | Self::Bazarr
            | Self::Maintainerr
            | Self::Crosswatch => &[],
        }
    }

    /// Stable protocol discriminator for source discovery.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Plex => "plex",
            Self::Sonarr => "sonarr",
            Self::Radarr => "radarr",
            Self::Sabnzbd => "sabnzbd",
            Self::Cleanuparr => "cleanuparr",
            Self::Bazarr => "bazarr",
            Self::Maintainerr => "maintainerr",
            Self::Crosswatch => "crosswatch",
        }
    }

    fn allows_permission(self, endpoint: &str) -> bool {
        if self == Self::Plex {
            return matches!(endpoint, "ratings-view" | "status-view");
        }

        if matches!(self, Self::Sonarr | Self::Radarr)
            && matches!(endpoint, "library-refresh" | "library-search")
        {
            return true;
        }

        if self == Self::Crosswatch && matches!(endpoint, "tracking-view" | "tracking-manage") {
            return true;
        }

        matches!(
            endpoint,
            "calendar-view" | "library-view" | "queue-summary" | "library-summary" | "status-view"
        ) || self
            .endpoints()
            .iter()
            .any(|native| native.name() == endpoint)
    }
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 32
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

impl Client {
    fn validate_binding(&self) -> Result<(), ConfigError> {
        if (self.seerr_user.is_some() || self.tracking_user.is_some())
            && self
                .permissions
                .iter()
                .any(|permission| matches!(permission.as_ref(), "admin" | "config:manage"))
        {
            return Err(ConfigError::invalid(
                "configuration clients cannot bind requester accounts",
            ));
        }

        if self.seerr_user.as_ref().is_some_and(|binding| {
            binding.id == 0 || binding.plex_id == 0 || binding.id > u64::from(u32::MAX)
        }) {
            return Err(ConfigError::invalid("invalid Seerr user binding"));
        }

        Ok(())
    }
}

fn validate_service_bindings(
    sources: &std::collections::BTreeMap<String, Source>,
) -> Result<(), ConfigError> {
    let mut bindings = std::collections::BTreeSet::new();

    for source in sources.values() {
        if source.seerr_server_id.is_some()
            && !matches!(source.kind, SourceKind::Sonarr | SourceKind::Radarr)
        {
            return Err(ConfigError::invalid(
                "only library sources support Seerr service binding",
            ));
        }

        if let Some(id) = source.seerr_server_id
            && !bindings.insert((matches!(source.kind, SourceKind::Sonarr), id))
        {
            return Err(ConfigError::invalid("duplicate Seerr service binding"));
        }
    }

    Ok(())
}

/// Catalog and requester source settings; credentials never serialize.
#[derive(Clone, Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SeerrConnection {
    pub url: String,
    pub api_key: crate::secret::Secret,
    #[serde(default = "locale")]
    pub language: String,
}
fn locale() -> String {
    "en".to_owned()
}
impl SeerrConnection {
    /// Validate the catalog locale and neutral API origin.
    ///
    /// # Errors
    /// Returns a redacted error for unsupported language codes or origins.
    pub fn validate(&self) -> Result<BaseUrl, ConfigError> {
        if self.language.len() > 16
            || !self
                .language
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            || self.language.is_empty()
        {
            return Err(ConfigError::invalid("invalid Seerr language"));
        }

        base_url(&self.url)
    }
}

/// A validated HTTP API origin with a normalized base path and no embedded credentials.
#[derive(Clone)]
pub struct BaseUrl(Url);

impl From<BaseUrl> for Url {
    fn from(value: BaseUrl) -> Self {
        value.0
    }
}

impl std::fmt::Debug for BaseUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("BaseUrl { [REDACTED] }")
    }
}

fn validate_tracking_clients(clients: &[Client]) -> Result<(), ConfigError> {
    let mut profiles = std::collections::BTreeSet::new();
    let mut identities = std::collections::BTreeSet::new();

    for binding in clients
        .iter()
        .filter_map(|client| client.tracking_user.as_ref())
    {
        if !profiles.insert((&binding.source, &binding.profile_id))
            || !identities.insert((&binding.source, binding.plex_id))
        {
            return Err(ConfigError::invalid(
                "tracking accounts must have unique native profiles and Plex identities",
            ));
        }
    }

    Ok(())
}

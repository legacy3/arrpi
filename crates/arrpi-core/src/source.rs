//! Object-safe source capabilities consumed by domain use cases.

use crate::{
    arguments::{Details, Issue, MediaRequest, MediaType, Page, Search},
    config::ConfigError,
    contracts::CacheState,
    error::ApiError,
    policy::Binding,
};
use async_trait::async_trait;
use bytes::Bytes;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Debug,
    sync::Arc,
};

/// Caller-controlled cache mode; sources enforce its documented semantics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadMode {
    Cached,
    Fresh,
}

/// Shared immutable native bytes; cloning retains the same backing allocation.
#[derive(Clone)]
pub struct Body(Bytes);

impl Debug for Body {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Body")
            .field("bytes", &self.0.len())
            .finish()
    }
}

impl AsRef<[u8]> for Body {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<Bytes> for Body {
    fn from(bytes: Bytes) -> Self {
        Self(bytes)
    }
}

impl From<Body> for Bytes {
    fn from(body: Body) -> Self {
        body.0
    }
}

/// Freshness is carried beside data, never inserted into native JSON.
#[derive(Clone, Debug, Serialize)]
pub struct Freshness {
    pub source: String,
    pub cache: Observation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub age_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checked_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<&'static str>,
}

impl Freshness {
    /// Typed provenance shared by normalized response documents.
    #[must_use]
    pub fn document(&self, bypass_requested: bool) -> PublicFreshness {
        PublicFreshness {
            source: self.source.clone(),
            cache: self.cache,
            checked_at: self.checked_at.clone(),
            age_seconds: self.age_seconds,
            note: self.note.map(str::to_owned),
            bypass_requested,
        }
    }

    /// Render source provenance for public tools without copying native data.
    #[must_use]
    pub fn public(&self, bypass_requested: bool) -> Value {
        serde_json::json!(self.document(bypass_requested))
    }
}

/// Uncached reads and native cache observations have distinct meanings.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Observation {
    Hit,
    Miss,
    Stale,
    NotCached,
}

/// Public observation metadata; age is omitted when an upstream does not provide it.
#[derive(Debug, Deserialize, JsonSchema, Serialize)]
pub struct PublicFreshness {
    pub source: String,
    pub cache: Observation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub age_seconds: Option<u64>,
    pub checked_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub bypass_requested: bool,
}

impl From<CacheState> for Observation {
    fn from(state: CacheState) -> Self {
        match state {
            CacheState::Hit => Self::Hit,
            CacheState::Miss => Self::Miss,
            CacheState::Stale => Self::Stale,
        }
    }
}

/// Data and source provenance share one lifetime and authorization context.
#[derive(Clone)]
pub struct SourceRead<T> {
    pub data: T,
    pub freshness: Freshness,
}

impl<T> Debug for SourceRead<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourceRead")
            .field("freshness", &self.freshness)
            .finish_non_exhaustive()
    }
}

impl<T> std::ops::Deref for SourceRead<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

/// Source identity is metadata; consumers discover its implemented capabilities.
#[derive(Clone, Debug)]
pub struct Descriptor {
    pub name: String,
    pub kind: &'static str,
    pub service_binding: Option<u32>,
    pub endpoints: std::borrow::Cow<'static, [crate::contracts::Endpoint]>,
}

/// A principal is resolved from a stored credential by the application boundary.
#[derive(Clone, Debug)]
pub struct Principal {
    pub name: String,
    pub permissions: BTreeSet<Box<str>>,
    pub binding: Option<Binding>,
}

impl Principal {
    /// Test one explicit grant; admin implies every local grant.
    #[must_use]
    pub fn allows(&self, source: &str, operation: &str) -> bool {
        self.permissions.contains("admin")
            || self
                .permissions
                .iter()
                .any(|grant| grant.split_once(':') == Some((source, operation)))
    }

    /// Source discovery is limited to the authenticated principal's grants.
    #[must_use]
    pub fn visible(&self, source: &str) -> bool {
        self.permissions.contains("admin")
            || self.permissions.iter().any(|grant| {
                grant
                    .split_once(':')
                    .is_some_and(|(name, _)| name == source)
            })
    }
}

bitflags::bitflags! {
    /// Provider-normalized account actions, independent of native role encodings.
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub struct ActorRights: u8 {
        const REQUEST_MOVIES = 1 << 0;
        const REQUEST_SERIES = 1 << 1;
        const SELECT_PROFILE = 1 << 2;
        const VIEW_ISSUES = 1 << 3;
        const REPORT_ISSUES = 1 << 4;
        const CANCEL_OWN_PENDING = 1 << 5;
    }
}

/// An account identity and semantic actions verified by its native provider.
#[derive(Clone, Copy, Debug)]
pub struct Actor {
    pub id: u64,
    pub rights: ActorRights,
}

/// Health distinguishes native reachability, stale data and missing actor binding.
#[derive(Debug, Serialize)]
pub struct Health {
    pub reachable: Option<bool>,
    pub version: Option<String>,
    pub health_issue_count: Option<usize>,
    pub writes_supported: Option<bool>,
    pub needs_account: bool,
    pub freshness: Option<Freshness>,
}

/// Each provider participates in one registry without downcasting.
#[async_trait]
pub trait ApiSource: Debug + Send + Sync {
    fn descriptor(&self) -> &Descriptor;

    async fn health(&self, principal: &Principal, mode: ReadMode) -> Result<Health, ApiError>;

    /// Probe a configured connection for an operator, independently of requester readiness.
    ///
    /// # Errors
    /// Rejects callers without configuration access and propagates bounded native read failures.
    async fn connection_health(
        &self,
        principal: &Principal,
        mode: ReadMode,
    ) -> Result<Health, ApiError> {
        if !principal.allows("config", "manage") {
            return Err(ApiError::FORBIDDEN);
        }

        self.health(principal, mode).await
    }

    fn summaries(&self) -> Option<&dyn crate::summaries::SummarySource> {
        None
    }

    fn library(&self) -> Option<&dyn LibrarySource> {
        None
    }

    fn calendar(&self) -> Option<&dyn CalendarSource> {
        None
    }

    fn catalog(&self) -> Option<&dyn CatalogSource> {
        None
    }

    fn accounts(&self) -> Option<&dyn AccountSource> {
        None
    }

    fn requests(&self) -> Option<&dyn RequestSource> {
        None
    }

    fn issues(&self) -> Option<&dyn IssueSource> {
        None
    }

    fn counters(&self) -> crate::contracts::Counters {
        crate::contracts::Counters::default()
    }

    fn native(&self) -> Option<&dyn NativeSource> {
        None
    }
}

/// Read-only native passthrough remains explicitly allowlisted by each adapter.
#[async_trait]
pub trait NativeSource: Debug + Send + Sync {
    async fn read(
        &self,
        endpoint: &str,
        query: &str,
        mode: ReadMode,
    ) -> Result<SourceRead<Body>, ApiError>;
}

/// Installed library snapshots and validated quality profiles.
#[async_trait]
pub trait LibrarySource: Debug + Send + Sync {
    fn media_type(&self) -> MediaType;

    async fn snapshot(&self, mode: ReadMode) -> Result<SourceRead<Body>, ApiError>;

    async fn profiles(&self) -> Result<Vec<QualityProfile>, ApiError>;
}

/// Calendar providers own their native query and return monitored release metadata.
#[async_trait]
pub trait CalendarSource: Debug + Send + Sync {
    async fn calendar(
        &self,
        start: &str,
        end: &str,
        mode: ReadMode,
    ) -> Result<SourceRead<Vec<crate::contracts::CalendarEvent>>, ApiError>;
}

/// Requestable catalog operations accept typed user inputs, never raw paths.
#[async_trait]
pub trait CatalogSource: Debug + Send + Sync {
    async fn search(&self, binding: &Binding, args: Search) -> Result<Value, ApiError>;

    async fn details(
        &self,
        binding: &Binding,
        args: Details,
        settings: &Value,
    ) -> Result<Value, ApiError>;

    async fn public_settings(&self, binding: &Binding) -> Value;
}

/// Bound accounts supply fresh native rights, quotas and service profile metadata.
#[async_trait]
pub trait AccountSource: Debug + Send + Sync {
    async fn identity(&self, binding: &Binding) -> Result<Actor, ApiError>;

    async fn compatible(&self, binding: &Binding) -> Result<bool, ApiError>;

    async fn quota(&self, binding: &Binding, bypass: bool) -> Result<Value, ApiError>;

    async fn service_profiles(
        &self,
        binding: &Binding,
        kind: MediaType,
    ) -> Result<Option<BrokerProfiles>, ApiError>;
}

/// A quality profile is marked safe only after decoding its enabled definitions.
#[derive(Clone, Debug)]
pub struct QualityProfile {
    pub id: u64,
    pub name: String,
    pub non_4k: bool,
}

/// A broker supplies profile identity; safety comes from library definitions.
#[derive(Clone, Debug)]
pub struct BrokerProfile {
    pub id: u64,
    pub name: String,
}

/// Native default non-4K service profiles are matched to the configured library source.
#[derive(Debug)]
pub struct BrokerProfiles {
    pub server_id: u64,
    pub default_profile_id: Option<u64>,
    pub anime_profile_id: Option<u64>,
    pub profiles: Vec<BrokerProfile>,
    pub freshness: Freshness,
}

/// A named choice is broker metadata, not authorization to write to Arr directly.
#[derive(Clone, Debug)]
pub struct ProfileSelection {
    pub server_id: u64,
    pub profile_id: u64,
    pub name: String,
}

/// Account-bound requests keep native quotas, tags, identity and no-4K checks.
#[async_trait]
pub trait RequestSource: Debug + Send + Sync {
    async fn submit(
        &self,
        binding: &Binding,
        item: MediaRequest,
        selection: Option<ProfileSelection>,
    ) -> Result<Value, ApiError>;

    async fn cancel(&self, binding: &Binding, request_id: u64) -> Result<Value, ApiError>;

    async fn requests(&self, binding: &Binding, args: Page) -> Result<Value, ApiError>;

    async fn request(&self, binding: &Binding, request_id: u64) -> Result<Value, ApiError>;
}

/// Only creator-owned issue workflows are exposed to callers.
#[async_trait]
pub trait IssueSource: Debug + Send + Sync {
    async fn issues(&self, binding: &Binding, args: Page) -> Result<Value, ApiError>;

    async fn report(&self, binding: &Binding, args: Issue) -> Result<Value, ApiError>;

    async fn close(&self, binding: &Binding, issue_id: u64) -> Result<Value, ApiError>;
}

/// Owned source reference for concurrent jobs without exposing reference counting.
#[derive(Clone, Debug)]
pub struct SourceHandle {
    inner: Arc<dyn ApiSource>,
}

impl std::ops::Deref for SourceHandle {
    type Target = dyn ApiSource;

    fn deref(&self) -> &Self::Target {
        self.inner.as_ref()
    }
}

/// Registry selection depends on capabilities, never concrete provider structs.
#[derive(Clone, Debug, Default)]
pub struct Sources {
    entries: BTreeMap<String, SourceHandle>,
}

impl Sources {
    /// Start an empty heterogeneous capability registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register one source without overwriting another identity or account broker.
    ///
    /// # Errors
    /// Rejects duplicate names and ambiguous account brokers.
    pub fn insert<T>(&mut self, source: T) -> Result<(), ConfigError>
    where
        T: ApiSource + 'static,
    {
        let name = source.descriptor().name.clone();

        if self.entries.contains_key(&name) {
            return Err(ConfigError::invalid("duplicate source identity"));
        }

        if source.accounts().is_some() && self.account_source().is_some() {
            return Err(ConfigError::invalid("configure one account broker"));
        }

        self.entries.insert(
            name,
            SourceHandle {
                inner: Arc::new(source),
            },
        );
        Ok(())
    }

    /// Resolve a source without exposing its implementation type.
    #[must_use]
    pub fn source(&self, name: &str) -> Option<&dyn ApiSource> {
        self.entries.get(name).map(|source| source.inner.as_ref())
    }

    /// Retain a source while an independent asynchronous job runs.
    #[must_use]
    pub fn shared(&self, name: &str) -> Option<SourceHandle> {
        self.entries.get(name).cloned()
    }

    /// Iterate capabilities in stable source-name order.
    #[must_use]
    pub fn iter(&self) -> SourcesIter<'_> {
        self.into_iter()
    }

    /// Discover the configured account broker through its capabilities.
    #[must_use]
    pub fn account_source(&self) -> Option<&dyn ApiSource> {
        self.entries
            .values()
            .find(|source| source.accounts().is_some())
            .map(|source| source.inner.as_ref())
    }

    /// Count all identities, including sources without native endpoints.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the registry has no source identities.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Borrowed registry iteration keeps source implementations private.
#[derive(Debug)]
pub struct SourcesIter<'a> {
    inner: std::collections::btree_map::Iter<'a, String, SourceHandle>,
}

impl<'a> Iterator for SourcesIter<'a> {
    type Item = (&'a str, &'a dyn ApiSource);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner
            .next()
            .map(|(name, source)| (name.as_str(), source.inner.as_ref()))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<'a> IntoIterator for &'a Sources {
    type Item = (&'a str, &'a dyn ApiSource);
    type IntoIter = SourcesIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        SourcesIter {
            inner: self.entries.iter(),
        }
    }
}

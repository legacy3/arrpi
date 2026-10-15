//! Protocol-neutral command execution and awaited best-effort observation contracts.

use crate::{
    error::ApiError,
    registry::{ToolDefinition, ToolId},
};
use async_trait::async_trait;
use serde_json::Value;
use std::fmt::Debug;
use std::sync::Arc;
use tokio::time::Instant;

/// HTTP and stdio use the same command contract without depending on an MCP SDK.
#[async_trait]
pub trait CommandExecutor: Debug + Send + Sync {
    async fn list(&self, credential: &str) -> Result<Vec<ToolDefinition>, ApiError>;

    async fn execute(
        &self,
        credential: &str,
        tool: ToolId,
        arguments: Value,
    ) -> Result<Value, ApiError>;

    /// Read an advertised schema; remote implementations require its caller identity.
    fn definition(&self, credential: Option<&str>, tool: ToolId) -> Option<ToolDefinition>;
}

/// A shared command implementation for protocol adapters, with private ownership mechanics.
#[derive(Clone, Debug)]
pub struct Executor {
    inner: Arc<dyn CommandExecutor>,
}

impl Executor {
    /// Erase a local or remote implementation once at composition.
    #[must_use]
    pub fn new<T>(executor: T) -> Self
    where
        T: CommandExecutor + 'static,
    {
        Self {
            inner: Arc::new(executor),
        }
    }
}

impl std::ops::Deref for Executor {
    type Target = dyn CommandExecutor;

    fn deref(&self) -> &Self::Target {
        self.inner.as_ref()
    }
}

/// A whitelisted write-attempt target, never arbitrary request or issue text.
#[derive(Clone, Debug)]
pub struct AuditEvent {
    pub tool: String,
    pub target: crate::contracts::AuditTarget,
    pub outcome: String,
}

/// Observation storage cannot change an authoritative native mutation receipt.
#[async_trait]
pub trait EventSink: Debug + Send + Sync {
    async fn audit(
        &self,
        client: &str,
        actor: Option<u64>,
        events: &[AuditEvent],
        maximum: usize,
    ) -> Result<(), ApiError>;

    async fn statistics(
        &self,
        client: &str,
        tool: ToolId,
        outcome: &str,
        duration_ms: u64,
    ) -> Result<(), ApiError>;
}

/// Shared observation wiring keeps the concrete database outside domain code.
#[derive(Clone, Debug)]
pub struct Events {
    inner: Arc<dyn EventSink>,
}

impl Events {
    /// Bind one observation implementation at runtime construction.
    #[must_use]
    pub fn new<T>(sink: T) -> Self
    where
        T: EventSink + 'static,
    {
        Self {
            inner: Arc::new(sink),
        }
    }
}

impl std::ops::Deref for Events {
    type Target = dyn EventSink;

    fn deref(&self) -> &Self::Target {
        self.inner.as_ref()
    }
}

/// Observation wiring and its monotonic duration clock, supplied at construction.
#[derive(Debug)]
pub struct Recording {
    pub(crate) events: Option<Events>,
    pub(crate) clock: fn() -> Instant,
}

impl Recording {
    /// Couple optional persistence with the injected clock used for its durations.
    #[must_use]
    pub const fn new(events: Option<Events>, clock: fn() -> Instant) -> Self {
        Self { events, clock }
    }
}

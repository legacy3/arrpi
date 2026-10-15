//! Awaited bounded blocking observations; failures never rewrite mutation receipts.

use super::Store;
use arrpi_core::{
    error::ApiError,
    executor::{AuditEvent, EventSink},
    registry::ToolId,
};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Semaphore;

/// Shared `SQLite` observer with at most four outstanding blocking jobs.
#[derive(Clone, Debug)]
pub struct SqliteEvents {
    store: Store,
    workers: Arc<Semaphore>,
}

impl SqliteEvents {
    /// Bind command observations to the same private settings database.
    #[must_use]
    pub fn new(store: Store) -> Self {
        Self {
            store,
            workers: Arc::new(Semaphore::new(4)),
        }
    }

    async fn observe<F>(&self, work: F) -> Result<(), ApiError>
    where
        F: FnOnce(Store) -> Result<(), ApiError> + Send + 'static,
    {
        let permit = Arc::clone(&self.workers)
            .acquire_owned()
            .await
            .map_err(|error| {
                tracing::debug!(event="observation_capacity",category=%error);
                ApiError::INTERNAL
            })?;
        let store = self.store.clone();

        tokio::task::spawn_blocking(move || {
            let _permit = permit;

            work(store)
        })
        .await
        .map_err(|error| {
            tracing::debug!(
                event = "observation_worker",
                cancelled = error.is_cancelled(),
                panicked = error.is_panic()
            );
            ApiError::INTERNAL
        })?
    }
}

#[async_trait]
impl EventSink for SqliteEvents {
    async fn audit(
        &self,
        client: &str,
        actor: Option<u64>,
        events: &[AuditEvent],
        maximum: usize,
    ) -> Result<(), ApiError> {
        let client = client.to_owned();
        let events = events.to_vec();

        self.observe(move |store| store.record_audit(&client, actor, &events, maximum))
            .await
    }

    async fn statistics(
        &self,
        client: &str,
        tool: ToolId,
        outcome: &str,
        duration_ms: u64,
    ) -> Result<(), ApiError> {
        let client = client.to_owned();
        let outcome = outcome.to_owned();

        self.observe(move |store| store.record_tool(&client, tool.name(), &outcome, duration_ms))
            .await
    }
}

//! Strictly bounded cache slots and per-key concurrent fetch sharing.

mod pressure;

use std::{
    collections::BTreeMap,
    sync::{
        Arc, Weak,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use bytes::Bytes;
use tokio::{
    sync::{Mutex, RwLock},
    time::Instant,
};

use crate::metrics::Metrics;
use arrpi_core::{config::CachePolicy, contracts::CacheState, error::ApiError, source::ReadMode};

#[derive(Debug)]
pub(crate) struct Cache {
    policy: CachePolicy,
    slots: Mutex<BTreeMap<String, Slot>>,
    clock: fn() -> Instant,
}

#[derive(Debug)]
struct Slot {
    record: Arc<Refresh>,
    used: Instant,
}

/// Weak cleanup handles kept independently of source capability objects.
#[derive(Clone, Debug)]
pub struct Pruner {
    caches: Vec<Weak<Cache>>,
}

/// Owned task lifetime; dropping it stops periodic cache pruning.
#[derive(Debug)]
pub struct Maintenance {
    task: tokio::task::JoinHandle<()>,
}

impl Pruner {
    pub(crate) fn new(caches: Vec<Weak<Cache>>) -> Self {
        Self { caches }
    }

    /// Start cleanup after the application enters its async runtime.
    ///
    /// # Errors
    /// Returns a redacted error when no Tokio runtime is active.
    pub fn start(&self) -> Result<Maintenance, arrpi_core::config::ConfigError> {
        let handle = tokio::runtime::Handle::try_current().map_err(|error| {
            tracing::debug!(
                event = "cache_maintenance_runtime",
                category = std::any::type_name_of_val(&error)
            );
            arrpi_core::config::ConfigError::TRANSPORT
        })?;
        let pruner = self.clone();
        let task = handle.spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(5)).await;
                pruner.prune().await;
            }
        });

        Ok(Maintenance { task })
    }

    pub(crate) async fn prune(&self) {
        for cache in &self.caches {
            if let Some(cache) = cache.upgrade() {
                cache.prune().await;
            }
        }
    }
}

impl Drop for Maintenance {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[derive(Debug, Default)]
struct Record {
    stored: Option<Stored>,
    failure: Option<Failure>,
    last_error: Option<ApiError>,
}

#[derive(Debug, Default)]
struct Refresh {
    record: RwLock<Record>,
    fetch: Mutex<()>,
    generation: AtomicU64,
}

struct Stored {
    body: Bytes,
    fetched: Instant,
}

#[derive(Debug)]
struct Failure {
    error: ApiError,
    at: Instant,
}

pub(crate) struct Cached {
    pub(crate) body: Bytes,
    pub(crate) age: u64,
    pub(crate) state: CacheState,
}

impl std::fmt::Debug for Cached {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cached")
            .field("bytes", &self.body.len())
            .field("age", &self.age)
            .field("state", &self.state)
            .finish()
    }
}

impl Cache {
    pub(crate) fn new(policy: CachePolicy, clock: fn() -> Instant) -> Self {
        Self {
            policy,
            slots: Mutex::new(BTreeMap::new()),
            clock,
        }
    }

    pub(crate) fn now(&self) -> Instant {
        (self.clock)()
    }

    pub(crate) async fn get<F>(
        &self,
        key: String,
        metrics: &Metrics,
        mode: ReadMode,
        fetch: F,
    ) -> Result<Cached, ApiError>
    where
        F: Future<Output = Result<Bytes, ApiError>> + Send,
    {
        let slot = self.slot(key).await?;

        self.expire(&mut *slot.record.write().await);
        let observed = slot.generation.load(Ordering::Acquire);

        if mode == ReadMode::Cached
            && let Some(response) = self.reuse(&*slot.record.read().await, metrics)
        {
            return response;
        }

        let _refresh = slot.fetch.lock().await;
        let record = slot.record.read().await;

        if mode == ReadMode::Fresh && slot.generation.load(Ordering::Acquire) != observed {
            if let Some(error) = record.last_error {
                return Err(error);
            }

            if let Some(stored) = &record.stored {
                metrics.hits.fetch_add(1, Ordering::Relaxed);

                return Ok(stored.response(CacheState::Hit, (self.clock)()));
            }
        }

        if mode == ReadMode::Cached
            && let Some(response) = self.reuse(&record, metrics)
        {
            return response;
        }

        drop(record);
        metrics.misses.fetch_add(1, Ordering::Relaxed);

        let result = fetch.await;
        let mut record = slot.record.write().await;

        if !matches!(result, Err(ApiError::BUSY | ApiError::CAPACITY)) {
            slot.generation.fetch_add(1, Ordering::Release);
            record.last_error = result.as_ref().err().copied();
        }

        self.finish(&mut record, result, metrics, mode)
    }

    pub(crate) async fn prune(&self) {
        let slots = self.slots.lock().await;

        for slot in slots.values() {
            if let Ok(mut record) = slot.record.record.try_write() {
                self.expire(&mut record);
            }
        }
    }

    fn expire(&self, record: &mut Record) {
        let lifetime = Duration::from_secs(
            self.policy
                .fresh_seconds
                .saturating_add(self.policy.stale_seconds),
        );

        if record
            .stored
            .as_ref()
            .is_some_and(|stored| (self.clock)().duration_since(stored.fetched) >= lifetime)
        {
            record.stored = None;
        }
    }

    fn finish(
        &self,
        record: &mut Record,
        result: Result<Bytes, ApiError>,
        metrics: &Metrics,
        mode: ReadMode,
    ) -> Result<Cached, ApiError> {
        match result {
            Ok(body) => {
                let stored = Stored {
                    body,
                    fetched: (self.clock)(),
                };
                let response = stored.response(CacheState::Miss, (self.clock)());

                record.stored = Some(stored);
                record.failure = None;

                Ok(response)
            }

            Err(error) => {
                if !matches!(error, ApiError::BUSY | ApiError::CAPACITY) {
                    metrics.upstream_failures.fetch_add(1, Ordering::Relaxed);
                    record.failure = Some(Failure {
                        error,
                        at: (self.clock)(),
                    });
                }

                if !error.can_serve_stale() && !matches!(error, ApiError::BUSY | ApiError::CAPACITY)
                {
                    record.stored = None;
                }

                if mode == ReadMode::Fresh {
                    Err(error)
                } else {
                    self.on_failure(record, error, metrics)
                }
            }
        }
    }

    fn reuse(&self, record: &Record, metrics: &Metrics) -> Option<Result<Cached, ApiError>> {
        if let Some(stored) = &record.stored
            && (self.clock)().duration_since(stored.fetched)
                < Duration::from_secs(self.policy.fresh_seconds)
        {
            metrics.hits.fetch_add(1, Ordering::Relaxed);

            return Some(Ok(stored.response(CacheState::Hit, (self.clock)())));
        }

        if let Some(failure) = &record.failure
            && (self.clock)().duration_since(failure.at)
                < Duration::from_secs(self.policy.retry_seconds)
        {
            return Some(self.on_failure(record, failure.error, metrics));
        }

        None
    }

    async fn slot(&self, key: String) -> Result<Arc<Refresh>, ApiError> {
        let mut slots = self.slots.lock().await;

        if !slots.contains_key(&key) && slots.len() == self.policy.max_entries {
            let oldest = slots
                .iter()
                .filter(|(_, slot)| Arc::strong_count(&slot.record) == 1)
                .min_by_key(|(_, slot)| slot.used)
                .map(|(key, _)| key.clone())
                .ok_or(ApiError::CAPACITY)?;

            slots.remove(&oldest);
        }

        let slot = slots.entry(key).or_insert_with(|| Slot {
            record: Arc::new(Refresh::default()),
            used: (self.clock)(),
        });

        slot.used = (self.clock)();
        Ok(Arc::clone(&slot.record))
    }

    fn on_failure(
        &self,
        record: &Record,
        error: ApiError,
        metrics: &Metrics,
    ) -> Result<Cached, ApiError> {
        if error.can_serve_stale()
            && let Some(stored) = &record.stored
        {
            let lifetime =
                Duration::from_secs(self.policy.fresh_seconds + self.policy.stale_seconds);

            if (self.clock)().duration_since(stored.fetched) < lifetime {
                metrics.stale.fetch_add(1, Ordering::Relaxed);

                return Ok(stored.response(CacheState::Stale, (self.clock)()));
            }
        }

        Err(error)
    }
}

impl Stored {
    fn response(&self, state: CacheState, now: Instant) -> Cached {
        Cached {
            body: self.body.clone(),
            age: now.duration_since(self.fetched).as_secs(),
            state,
        }
    }
}

impl std::fmt::Debug for Stored {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Stored")
            .field("bytes", &self.body.len())
            .field("fetched", &self.fetched)
            .finish()
    }
}

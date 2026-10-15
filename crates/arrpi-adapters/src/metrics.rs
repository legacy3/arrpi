//! Aggregate counters without credentials, query labels or native identities.
use arrpi_core::contracts::Counters;
use std::sync::atomic::{AtomicU64, Ordering};

/// Source-local cache counters exposed as a typed public snapshot.
#[derive(Debug, Default)]
pub struct Metrics {
    pub(crate) hits: AtomicU64,
    pub(crate) misses: AtomicU64,
    pub(crate) stale: AtomicU64,
    pub(crate) upstream_failures: AtomicU64,
}

impl Metrics {
    /// Construct empty counters for a source revision.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            stale: AtomicU64::new(0),
            upstream_failures: AtomicU64::new(0),
        }
    }

    /// Read an aggregate snapshot without exposing native request identity.
    #[must_use]
    pub fn snapshot(&self) -> Counters {
        Counters {
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
            stale: self.stale.load(Ordering::Relaxed),
            upstream_failures: self.upstream_failures.load(Ordering::Relaxed),
        }
    }
}

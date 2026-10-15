//! Shared admission for CPU normalization, acquired only after upstream reads.

use crate::{config::ConfigError, error::ApiError};
use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// Library and calendar normalization share one configured concurrency pool.
#[derive(Clone, Debug)]
pub struct Normalizations {
    slots: Arc<Semaphore>,
}

/// A normalization slot is released on completion or cancellation.
#[derive(Debug)]
pub struct Lease {
    _permit: OwnedSemaphorePermit,
}

impl Normalizations {
    /// Build a pool without spawning tasks.
    ///
    /// # Errors
    /// Rejects limits outside the supported configuration range.
    pub fn new(maximum: usize) -> Result<Self, ConfigError> {
        if !(1..=16).contains(&maximum) {
            return Err(ConfigError::invalid("invalid normalization concurrency"));
        }

        Ok(Self {
            slots: Arc::new(Semaphore::new(maximum)),
        })
    }

    /// Enter a CPU phase; network waits must not retain this lease.
    ///
    /// # Errors
    /// Returns retry-safe busy when all normalization slots are occupied.
    pub fn try_acquire(&self) -> Result<Lease, ApiError> {
        Ok(Lease {
            _permit: Arc::clone(&self.slots)
                .try_acquire_owned()
                .map_err(ApiError::from)?,
        })
    }
}

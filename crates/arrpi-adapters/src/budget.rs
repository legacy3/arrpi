//! Retained payload reservations follow the final shared backing allocation.
use arrpi_core::{config::ConfigError, error::ApiError};
use bytes::Bytes;
use std::sync::{Arc, OnceLock};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// Shared native-body capacity limit across sources and outgoing references.
#[derive(Clone, Debug)]
pub struct Budget {
    slots: Arc<Semaphore>,
    limit: usize,
    pruner: Arc<OnceLock<crate::cache::Pruner>>,
}

struct Backing {
    bytes: Vec<u8>,
    _reservation: OwnedSemaphorePermit,
}

impl Budget {
    /// Construct one capacity pool for an immutable runtime revision.
    ///
    /// # Errors
    /// Returns invalid configuration for a zero or unsupported capacity.
    pub fn new(limit: usize) -> Result<Self, ConfigError> {
        if limit == 0 || limit > Semaphore::MAX_PERMITS || u32::try_from(limit).is_err() {
            return Err(ConfigError::invalid("unsupported retained-byte budget"));
        }

        Ok(Self {
            slots: Arc::new(Semaphore::new(limit)),
            limit,
            pruner: Arc::new(OnceLock::new()),
        })
    }

    /// Capacity retained by collected bodies and in-progress collection.
    #[must_use]
    pub fn used(&self) -> usize {
        self.limit - self.slots.available_permits()
    }

    /// Configured payload capacity, excluding allocator and HTTP overhead.
    #[must_use]
    pub const fn limit(&self) -> usize {
        self.limit
    }

    pub(crate) fn attach(&self, pruner: crate::cache::Pruner) -> Result<(), ConfigError> {
        self.pruner.set(pruner).map_err(|error| {
            tracing::debug!(
                event = "cache_housekeeping_duplicate",
                category = std::any::type_name_of_val(&error)
            );
            ConfigError::invalid("cache housekeeping already configured")
        })
    }

    pub(crate) async fn reserve(&self, capacity: usize) -> Result<OwnedSemaphorePermit, ApiError> {
        let amount = u32::try_from(capacity).map_err(|error| {
            tracing::debug!(
                event = "retained_capacity_invalid",
                category = std::any::type_name_of_val(&error)
            );
            ApiError::CAPACITY
        })?;

        if capacity > self.limit {
            return Err(ApiError::CAPACITY);
        }

        if let Ok(reservation) = Arc::clone(&self.slots).try_acquire_many_owned(amount) {
            return Ok(reservation);
        }

        if let Some(pruner) = self.pruner.get()
            && let Some(reservation) = pruner.reclaim(&self.slots, amount).await
        {
            return Ok(reservation);
        }

        Arc::clone(&self.slots)
            .try_acquire_many_owned(amount)
            .map_err(|error| {
                tracing::debug!(
                    event = "retained_capacity_full",
                    category = std::any::type_name_of_val(&error)
                );
                ApiError::CAPACITY
            })
    }
}

pub(crate) fn leased(
    bytes: Vec<u8>,
    mut reservation: OwnedSemaphorePermit,
) -> Result<Bytes, ApiError> {
    let unused = reservation
        .num_permits()
        .checked_sub(bytes.capacity())
        .ok_or(ApiError::CAPACITY)?;

    drop(reservation.split(unused));
    Ok(Bytes::from_owner(Backing {
        bytes,
        _reservation: reservation,
    }))
}

impl AsRef<[u8]> for Backing {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

impl std::fmt::Debug for Backing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Backing")
            .field("capacity", &self.bytes.capacity())
            .finish_non_exhaustive()
    }
}

//! One bounded LRU snapshot reclaims idle cache ownership under byte pressure.

use std::sync::{Arc, Weak};

use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::Instant;

use super::{Cache, Pruner, Refresh};

struct Candidate {
    cache: usize,
    key: String,
    record: Weak<Refresh>,
    used: Instant,
}

impl Pruner {
    pub(crate) async fn reclaim(
        &self,
        slots: &Arc<Semaphore>,
        amount: u32,
    ) -> Option<OwnedSemaphorePermit> {
        self.prune().await;

        if let Ok(reservation) = Arc::clone(slots).try_acquire_many_owned(amount) {
            return Some(reservation);
        }

        let mut candidates = Vec::new();

        for (index, cache) in self.caches.iter().enumerate() {
            if let Some(cache) = cache.upgrade() {
                cache.candidates(index, &mut candidates).await;
            }
        }

        candidates.sort_unstable_by(|left, right| {
            left.used
                .cmp(&right.used)
                .then_with(|| left.cache.cmp(&right.cache))
                .then_with(|| left.key.cmp(&right.key))
        });

        for candidate in candidates {
            if let Ok(reservation) = Arc::clone(slots).try_acquire_many_owned(amount) {
                return Some(reservation);
            }

            if let Some(cache) = self.caches[candidate.cache].upgrade() {
                cache.evict(&candidate).await;
            }
        }

        Arc::clone(slots).try_acquire_many_owned(amount).ok()
    }
}

impl Cache {
    async fn candidates(&self, cache: usize, candidates: &mut Vec<Candidate>) {
        let slots = self.slots.lock().await;

        for (key, slot) in slots.iter() {
            if Arc::strong_count(&slot.record) == 1
                && slot
                    .record
                    .record
                    .try_read()
                    .is_ok_and(|record| record.stored.is_some())
            {
                candidates.push(Candidate {
                    cache,
                    key: key.clone(),
                    record: Arc::downgrade(&slot.record),
                    used: slot.used,
                });
            }
        }
    }

    async fn evict(&self, candidate: &Candidate) {
        let mut slots = self.slots.lock().await;
        let unchanged_idle = slots.get(&candidate.key).is_some_and(|slot| {
            slot.used == candidate.used
                && Arc::strong_count(&slot.record) == 1
                && candidate.record.ptr_eq(&Arc::downgrade(&slot.record))
        });

        if unchanged_idle {
            slots.remove(&candidate.key);
        }
    }
}

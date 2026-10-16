//! Scoped `CrossWatch` account setup over native PIN and Watcher APIs.

mod client;
mod projection;
mod providers;
mod scope;
mod workflow;

use arrpi_core::{
    config::{CachePolicy, ConfigError, Source},
    error::ApiError,
    source::{ApiSource, Descriptor, Health, Principal, ReadMode},
    tracking::{
        Authorization, Binding, DestinationAction, Poll, PollState, Provider, Status,
        TrackingSource,
    },
};
use async_trait::async_trait;
use tokio::sync::{Mutex, MutexGuard, Semaphore, SemaphorePermit};

use crate::{budget::Budget, http::Http};

#[derive(Debug)]
pub(crate) struct Backend {
    descriptor: Descriptor,
    client: client::Client,
    writes: Mutex<()>,
    waiting: Semaphore,
}

impl Backend {
    pub(crate) fn new(
        name: &str,
        source: &Source,
        policy: CachePolicy,
        http: Http,
        budget: Budget,
    ) -> Result<Self, ConfigError> {
        Ok(Self {
            descriptor: Descriptor {
                name: name.to_owned(),
                kind: "crosswatch",
                service_binding: None,
                endpoints: (&[][..]).into(),
            },
            client: client::Client::new(source, policy, http, budget)?,
            writes: Mutex::new(()),
            waiting: Semaphore::new(policy.max_upstream_requests),
        })
    }

    async fn write(&self) -> Result<Write<'_>, ApiError> {
        let slot = self
            .waiting
            .try_acquire()
            .map_err(|error| crate::http::redacted_error(&error, ApiError::CAPACITY))?;
        let lock = tokio::time::timeout(std::time::Duration::from_secs(15), self.writes.lock())
            .await
            .map_err(|error| crate::http::redacted_error(&error, ApiError::CAPACITY))?;

        Ok(Write {
            _slot: slot,
            _lock: lock,
        })
    }
}

struct Write<'a> {
    _slot: SemaphorePermit<'a>,
    _lock: MutexGuard<'a, ()>,
}

#[async_trait]
impl ApiSource for Backend {
    fn descriptor(&self) -> &Descriptor {
        &self.descriptor
    }

    async fn health(&self, principal: &Principal, _: ReadMode) -> Result<Health, ApiError> {
        if !principal.visible(&self.descriptor.name) {
            return Err(ApiError::FORBIDDEN);
        }

        self.client.get("api/user-profiles", &[]).await?;
        Ok(Health {
            reachable: Some(true),
            version: None,
            health_issue_count: None,
            writes_supported: Some(true),
            needs_account: principal.tracking.is_none(),
            freshness: None,
        })
    }

    fn tracking(&self) -> Option<&dyn TrackingSource> {
        Some(self)
    }
}

#[async_trait]
impl TrackingSource for Backend {
    async fn status(&self, binding: &Binding) -> Result<Status, ApiError> {
        let scope = self.scope(binding).await?;

        self.project(binding, &scope).await
    }

    async fn authorize(
        &self,
        binding: &Binding,
        provider: Provider,
    ) -> Result<Authorization, ApiError> {
        let _lock = self.write().await?;

        self.start_authorization(binding, provider).await
    }

    async fn set_enabled(&self, binding: &Binding, enabled: bool) -> Result<Status, ApiError> {
        let _lock = self.write().await?;

        self.change_enabled(binding, enabled).await?;
        self.status(binding).await
    }

    async fn disconnect(&self, binding: &Binding) -> Result<Status, ApiError> {
        let _lock = self.write().await?;

        self.disconnect_trakt(binding).await?;
        self.status(binding).await
    }

    async fn poll(&self, binding: &Binding, provider: &str) -> Result<Poll, ApiError> {
        providers::pin_flow(provider).ok_or(ApiError::NOT_FOUND)?;
        let Ok(_lock) = self.writes.try_lock() else {
            return Ok(Poll {
                state: PollState::Pending,
            });
        };
        let scope = self.scope(binding).await?.select(provider)?;

        self.poll_destination(&scope).await
    }

    async fn destination(
        &self,
        binding: &Binding,
        provider: &str,
        action: DestinationAction,
    ) -> Result<Status, ApiError> {
        let _lock = self.write().await?;
        let scope = self.scope(binding).await?.select(provider)?;

        match action {
            DestinationAction::Enable => self.change_destination(binding, &scope, true).await?,
            DestinationAction::Pause => self.change_destination(binding, &scope, false).await?,
            DestinationAction::Disconnect => self.disconnect_destination(binding, &scope).await?,
        }

        self.status(binding).await
    }
}

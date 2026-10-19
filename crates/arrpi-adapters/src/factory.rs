//! Explicit composition of native adapters and their shared resource limits.

use std::{sync::Arc, time::Duration};

use arrpi_core::{
    config::{Config, ConfigError, SourceKind},
    source::Sources,
};
use tokio::time::Instant;

use crate::{
    budget::Budget,
    cache::{Cache, Pruner},
    http::{Http, HttpOptions},
    native::{Native, Pools, Radarr, Sonarr},
};

/// One immutable configuration revision and its explicit maintenance lifecycle.
#[derive(Debug)]
pub struct BuiltSources {
    pub sources: Sources,
    pub pruner: Pruner,
    pub budget: Budget,
    pub normalizations: arrpi_core::admission::Normalizations,
}

/// Construct sources without spawning tasks or contacting native applications.
///
/// # Errors
/// Rejects invalid URLs, credentials, duplicate identities or unsupported limits.
pub fn build(config: &Config, clock: fn() -> Instant) -> Result<BuiltSources, ConfigError> {
    let http = Http::new(HttpOptions {
        timeout: Duration::from_secs(config.cache.timeout_seconds),
        connect_timeout: Duration::from_secs(config.cache.timeout_seconds),
        pool_idle_per_host: config.cache.max_upstream_requests,
    })?;
    let budget = Budget::new(config.cache.max_retained_bytes)?;
    let normalizations =
        arrpi_core::admission::Normalizations::new(config.cache.max_normalizations)?;
    let mut sources = Sources::new();
    let mut caches = Vec::with_capacity(config.sources.len());

    for (name, source) in &config.sources {
        if source.kind == SourceKind::Crosswatch {
            sources.insert(crate::crosswatch::Backend::new(
                name,
                source,
                config.cache,
                http.clone(),
                budget.clone(),
            )?)?;
            continue;
        }

        let policy = if matches!(
            source.kind,
            SourceKind::Sonarr | SourceKind::Radarr | SourceKind::Plex
        ) {
            config.cache
        } else {
            crate::monitoring::operational_policy(config.cache)
        };
        let cache = Arc::new(Cache::new(policy, clock));

        caches.push(Arc::downgrade(&cache));

        if source.kind == SourceKind::Plex {
            sources.insert(crate::plex::Plex::new(
                name,
                source,
                config.cache,
                http.clone(),
                budget.clone(),
                cache,
            )?)?;
            continue;
        }

        if !matches!(source.kind, SourceKind::Sonarr | SourceKind::Radarr) {
            sources.insert(crate::monitoring::Monitor::new(
                name,
                source,
                config.cache,
                http.clone(),
                crate::monitoring::Admission {
                    budget: budget.clone(),
                    normalizations: normalizations.clone(),
                },
                cache,
            )?)?;
            continue;
        }

        let operational_cache = Arc::new(Cache::new(
            crate::monitoring::operational_policy(config.cache),
            clock,
        ));

        caches.push(Arc::downgrade(&operational_cache));
        let native = Native::new(
            name,
            source,
            config.cache,
            http.clone(),
            Pools {
                budget: budget.clone(),
                normalizations: normalizations.clone(),
                operational_cache,
                write_limits: config.mcp.limits,
            },
            cache,
        )?;

        match source.kind {
            SourceKind::Sonarr => sources.insert(Sonarr(native))?,
            SourceKind::Radarr => sources.insert(Radarr(native))?,
            _ => return Err(ConfigError::invalid("unsupported library provider")),
        }
    }

    if let Some(connection) = &config.seerr {
        sources.insert(crate::seerr::Backend::new(
            connection,
            config.cache,
            config.mcp.limits,
            http,
            budget.clone(),
        )?)?;
    }

    let pruner = Pruner::new(caches);

    budget.attach(pruner.clone())?;

    Ok(BuiltSources {
        sources,
        pruner,
        budget,
        normalizations,
    })
}

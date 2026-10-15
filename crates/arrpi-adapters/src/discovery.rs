//! Credential-free, bounded identity probes for explicitly selected hosts only.
#[cfg(test)]
mod tests;
use crate::http::{self, Http, HttpOptions, Intent};
use arrpi_core::{config::ConfigError, error::ApiError};
use serde::Serialize;
use serde_json::Value;
use std::{net::IpAddr, time::Duration};

/// Reachability is separate from a verified application identity.
#[derive(Debug, Serialize)]
pub struct Probe {
    pub address: IpAddr,
    pub port: u16,
    pub expected: &'static str,
    pub detected: Option<&'static str>,
    pub status: &'static str,
}

const APPS: &[(&str, u16, &str)] = &[
    ("sonarr", 8989, "api/v3/system/status"),
    ("radarr", 7878, "api/v3/system/status"),
    ("prowlarr", 9696, "api/v1/system/status"),
    ("sabnzbd", 8080, "api?mode=version&output=json"),
    ("seerr", 5055, "api/v1/status"),
    ("bazarr", 6767, "api/system/status"),
    ("maintainerr", 6246, "api/app/version"),
    ("cleanuparr", 11011, "api/version"),
    ("plex", 32400, "identity"),
    ("tautulli", 8181, "api/v2?cmd=get_tautulli_info"),
    ("arrpi", 8780, "health"),
];

/// Probe common media-app ports with no API keys, cookies, redirects or mutations.
///
/// # Errors
/// Rejects empty/excessive address sets or an unavailable local HTTP transport.
pub(crate) async fn discover(
    addresses: &[IpAddr],
    scheme: &str,
    clock: fn() -> std::time::Instant,
) -> Result<crate::DiscoveryReport, ConfigError> {
    if !matches!(scheme, "http" | "https") {
        return Err(ConfigError::invalid("discover requires HTTP or HTTPS"));
    }

    if addresses.is_empty() || addresses.len() > 64 {
        return Err(ConfigError::invalid(
            "discover needs 1-64 explicit IP addresses",
        ));
    }

    let http = Http::new(HttpOptions {
        timeout: Duration::from_secs(2),
        connect_timeout: Duration::from_secs(1),
        pool_idle_per_host: 1,
    })?;
    let deadline = tokio::time::Instant::from_std(clock() + Duration::from_secs(30));
    let mut probes = Vec::with_capacity(addresses.len() * APPS.len());
    let mut tasks = tokio::task::JoinSet::new();
    let mut targets = addresses
        .iter()
        .flat_map(|address| APPS.iter().map(move |app| (*address, app)));

    loop {
        while tasks.len() < 8 {
            let Some((address, &(name, port, path))) = targets.next() else {
                break;
            };
            let http = http.clone();
            let scheme = scheme.to_owned();

            tasks.spawn(async move { probe(&http, address, port, name, path, &scheme).await });
        }

        if tasks.is_empty() {
            break;
        }

        match tokio::time::timeout_at(deadline, tasks.join_next()).await {
            Ok(Some(Ok(probe))) => probes.push(probe),

            Ok(_) => return Err(ConfigError::TRANSPORT),

            Err(_) => {
                tasks.abort_all();
                break;
            }
        }
    }

    probes.sort_by_key(|probe| (probe.address, probe.port));
    Ok(crate::DiscoveryReport {
        complete: probes.len() == addresses.len() * APPS.len(),
        probed: probes.len(),
        expected_probes: addresses.len() * APPS.len(),
        results: probes,
    })
}

async fn probe(
    http: &Http,
    address: IpAddr,
    port: u16,
    expected: &'static str,
    path: &str,
    scheme: &str,
) -> Probe {
    let mut probe = Probe {
        address,
        port,
        expected,
        detected: None,
        status: "unreachable",
    };
    let authority = std::net::SocketAddr::new(address, port);
    let Ok(response) = http
        .client()
        .get(format!("{scheme}://{authority}/{path}"))
        .send()
        .await
    else {
        return probe;
    };
    let status = response.status();

    probe.status = if matches!(status.as_u16(), 401 | 403) {
        "authentication_required"
    } else {
        "reachable_unidentified"
    };

    if status == reqwest::StatusCode::OK
        && let Ok(body) = http::bounded(response, 65_536, Intent::Read, None).await
    {
        probe.detected = identity(expected, &body).ok().flatten();

        if probe.detected.is_some() {
            probe.status = "identified";
        }
    }

    probe
}

fn identity(expected: &'static str, body: &[u8]) -> Result<Option<&'static str>, ApiError> {
    let native: Value = serde_json::from_slice(body)?;
    let app = native["appName"].as_str().map(str::to_ascii_lowercase);

    if app.as_deref() == Some(expected) {
        return Ok(Some(expected));
    }

    Ok(None)
}

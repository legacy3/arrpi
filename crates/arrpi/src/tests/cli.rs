//! CLI grammar, scoped HTTP credentials and machine-readable output.

use clap::{CommandFactory as _, Parser as _};
use serde_json::Value;

use crate::{
    cli::{self, Cli, Connection},
    proxy::router,
};

use super::{CLIENT_KEY, gateway, mock, resolve, server};

#[test]
fn clap_commands_have_consistent_arguments_and_version() {
    Cli::command().debug_assert();

    for command in [
        vec!["arrpi", "serve", "--database", "arrpi.sqlite"],
        vec!["arrpi", "discover", "--ip", "127.0.0.1"],
        vec!["arrpi", "discover", "--ip", "::1", "--https"],
        vec!["arrpi", "discover", "--docker-network", "media"],
        vec![
            "arrpi",
            "check-config",
            "--config",
            "tests/settings.example.json",
        ],
        vec!["arrpi", "serve", "--listen", "0.0.0.0:8780"],
        vec![
            "arrpi",
            "get",
            "sonarr",
            "series",
            "--url",
            "http://127.0.0.1:8780/",
        ],
        vec![
            "arrpi",
            "calendar",
            "--url",
            "http://127.0.0.1:8780/",
            "--start",
            "2026-10-01",
            "--end",
            "2026-11-01",
        ],
    ] {
        assert!(
            Cli::try_parse_from(command).is_ok(),
            "valid documented command"
        );
    }

    assert!(
        Cli::try_parse_from([
            "arrpi",
            "get",
            "sonarr",
            "command",
            "--url",
            "http://127.0.0.1:8780/"
        ])
        .is_err(),
        "CLI cannot dispatch mutations"
    );
}

#[tokio::test]
async fn cli_uses_proxy_credentials_and_returns_json_without_banners() {
    let (upstream_server, upstream) = mock(r#"[{"id":1,"title":"Example"}]"#).await;
    let api = server(router(gateway(&upstream_server.url))).await;
    let connection = Connection {
        url: api.url.clone(),
        key_env: "ARRPI_READER_KEY".into(),
        pretty: false,
        metadata: true,
        no_cache: false,
    };
    let mut output = Vec::new();

    cli::fetch(
        &connection,
        "v1/sonarr/api/v3/series",
        "",
        &mut output,
        resolve,
    )
    .await
    .expect("CLI read through proxy");

    let data: Value = serde_json::from_slice(&output).expect("clean machine-readable stdout");

    assert_eq!(
        data["data"][0]["id"], 1,
        "native JSON wrapped only on request"
    );
    assert_eq!(
        data["cache"]["state"], "miss",
        "CLI exposes optional cache metadata"
    );
    assert!(
        !String::from_utf8(output)
            .expect("JSON UTF-8")
            .contains(CLIENT_KEY),
        "client credential never printed"
    );
    assert!(
        upstream.seen.lock().await[0]
            .1
            .get("authorization")
            .is_none(),
        "proxy key not forwarded to native app"
    );
}

#[tokio::test]
async fn cli_rejects_untrusted_origins_and_does_not_follow_redirects() {
    let (upstream_server, upstream) = mock("[]").await;

    upstream
        .status
        .store(302, std::sync::atomic::Ordering::Relaxed);

    let connection = Connection {
        url: upstream_server.url.clone(),
        key_env: "ARRPI_READER_KEY".into(),
        pretty: false,
        metadata: false,
        no_cache: false,
    };
    let mut output = Vec::new();
    let error = cli::fetch(&connection, "v1/calendar", "", &mut output, resolve)
        .await
        .expect_err("reject redirect");

    assert_eq!(
        error.to_string(),
        arrpi_core::error::ApiError::REJECTED.to_string(),
        "error excludes upstream body and URL"
    );
    assert_eq!(
        error
            .downcast_ref::<arrpi_core::error::ApiError>()
            .expect("structured upstream failure")
            .code(),
        "upstream_rejected"
    );
    assert!(output.is_empty(), "failed request produces no partial JSON");

    let connection = Connection {
        url: "http://user:password@localhost/".into(),
        ..connection
    };

    assert!(
        cli::fetch(&connection, "health", "", &mut output, |_| None)
            .await
            .is_err(),
        "credential-bearing URL rejected before network"
    );
}

#[tokio::test]
async fn cli_cache_bypass_returns_new_native_data() {
    let (native, upstream) = mock(r#"[{"title":"Before"}]"#).await;
    let api = server(router(gateway(&native.url))).await;
    let mut connection = Connection {
        url: api.url.clone(),
        key_env: "ARRPI_READER_KEY".into(),
        pretty: false,
        metadata: true,
        no_cache: false,
    };
    let mut output = Vec::new();

    cli::fetch(
        &connection,
        "v1/sonarr/api/v3/series",
        "",
        &mut output,
        resolve,
    )
    .await
    .expect("warm native cache");

    *upstream.body.lock().await = r#"[{"title":"After"}]"#.into();
    output.clear();
    connection.no_cache = true;

    cli::fetch(
        &connection,
        "v1/sonarr/api/v3/series",
        "",
        &mut output,
        resolve,
    )
    .await
    .expect("force a new native read");

    let data: Value = serde_json::from_slice(&output).expect("CLI JSON");

    assert_eq!(
        data["data"][0]["title"], "After",
        "CLI bypasses a fresh cache entry"
    );
    assert_eq!(
        data["cache"]["state"], "miss",
        "CLI preserves gateway cache state"
    );
    assert_eq!(
        upstream.calls.load(std::sync::atomic::Ordering::Relaxed),
        2,
        "exactly one forced upstream read"
    );
}

#[test]
fn discovery_requires_one_explicit_target_and_never_accepts_a_network_range() {
    for args in [
        vec!["arrpi", "discover"],
        vec!["arrpi", "discover", "--ip", "10.0.0.0/24"],
        vec![
            "arrpi",
            "discover",
            "--ip",
            "127.0.0.1",
            "--docker-network",
            "media",
        ],
    ] {
        Cli::try_parse_from(args).expect_err("one explicit IP or Docker network required");
    }
}

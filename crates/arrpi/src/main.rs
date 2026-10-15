//! `ArrPi` process entry point and runtime I/O boundary.

mod auth;
mod calendar;
mod cli;
mod commands;
mod error;
mod feeds;
mod ical;
mod proxy;
mod query;
mod rss;
mod settings;
mod subscriptions;
#[cfg(test)]
mod tests;

mod transport;

use arrpi_core::{config, executor::CommandExecutor as _};
use std::{io::Write as _, process::ExitCode};

use clap::Parser as _;
use cli::{Cli, Command};
use config::Config;
use proxy::Gateway;
use tracing_subscriber::{
    EnvFilter, Layer as _, layer::SubscriberExt as _, util::SubscriberInitExt as _,
};

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "arrpi=info".into()))
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_filter(tracing_subscriber::filter::filter_fn(|metadata| {
                    !metadata.target().starts_with("oauth_resource_server")
                })),
        )
        .init();

    match run().await {
        Ok(()) => ExitCode::SUCCESS,

        Err(error) => {
            let _ = writeln!(std::io::stderr().lock(), "ArrPi: {error}");

            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Command::Discover {
            ip,
            docker_network,
            https,
        } => run_discover(ip, docker_network.as_deref(), https).await?,

        Command::Serve { database, listen } => run_server(database, listen).await?,

        Command::Config { action } => run_config(action).await?,

        Command::CheckConfig { config } => {
            let text = std::fs::read_to_string(config)
                .map_err(|error| format!("cannot read configuration file: {}", error.kind()))?;

            Config::parse(&text)?;
            writeln!(std::io::stdout().lock(), "{{\"valid\":true}}")?;
        }

        Command::Get {
            source,
            endpoint,
            connection,
            query,
        } => {
            if source.is_empty()
                || source.len() > 32
                || !source
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            {
                return Err("invalid source name".into());
            }

            let query = query.encode(endpoint.name())?;

            cli::fetch(
                &connection,
                &format!("v1/{source}/api/v3/{}", endpoint.name()),
                &query,
                &mut std::io::stdout(),
                |name| std::env::var(name).ok(),
            )
            .await?;
        }

        Command::Calendar {
            connection,
            start,
            end,
        } => {
            let query = cli::Query {
                start: Some(start),
                end: Some(end),
                page: None,
                page_size: None,
            }
            .encode("calendar")?;

            cli::fetch(
                &connection,
                "v1/calendar",
                &query,
                &mut std::io::stdout(),
                |name| std::env::var(name).ok(),
            )
            .await?;
        }

        Command::Mcp { connection } => {
            let key = std::env::var(&connection.key_env).map_err(|error| {
                tracing::debug!(
                    event = "missing_stdio_credential",
                    category = std::any::type_name_of_val(&error)
                );
                config::ConfigError::CREDENTIAL
            })?;
            let remote =
                arrpi_adapters::remote::RemoteApi::new(&connection.url, connection.no_cache)?;

            arrpi_mcp::serve_stdio(arrpi_core::executor::Executor::new(remote), key.into()).await?;
        }

        Command::Tool {
            connection,
            name,
            arguments,
        } => run_tool(connection, name, arguments).await?,

        Command::Health { connection } => {
            cli::fetch(&connection, "health", "", &mut std::io::stdout(), |_| None).await?;
        }
    }

    Ok(())
}

async fn shutdown() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};

        match signal(SignalKind::terminate()) {
            Ok(mut terminate) => {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {},
                    _ = terminate.recv() => {},
                }
            }

            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }

    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

fn build_gateway(
    config: &Config,
    database: &std::path::Path,
    store: settings::Store,
) -> Result<Gateway, error::ApiError> {
    let subscriptions = if config.subscriptions.enabled {
        let connection = rusqlite::Connection::open(database).map_err(|error| {
            tracing::debug!(event="feed_database_open", code=?error.sqlite_error_code());
            error::ApiError::INTERNAL
        })?;
        let mut namespace = [0_u8; 32];

        getrandom::fill(&mut namespace).map_err(|error| {
            tracing::debug!(event="feed_entropy", category=%error);
            error::ApiError::INTERNAL
        })?;
        Some(std::sync::Arc::new(
            subscriptions::Store::new(
                connection.into(),
                config.subscriptions.master_key.clone(),
                config.subscriptions.clone(),
                || Ok(chrono::Utc::now().date_naive()),
                namespace,
            )
            .map_err(|error| {
                tracing::debug!(event="feed_store_init", category=%error);
                error::ApiError::INTERNAL
            })?,
        ))
    } else {
        None
    };

    let events =
        arrpi_core::executor::Events::new(arrpi_adapters::storage::SqliteEvents::new(store));

    Gateway::with_state(
        config,
        tokio::time::Instant::now,
        Some(events),
        subscriptions,
        std::env::var("ARRPI_MCP_PROXY_HOST").ok(),
    )
    .map_err(|error| {
        tracing::debug!(event="settings_gateway",category=%error);
        error::ApiError::BAD_QUERY
    })
}

fn open_settings(
    database: &std::path::Path,
) -> Result<settings::Store, Box<dyn std::error::Error>> {
    if let Some(parent) = database
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }

    let connection = rusqlite::Connection::open(database).map_err(|error| {
        tracing::debug!(event="settings_open", code=?error.sqlite_error_code());
        "cannot open application database"
    })?;

    #[cfg(unix)]
    protect_database(database)?;
    Ok(settings::Store::new(connection.into())?)
}

fn fresh_config() -> Result<Config, Box<dyn std::error::Error>> {
    let config = serde_json::json!({"listen":"127.0.0.1:8780", "sources": {}, "clients":[{"name":"admin", "key":random_key()?, "permissions":["admin"]}], "subscriptions":{"master_key":random_key()?}});

    Ok(serde_json::from_value(config)?)
}

fn random_key() -> Result<String, Box<dyn std::error::Error>> {
    use base64::Engine as _;
    let mut bytes = [0_u8; 32];

    getrandom::fill(&mut bytes).map_err(|error| {
        tracing::debug!(event="settings_entropy", category=%error);
        "cannot generate application credential"
    })?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

async fn run_config(action: cli::ConfigCommand) -> Result<(), Box<dyn std::error::Error>> {
    match action {
        cli::ConfigCommand::Get { connection } => {
            cli::fetch(
                &connection,
                "v1/config",
                "",
                &mut std::io::stdout(),
                |name| std::env::var(name).ok(),
            )
            .await?;
        }

        cli::ConfigCommand::Set { connection, file } => {
            let body = std::fs::read_to_string(file)?;

            cli::send(
                &connection,
                cli::Operation {
                    path: "v1/config",
                    query: "",
                    body: Some(&body),
                },
                &mut std::io::stdout(),
                |name| std::env::var(name).ok(),
            )
            .await?;
        }

        cli::ConfigCommand::Import { database, file } => {
            let text = std::fs::read_to_string(file)?;
            let config = Config::parse(&text)?;

            Gateway::new(&config, tokio::time::Instant::now)?;
            let store = open_settings(&database)?;

            if let Some(revision) = store.revision()? {
                store.save(revision, &config)?;
            } else {
                store.seed(&config)?;
            }

            writeln!(std::io::stdout().lock(), "{{\"saved\":true}}")?;
        }

        cli::ConfigCommand::Key { database, name } => {
            let store = open_settings(&database)?;
            let (_, config) = store.load()?.ok_or("database is not initialized")?;
            let key = config
                .clients
                .iter()
                .find(|client| client.name == name)
                .ok_or("unknown client")?;

            writeln!(std::io::stdout().lock(), "{}", key.key.value())?;
        }
    }

    Ok(())
}

#[cfg(unix)]
fn protect_database(path: &std::path::Path) -> Result<(), config::ConfigError> {
    use std::os::unix::fs::PermissionsExt as _;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(|error| {
        tracing::debug!(event = "feed_file_permissions", category = ?error.kind());
        config::ConfigError::invalid("cannot protect subscription storage")
    })
}

async fn run_tool(
    connection: cli::Connection,
    name: String,
    arguments: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let key = std::env::var(&connection.key_env).map_err(|error| {
        tracing::debug!(
            event = "command_failure",
            category = std::any::type_name_of_val(&error)
        );
        config::ConfigError::CREDENTIAL
    })?;
    let args = serde_json::from_str(&arguments).map_err(|error| {
        tracing::debug!(
            event = "command_failure",
            category = std::any::type_name_of_val(&error)
        );
        "invalid tool arguments"
    })?;
    let tool = arrpi_core::registry::ToolId::parse(&name).ok_or(error::ApiError::NOT_FOUND)?;
    let value = arrpi_adapters::remote::RemoteApi::new(&connection.url, connection.no_cache)?
        .execute(&key, tool, args)
        .await?;

    serde_json::to_writer(std::io::stdout().lock(), &value)?;
    writeln!(std::io::stdout().lock())?;
    Ok(())
}

async fn discovery_addresses(
    ip: Option<std::net::IpAddr>,
    network: Option<&str>,
) -> Result<Vec<std::net::IpAddr>, Box<dyn std::error::Error>> {
    if let Some(ip) = ip {
        return Ok(vec![ip]);
    }

    let network = network
        .filter(|network| {
            !network.is_empty()
                && network.len() <= 128
                && network
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        })
        .ok_or("invalid Docker network name")?;
    let output = inspect_docker_network(network).await?;

    let containers: std::collections::BTreeMap<String, serde_json::Value> =
        serde_json::from_slice(&output).map_err(|error| {
            tracing::debug!(event="docker_network_json", category=?error.classify());
            "invalid Docker network response"
        })?;

    if containers.is_empty() || containers.len() > 64 {
        return Err("discover supports 1-64 containers on one explicit network".into());
    }

    let mut addresses = std::collections::BTreeSet::new();

    for container in containers.values() {
        for field in ["IPv4Address", "IPv6Address"] {
            if let Some(value) = container[field].as_str().filter(|value| !value.is_empty()) {
                let address = value
                    .split('/')
                    .next()
                    .ok_or("invalid Docker address")?
                    .parse::<std::net::IpAddr>()
                    .map_err(|error| invalid_docker_address(&error))?;

                addresses.insert(address);
            }
        }
    }

    if addresses.len() > 64 {
        return Err("discover supports at most 64 addresses".into());
    }

    Ok(addresses.into_iter().collect())
}

async fn run_discover(
    ip: Option<std::net::IpAddr>,
    network: Option<&str>,
    https: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let addresses = discovery_addresses(ip, network).await?;
    let result = arrpi_adapters::discover(
        &addresses,
        if https { "https" } else { "http" },
        std::time::Instant::now,
    )
    .await?;

    serde_json::to_writer_pretty(std::io::stdout().lock(), &result)?;
    writeln!(std::io::stdout().lock())?;

    if !result.complete {
        return Err("discovery deadline reached; results are incomplete".into());
    }

    Ok(())
}

async fn run_server(
    database: std::path::PathBuf,
    listen: Option<std::net::SocketAddr>,
) -> Result<(), Box<dyn std::error::Error>> {
    let store = open_settings(&database)?;

    if store.load()?.is_none() {
        store.seed(&fresh_config()?)?;
    }

    let (_, config) = store.load()?.ok_or("missing settings")?;
    let listen = listen.unwrap_or(config.listen);
    let runtime = settings::Runtime::new(std::sync::Arc::new(store), database, build_gateway)?;
    let listener = tokio::net::TcpListener::bind(listen).await?;

    tracing::info!(event = "listening", address = %listener.local_addr()?);
    let listener =
        transport::LimitedListener::new(listener, config.cache, tokio::time::Instant::now);

    axum::serve(listener, runtime.router())
        .with_graceful_shutdown(shutdown())
        .await?;

    Ok(())
}

fn invalid_docker_address(error: &std::net::AddrParseError) -> &'static str {
    tracing::debug!(
        event = "docker_network_address",
        category = std::any::type_name_of_val(error)
    );
    "invalid Docker address"
}

async fn inspect_docker_network(network: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    use tokio::io::AsyncReadExt as _;
    let mut child = tokio::process::Command::new("docker")
        .args([
            "network",
            "inspect",
            "--format",
            "{{json .Containers}}",
            "--",
            network,
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| {
            tracing::debug!(
                event = "docker_network_inspect",
                category = std::any::type_name_of_val(&error)
            );
            "cannot inspect Docker network; Docker CLI access is required on this machine"
        })?;
    let stdout = child
        .stdout
        .take()
        .ok_or("cannot read Docker network response")?;
    let operation = async {
        let mut output = Vec::with_capacity(4096);

        stdout
            .take(262_145)
            .read_to_end(&mut output)
            .await
            .map_err(|error| {
                tracing::debug!(
                    event = "docker_network_response",
                    category = std::any::type_name_of_val(&error)
                );
                "cannot read Docker network response"
            })?;

        if output.len() > 262_144 {
            return Err("Docker network response exceeds discovery limit");
        }

        let status = child.wait().await.map_err(|error| {
            tracing::debug!(
                event = "docker_network_wait",
                category = std::any::type_name_of_val(&error)
            );
            "cannot inspect Docker network"
        })?;

        if !status.success() {
            return Err("cannot inspect Docker network");
        }

        Ok(output)
    };
    let output = tokio::time::timeout(std::time::Duration::from_secs(5), operation)
        .await
        .map_err(|error| {
            tracing::debug!(
                event = "docker_network_timeout",
                category = std::any::type_name_of_val(&error)
            );
            "Docker network inspection timed out"
        })??;

    Ok(output)
}

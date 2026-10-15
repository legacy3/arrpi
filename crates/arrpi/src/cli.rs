//! Clap commands and a bounded HTTP client for agents and shell pipelines.

use crate::config::ConfigError;
use arrpi_adapters::remote::ApiClient;
use clap::{Parser, Subcommand, ValueEnum};
use std::{net::SocketAddr, path::PathBuf};

#[derive(Debug, Parser)]
#[command(
    version,
    about = "Cached media APIs, calendar and account-bound Seerr requests"
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Probe common app ports on an explicit IP or Docker network, without credentials.
    Discover {
        /// Probe HTTPS instead of HTTP (normal certificate validation applies).
        #[arg(long)]
        https: bool,
        #[arg(
            long,
            required_unless_present = "docker_network",
            conflicts_with = "docker_network"
        )]
        ip: Option<std::net::IpAddr>,
        #[arg(long, required_unless_present = "ip", conflicts_with = "ip")]
        docker_network: Option<String>,
    },
    /// Run the cached API server.
    Serve {
        /// Private `SQLite` application database.
        #[arg(long, env = "ARRPI_DATABASE", default_value = "/state/arrpi.sqlite")]
        database: PathBuf,
        /// Override the configured bind address, for container networking.
        #[arg(long, env = "ARRPI_LISTEN")]
        listen: Option<SocketAddr>,
    },
    /// Manage database configuration locally.
    Config {
        #[command(subcommand)]
        action: ConfigCommand,
    },
    /// Validate configuration syntax and policy without starting a server.
    CheckConfig {
        #[arg(long, default_value = "arrpi-import.json")]
        config: PathBuf,
    },
    /// Read an allowlisted native endpoint through an `ArrPi` server.
    Get {
        source: String,
        #[arg(value_enum)]
        endpoint: Endpoint,
        #[command(flatten)]
        connection: Connection,
        #[command(flatten)]
        query: Query,
    },
    /// Read the combined calendar for all authorized sources.
    Calendar {
        #[command(flatten)]
        connection: Connection,
        #[arg(long)]
        start: String,
        #[arg(long)]
        end: String,
    },
    /// Run the MCP stdio adapter against the central authenticated server.
    Mcp {
        #[command(flatten)]
        connection: Connection,
    },
    /// Invoke the same typed command used by MCP tools.
    Tool {
        name: String,
        #[arg(long, default_value = "{}")]
        arguments: String,
        #[command(flatten)]
        connection: Connection,
    },
    /// Check process liveness; this endpoint does not require credentials.
    Health {
        #[command(flatten)]
        connection: Connection,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum ConfigCommand {
    /// Read the active server settings, with credentials redacted.
    Get {
        #[command(flatten)]
        connection: Connection,
    },
    /// Apply a revisioned settings document through the same API as the web UI.
    Set {
        #[command(flatten)]
        connection: Connection,
        #[arg(long)]
        file: PathBuf,
    },

    /// Import settings into the database. Stop the server before an offline import.
    Import {
        #[arg(long, env = "ARRPI_DATABASE", default_value = "/state/arrpi.sqlite")]
        database: PathBuf,
        #[arg(long)]
        file: PathBuf,
    },
    /// Print a stored client credential for local setup or recovery.
    Key {
        #[arg(long, env = "ARRPI_DATABASE", default_value = "/state/arrpi.sqlite")]
        database: PathBuf,
        #[arg(long, default_value = "admin")]
        name: String,
    },
}

#[derive(Debug, clap::Args)]
pub(crate) struct Connection {
    /// `ArrPi` base URL, without credentials, query or fragment.
    #[arg(long, env = "ARRPI_URL")]
    pub(crate) url: String,
    /// Environment variable containing the scoped `ArrPi` client key.
    #[arg(long, default_value = "ARRPI_API_KEY")]
    pub(crate) key_env: String,
    /// Indent JSON output.
    #[arg(long)]
    pub(crate) pretty: bool,
    /// Wrap native JSON with cache state and age metadata.
    #[arg(long)]
    pub(crate) metadata: bool,
    /// Bypass the gateway cache and require a new upstream result.
    #[arg(long)]
    pub(crate) no_cache: bool,
}

#[derive(Debug, clap::Args)]
pub(crate) struct Query {
    #[arg(long, requires = "end")]
    pub(crate) start: Option<String>,
    #[arg(long, requires = "start")]
    pub(crate) end: Option<String>,
    #[arg(long)]
    pub(crate) page: Option<u16>,
    #[arg(long)]
    pub(crate) page_size: Option<u16>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum Endpoint {
    Calendar,
    Series,
    Movie,
    Queue,
    Health,
    Qualityprofile,
}

impl Endpoint {
    pub(crate) const fn name(self) -> &'static str {
        use arrpi_core::contracts::Endpoint as Wire;
        let endpoint = match self {
            Self::Calendar => Wire::Calendar,
            Self::Series => Wire::Series,
            Self::Movie => Wire::Movie,
            Self::Queue => Wire::Queue,
            Self::Health => Wire::Health,
            Self::Qualityprofile => Wire::Qualityprofile,
        };

        endpoint.name()
    }
}

impl Query {
    pub(crate) fn encode(&self, endpoint: &str) -> Result<String, crate::error::ApiError> {
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());

        for (key, value) in [
            ("start", self.start.clone()),
            ("end", self.end.clone()),
            ("page", self.page.map(|page| page.to_string())),
            ("pageSize", self.page_size.map(|size| size.to_string())),
        ] {
            if let Some(value) = value {
                serializer.append_pair(key, &value);
            }
        }

        let raw = serializer.finish();

        // Validate locally; the server adds its own mandatory calendar flags.
        crate::query::canonical(endpoint, Some(&raw))?;
        Ok(raw)
    }
}

pub(crate) struct Operation<'a> {
    pub(crate) path: &'a str,
    pub(crate) query: &'a str,
    pub(crate) body: Option<&'a str>,
}

impl std::fmt::Debug for Operation<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Operation").finish_non_exhaustive()
    }
}

pub(crate) async fn fetch(
    connection: &Connection,
    path: &str,
    query: &str,
    output: &mut impl std::io::Write,
    resolve: impl Fn(&str) -> Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    send(
        connection,
        Operation {
            path,
            query,
            body: None,
        },
        output,
        resolve,
    )
    .await
}

pub(crate) async fn send(
    connection: &Connection,
    operation: Operation<'_>,
    output: &mut impl std::io::Write,
    resolve: impl Fn(&str) -> Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let Operation { path, query, body } = operation;
    let client = ApiClient::new(&connection.url, connection.no_cache)?;
    let credential = if path == "health" {
        None
    } else {
        Some(resolve(&connection.key_env).ok_or(ConfigError::CREDENTIAL)?)
    };
    let response = if let Some(body) = body {
        if path != "v1/config" || !query.is_empty() {
            return Err("only configuration updates are supported".into());
        }

        client
            .put_config(
                credential.as_deref().ok_or(ConfigError::CREDENTIAL)?,
                body.as_bytes(),
            )
            .await?
    } else {
        client.get(credential.as_deref(), path, query).await?
    };
    let data = if connection.metadata {
        serde_json::json!({"data":response.data,"cache":{"state":response.cache_state,"age_seconds":response.age_seconds}})
    } else {
        response.data
    };

    if connection.pretty {
        serde_json::to_writer_pretty(&mut *output, &data)?;
    } else {
        serde_json::to_writer(&mut *output, &data)?;
    }

    writeln!(output)?;
    Ok(())
}

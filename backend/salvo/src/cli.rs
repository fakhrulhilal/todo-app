//! Command-line arguments: one of the configuration sources (`config.rs`).
//!
//! Every setting has one name, its key in the YAML file, and the other two
//! follow from it:
//!
//! | YAML key       | environment variable | argument         |
//! |----------------|----------------------|------------------|
//! | `database.url` | `DATABASE__URL`      | `--database-url` |
//!
//! The variable is the key in upper case with each dot as `__`; the argument
//! is the key with each dot and underscore as `-`.

use std::path::PathBuf;

use clap::{Args, Parser};
use serde::Serialize;

use crate::config::{Environment, LogFormat};

#[derive(Debug, Default, Parser)]
#[command(
    version,
    about,
    after_help = "Every setting can also be given in the YAML file or as an environment \
                  variable:\n--server-port is `server.port` in the file and SERVER__PORT in \
                  the environment.\nArguments win over variables, variables over the file."
)]
pub struct Cli {
    /// YAML configuration file [default: config.yaml, when it exists]
    #[arg(short, long, env = "CONFIG", value_name = "FILE")]
    pub config: Option<PathBuf>,

    #[command(flatten)]
    pub settings: Settings,
}

/// The settings given as arguments, in the shape of `Config` so they merge
/// over the other sources. Arguments that were not given are left out.
#[derive(Debug, Default, Args, Serialize)]
pub struct Settings {
    /// Runtime environment
    #[arg(long, value_enum)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<Environment>,

    #[command(flatten)]
    pub server: ServerArgs,

    #[command(flatten)]
    pub database: DatabaseArgs,

    #[command(flatten)]
    pub log: LogArgs,
}

#[derive(Debug, Default, Args, Serialize)]
pub struct ServerArgs {
    /// Address to listen on
    #[arg(long = "server-host", value_name = "HOST")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,

    /// Port to listen on
    #[arg(short = 'p', long = "server-port", value_name = "PORT")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,

    /// Sites whose pages may call the API, comma-separated, or `*` for any
    #[arg(long = "server-cors-origins", value_name = "ORIGINS")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cors_origins: Option<String>,
}

#[derive(Debug, Default, Args, Serialize)]
pub struct DatabaseArgs {
    /// Database URL: sqlite://<path>, sqlite::memory: or postgres://...
    #[arg(long = "database-url", value_name = "URL")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    /// PostgreSQL connection pool size
    #[arg(long = "database-max-connections", value_name = "COUNT")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_connections: Option<u32>,

    /// Apply the pending migrations on startup: true or false
    #[arg(long = "database-migration", value_name = "BOOL")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migration: Option<bool>,
}

#[derive(Debug, Default, Args, Serialize)]
pub struct LogArgs {
    /// Log level or filter directives, e.g. `debug` or `info,sqlx=warn`
    #[arg(long = "log-level", value_name = "LEVEL")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,

    /// Log output format
    #[arg(long = "log-format", value_enum)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<LogFormat>,
}

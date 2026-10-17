//! Configuration, in the manner of .NET's options pattern: sources are
//! merged in order, the later one winning, and the result is bound to the
//! structs below. The order is: built-in defaults, the YAML file,
//! environment variables, command-line arguments.
//!
//! `Config` is what the application reads. `cli.rs` has the naming convention
//! that ties the three spellings of a setting together.

use std::path::Path;

use clap::ValueEnum;
use figment::{
    Figment,
    providers::{Env, Format, Serialized, Yaml},
};
use serde::{Deserialize, Serialize};

use crate::cli::Cli;

const DEFAULT_CONFIG_FILE: &str = "config.yaml";
const DEFAULTS: &str = include_str!("../config/default.yaml");

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub env: Environment,
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub log: LogConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    Local,
    Production,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    /// Origins allowed to call the API from a browser, comma-separated.
    pub cors_origins: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
    /// Connection pool size. Only PostgreSQL uses a pool.
    pub max_connections: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LogConfig {
    pub level: String,
    pub format: LogFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    /// Plain text locally, JSON lines in production.
    Auto,
    Text,
    Json,
}

impl Config {
    pub fn load(cli: &Cli) -> Result<Self, Box<figment::Error>> {
        let defaults = Figment::from(Yaml::string(DEFAULTS));

        // A file that was asked for must exist; the default one need not.
        let file = match &cli.config {
            Some(path) => Figment::from(Yaml::file_exact(path)),
            None if Path::new(DEFAULT_CONFIG_FILE).exists() => {
                Figment::from(Yaml::file_exact(DEFAULT_CONFIG_FILE))
            }
            None => Figment::new(),
        };

        // SERVER__PORT is server.port. Only variables that name a setting
        // are read, so the rest of the environment cannot interfere.
        let keys = keys(&defaults);
        let environment = Env::raw()
            .filter(move |name| {
                keys.iter()
                    .any(|key| name == key.replace('.', "__").as_str())
            })
            .split("__");

        defaults
            .merge(file)
            .merge(environment)
            .merge(Serialized::defaults(&cli.settings))
            .extract()
            .map_err(Box::new)
    }
}

/// Every setting, by its key in the YAML file: `server.port`, `log.level`...
/// The built-in defaults name them all.
pub fn keys(defaults: &Figment) -> Vec<String> {
    fn walk(prefix: &str, value: &figment::value::Value, keys: &mut Vec<String>) {
        match value.as_dict() {
            Some(dict) => {
                for (name, value) in dict {
                    let key = match prefix {
                        "" => name.clone(),
                        _ => format!("{prefix}.{name}"),
                    };
                    walk(&key, value, keys);
                }
            }
            None => keys.push(prefix.to_owned()),
        }
    }

    let mut keys = Vec::new();
    if let Ok(value) = defaults.extract::<figment::value::Value>() {
        walk("", &value, &mut keys);
    }
    keys
}

#[cfg(test)]
// figment's test jail hands its own (large) error type to the closures.
#[allow(clippy::result_large_err)]
mod tests {
    use clap::{CommandFactory, Parser};
    use figment::Jail;

    use super::*;

    fn load(arguments: &[&str]) -> Result<Config, figment::Error> {
        Config::load(&Cli::parse_from([&["todo-backend"], arguments].concat()))
            .map_err(|error| *error)
    }

    #[test]
    fn every_setting_has_an_argument_named_after_its_key() {
        let command = Cli::command();
        let keys = keys(&Figment::from(Yaml::string(DEFAULTS)));
        assert!(keys.contains(&"database.max_connections".to_owned()));

        for key in &keys {
            let long = key.replace(['.', '_'], "-");
            assert!(
                command
                    .get_arguments()
                    .any(|argument| argument.get_long() == Some(long.as_str())),
                "`{key}` has no --{long} argument"
            );
        }
        // And no argument without a setting, apart from --config.
        let arguments = command
            .get_arguments()
            .filter(|argument| argument.get_id() != "config")
            .count();
        assert_eq!(arguments, keys.len());
    }

    #[test]
    fn later_sources_win() {
        Jail::expect_with(|jail| {
            jail.clear_env();

            let config = load(&[])?;
            assert_eq!(config.server.port, 8080);
            assert_eq!(config.env, Environment::Local);

            jail.create_file(
                "config.yaml",
                "server:\n  port: 9001\n  host: 0.0.0.0\ndatabase:\n  max_connections: 3\n",
            )?;
            let config = load(&[])?;
            assert_eq!(config.server.port, 9001);

            jail.set_env("SERVER__PORT", 9002);
            jail.set_env("DATABASE__MAX_CONNECTIONS", 4);
            jail.set_env("ENV", "production");
            let config = load(&[])?;
            assert_eq!(config.server.port, 9002);
            assert_eq!(config.database.max_connections, 4);
            assert_eq!(config.env, Environment::Production);
            // Still from the file.
            assert_eq!(config.server.host, "0.0.0.0");

            let config = load(&["--server-port", "9003", "--log-format", "json"])?;
            assert_eq!(config.server.port, 9003);
            assert_eq!(config.log.format, LogFormat::Json);
            assert_eq!(config.database.max_connections, 4);
            // And the defaults fill in the rest.
            assert_eq!(config.log.level, "info");
            Ok(())
        });
    }

    #[test]
    fn other_environment_variables_are_not_settings() {
        Jail::expect_with(|jail| {
            jail.clear_env();
            jail.set_env("PORT", 1);
            jail.set_env("SERVER__", "x");
            jail.set_env("__SERVER__PORT", 2);
            assert_eq!(load(&[])?.server.port, 8080);
            Ok(())
        });
    }

    #[test]
    fn a_bad_value_names_the_setting() {
        Jail::expect_with(|jail| {
            jail.clear_env();
            jail.set_env("SERVER__PORT", "eighty");
            let error = load(&[]).unwrap_err().to_string();
            // The message names the setting and where the value came from.
            assert!(error.to_lowercase().contains("server.port"), "{error}");
            assert!(error.contains("environment variable"), "{error}");

            assert!(load(&["--config", "missing.yaml"]).is_err());
            Ok(())
        });
    }
}

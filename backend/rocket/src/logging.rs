//! Log output: plain text for people, one JSON object per line for collectors.

use std::io::IsTerminal;

use anyhow::Context;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use crate::config::{Config, Environment, LogFormat};

/// Rocket logs through the `log` crate, which tracing-subscriber forwards.
/// Its lines repeat what `RequestLog` says (and call every 404 an error), so
/// they are off unless `log.level` names the target, as in `info,rocket=info`.
const ROCKET_OFF: &str = "rocket=off";

pub fn init(config: &Config) -> anyhow::Result<()> {
    let mut filter = EnvFilter::try_new(&config.log.level)
        .with_context(|| format!("invalid log level `{}`", config.log.level))?;
    if !config.log.level.contains("rocket") {
        filter = filter.add_directive(ROCKET_OFF.parse()?);
    }

    let json = match config.log.format {
        LogFormat::Json => true,
        LogFormat::Text => false,
        LogFormat::Auto => config.env == Environment::Production,
    };

    let registry = tracing_subscriber::registry().with(filter);
    if json {
        registry
            .with(fmt::layer().json().flatten_event(true))
            .try_init()?;
    } else {
        registry
            .with(fmt::layer().with_ansi(std::io::stdout().is_terminal()))
            .try_init()?;
    }
    Ok(())
}

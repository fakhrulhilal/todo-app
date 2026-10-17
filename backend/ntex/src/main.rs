use std::process::ExitCode;

use anyhow::Context;
use clap::Parser;
use todo_backend_ntex::{cli::Cli, config::Config, logging};

/// Starts the compio runtime on this thread; the server starts one more per
/// worker.
#[ntex::main]
async fn main() -> ExitCode {
    match run(Cli::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    let config = Config::load(&cli).context("invalid configuration")?;
    logging::init(&config)?;
    todo_backend_ntex::run(config).await
}

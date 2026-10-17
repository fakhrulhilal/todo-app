use std::process::ExitCode;

use anyhow::Context;
use clap::Parser;
use todo_backend::{cli::Cli, config::Config, logging};

#[tokio::main]
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
    todo_backend::run(config).await
}

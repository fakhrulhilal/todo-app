use std::process::ExitCode;

use anyhow::Context;
use clap::Parser;
use todo_backend_actix::{cli::Cli, config::Config, logging};

#[actix_web::main]
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
    todo_backend_actix::run(config).await
}

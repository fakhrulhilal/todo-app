//! A todo list, server-rendered with Topcoat.

pub mod actions;
pub mod cli;
pub mod config;
pub mod domain;
pub mod health;
pub mod logging;
pub mod store;
pub mod web;

use std::time::Instant;

use anyhow::Context;
use axum::{
    Router,
    extract::Request,
    http::header::CONTENT_TYPE,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
};
use tokio::net::TcpListener;
use topcoat::{asset::AssetBundle, router::tower::TowerService};

use crate::{config::Config, store::Store};

pub const STYLESHEET_PATH: &str = "/assets/app.css";

/// The whole application: the health probes and the stylesheet, with
/// everything else falling through to the web UI.
pub fn app(store: Store, assets: AssetBundle) -> Router {
    health::router(store.clone())
        .route(STYLESHEET_PATH, get(stylesheet))
        .fallback_service(TowerService::new(web::router(store, assets)))
        .layer(middleware::from_fn(log_request))
}

pub async fn run(config: Config) -> anyhow::Result<()> {
    let store = store::connect(&config.database)
        .await
        .context("failed to open the database")?;
    // Produced next to the binary by `topcoat asset bundle`.
    let assets = AssetBundle::load()
        .context("no asset bundle next to the executable: run `topcoat asset bundle`")?;

    let address = format!("{}:{}", config.server.host, config.server.port);
    let listener = TcpListener::bind(&address)
        .await
        .with_context(|| format!("failed to listen on {address}"))?;
    tracing::info!(
        address = %listener.local_addr()?,
        env = ?config.env,
        "server started"
    );

    axum::serve(listener, app(store, assets))
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    tracing::info!("server stopped");
    Ok(())
}

async fn stylesheet() -> impl IntoResponse {
    (
        [(CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("web/style.css"),
    )
}

/// One log line per request. Probes log at debug so they do not drown the rest.
async fn log_request(request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let started = Instant::now();

    let response = next.run(request).await;

    let status = response.status().as_u16();
    let latency_ms = started.elapsed().as_micros() as f64 / 1000.0;
    if path.starts_with("/health/") {
        tracing::debug!(%method, path, status, latency_ms, "request");
    } else {
        tracing::info!(%method, path, status, latency_ms, "request");
    }
    response
}

/// Resolves on Ctrl+C, or SIGTERM on Unix.
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.ok();
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(_) => std::future::pending().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
}

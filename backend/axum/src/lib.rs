//! The todo list's JSON API, described by an OpenAPI document. The frontend is a separate
//! static site (`frontend/dioxus/`) that calls this API from the browser.

pub mod api;
pub mod cli;
pub mod config;
pub mod domain;
pub mod logging;
pub mod store;

use std::time::Instant;

use anyhow::Context;
use axum::{
    Router,
    extract::Request,
    http::{HeaderValue, Method, header::CONTENT_TYPE},
    middleware::{self, Next},
    response::Response,
};
use tokio::net::TcpListener;
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::{config::Config, store::Store};

/// The whole application. `cors_origins` lists the sites whose pages may call
/// the API from a browser, separated by commas; `*` allows any site.
pub fn app(store: Store, cors_origins: &str) -> anyhow::Result<Router> {
    Ok(api::router(store)
        .layer(cors(cors_origins)?)
        .layer(middleware::from_fn(log_request)))
}

pub async fn run(config: Config) -> anyhow::Result<()> {
    let app = app(
        store::connect(&config.database)
            .await
            .context("failed to open the database")?,
        &config.server.cors_origins,
    )?;

    let address = format!("{}:{}", config.server.host, config.server.port);
    let listener = TcpListener::bind(&address)
        .await
        .with_context(|| format!("failed to listen on {address}"))?;
    tracing::info!(
        address = %listener.local_addr()?,
        env = ?config.env,
        cors_origins = config.server.cors_origins,
        "server started"
    );

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    tracing::info!("server stopped");
    Ok(())
}

/// The web client is served from another origin, so browsers ask for
/// permission before calling the API.
fn cors(origins: &str) -> anyhow::Result<CorsLayer> {
    let origins: Vec<&str> = origins
        .split(',')
        .map(|origin| origin.trim().trim_end_matches('/'))
        .filter(|origin| !origin.is_empty())
        .collect();

    let allow_origin = if origins.contains(&"*") {
        AllowOrigin::any()
    } else {
        let origins = origins
            .into_iter()
            .map(|origin| {
                origin
                    .parse::<HeaderValue>()
                    .with_context(|| format!("invalid CORS origin `{origin}`"))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        AllowOrigin::list(origins)
    };

    Ok(CorsLayer::new()
        .allow_origin(allow_origin)
        .allow_methods([Method::GET, Method::POST, Method::PATCH, Method::DELETE])
        .allow_headers([CONTENT_TYPE]))
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

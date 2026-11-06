//! The todo list's JSON API on actix-web, the same API as `backend/axum/`,
//! described by the same OpenAPI document (`contract/openapi.json`).
//!
//! actix-web runs one single-threaded tokio runtime per worker thread. The
//! application factory passed to `HttpServer::new` builds an `App` on every
//! worker; what they share (the store, the health checks) is made once in
//! `run` and handed to each as the same `web::Data`.

pub mod api;
pub mod cli;
pub mod config;
pub mod domain;
pub mod logging;
pub mod store;

use std::time::Instant;

use actix_cors::Cors;
use actix_web::{
    App, Error, HttpServer,
    body::MessageBody,
    dev::{ServiceFactory, ServiceRequest, ServiceResponse},
    http::{Method, Uri, header::CONTENT_TYPE},
    middleware::{Next, from_fn},
    web::{self, Data, JsonConfig, PathConfig, QueryConfig},
};
use anyhow::Context;

use crate::{
    api::{AppState, problem},
    config::Config,
};

/// The largest JSON body accepted, axum's default (and actix's).
const JSON_LIMIT: usize = 2 * 1024 * 1024;

/// How long open requests may take to finish once SIGTERM arrives: under
/// Docker's 10 seconds before it kills the process.
const SHUTDOWN_TIMEOUT_SECS: u64 = 8;

/// The whole application, shared by `run()` and the tests. It is built on
/// every worker, so it holds nothing that must be made only once: `state` is
/// already shared, and cloning it only bumps a reference count.
///
/// `cors` is taken by value: a borrow would be captured by the `impl` return
/// type (edition 2024), and the factory closure could not return the app.
pub fn app(
    state: Data<AppState>,
    cors: CorsOrigins,
) -> App<
    impl ServiceFactory<
        ServiceRequest,
        Config = (),
        Response = ServiceResponse<impl MessageBody>,
        Error = Error,
        InitError = (),
    >,
> {
    App::new()
        .app_data(state)
        // The extractors' errors become problems (`api/problem.rs`).
        .app_data(
            JsonConfig::default()
                .limit(JSON_LIMIT)
                .error_handler(problem::json_error),
        )
        .app_data(PathConfig::default().error_handler(problem::path_error))
        .app_data(QueryConfig::default().error_handler(problem::query_error))
        .configure(api::routes)
        .default_service(web::to(problem::unknown_endpoint))
        // The last `wrap` is the outermost: logging sees the finished
        // response, and the problem middleware sees CORS's errors.
        .wrap(cors.middleware())
        .wrap(from_fn(problem::problem_details))
        .wrap(from_fn(log_request))
}

pub async fn run(config: Config) -> anyhow::Result<()> {
    let cors = CorsOrigins::parse(&config.server.cors_origins)?;
    // Made once and shared by every worker.
    let state = Data::new(AppState::new(
        store::connect(&config.database)
            .await
            .context("failed to open the database")?,
    ));

    let address = format!("{}:{}", config.server.host, config.server.port);
    // actix handles the signals: SIGTERM (`docker stop`) lets open requests
    // finish, Ctrl+C (SIGINT) and SIGQUIT stop at once.
    let server = HttpServer::new(move || app(state.clone(), cors.clone()))
        .shutdown_timeout(SHUTDOWN_TIMEOUT_SECS)
        .bind(&address)
        .with_context(|| format!("failed to listen on {address}"))?;
    let local_address = server
        .addrs()
        .first()
        .map_or(address, |address| address.to_string());
    tracing::info!(
        address = local_address,
        env = ?config.env,
        cors_origins = config.server.cors_origins,
        "server started"
    );

    server.run().await?;
    tracing::info!("server stopped");
    Ok(())
}

/// The sites whose pages may call the API from a browser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CorsOrigins {
    /// `*`: any site.
    Any,
    Only(Vec<String>),
}

impl CorsOrigins {
    /// `origins` is a comma-separated list, or `*` for any site.
    pub fn parse(origins: &str) -> anyhow::Result<Self> {
        let origins: Vec<&str> = origins
            .split(',')
            .map(|origin| origin.trim().trim_end_matches('/'))
            .filter(|origin| !origin.is_empty())
            .collect();

        if origins.contains(&"*") {
            return Ok(Self::Any);
        }
        // An origin is `scheme://host[:port]`. actix-cors would only reject
        // what `Uri` cannot parse, and then by failing every worker's app at
        // startup with nothing but a log line.
        let origins = origins
            .into_iter()
            .map(|origin| match origin.parse::<Uri>() {
                Ok(uri) if uri.scheme().is_some() && uri.host().is_some() => Ok(origin.to_owned()),
                _ => anyhow::bail!("invalid CORS origin `{origin}`"),
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        Ok(Self::Only(origins))
    }

    /// The web client is served from another origin, so browsers ask for
    /// permission before calling the API. As with tower-http in the axum
    /// backend, a request from an origin that is not listed is served without
    /// the CORS headers (actix-cors's `block_on_origin_mismatch` is off by
    /// default), and `*` answers `Access-Control-Allow-Origin: *`.
    fn middleware(&self) -> Cors {
        let cors = Cors::default()
            .allowed_methods([Method::GET, Method::POST, Method::PATCH, Method::DELETE])
            .allowed_header(CONTENT_TYPE);
        match self {
            Self::Any => cors.allow_any_origin().send_wildcard(),
            Self::Only(origins) => origins
                .iter()
                .fold(cors, |cors, origin| cors.allowed_origin(origin)),
        }
    }
}

/// One log line per request. Probes log at debug so they do not drown the rest.
async fn log_request(
    request: ServiceRequest,
    next: Next<impl MessageBody>,
) -> Result<ServiceResponse<impl MessageBody>, Error> {
    let method = request.method().clone();
    let path = request.path().to_owned();
    let started = Instant::now();

    // The problem middleware inside turns every failure into a response.
    let response = next.call(request).await?;

    let status = response.status().as_u16();
    let latency_ms = started.elapsed().as_micros() as f64 / 1000.0;
    if path.starts_with("/health/") {
        tracing::debug!(%method, path, status, latency_ms, "request");
    } else {
        tracing::info!(%method, path, status, latency_ms, "request");
    }
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cors_origins_are_checked_at_startup() {
        assert_eq!(CorsOrigins::parse(" * ").unwrap(), CorsOrigins::Any);
        assert_eq!(
            CorsOrigins::parse("https://a.example/, http://b.example:8080").unwrap(),
            CorsOrigins::Only(vec![
                "https://a.example".to_owned(),
                "http://b.example:8080".to_owned()
            ])
        );
        assert!(CorsOrigins::parse("a.example").is_err());
        assert!(CorsOrigins::parse("https://a b").is_err());
    }
}

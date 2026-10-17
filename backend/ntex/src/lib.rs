//! The todo list's JSON API on ntex, the same API as `backend/axum/`, described
//! by the same OpenAPI document (`contract/openapi.json`).
//!
//! Every ntex worker runs a compio runtime (io_uring on Linux, kqueue on
//! macOS, IOCP on Windows). There is no tokio reactor on them, so sqlx runs on
//! a tokio runtime of its own (`store/postgres.rs`).

pub mod api;
pub mod cli;
pub mod config;
pub mod domain;
pub mod logging;
pub mod store;

use std::{net::TcpListener, time::Instant};

use anyhow::Context;
use ntex::{
    Ctx, Middleware, Service, ServiceFactory, SharedCfg,
    error::Failure,
    http::{HttpServiceConfig, Method, Request, Response, header::CONTENT_TYPE},
    io::IoConfig,
    time::Seconds,
    url::Url,
    web::{self, App, WebAppConfig, WebError, WebRequest, WebResponse, types::JsonConfig},
};
use ntex_cors::{Cors, CorsMiddleware};

use crate::{
    api::{
        AppState,
        problem::{ApiError, ProblemDetails, unknown_endpoint},
    },
    config::Config,
};

/// The largest JSON body accepted, axum's default.
const JSON_LIMIT: usize = 2 * 1024 * 1024;

/// The whole application, shared by `run()` and the tests. It is built on
/// every worker, so it holds nothing that must be made only once.
pub fn app(
    cors: &CorsOrigins,
) -> impl ServiceFactory<
    AppState,
    Request,
    Res = Response,
    Error = WebError<AppState, ApiError>,
    InitError = Failure,
> + use<> {
    App::new()
        // Settings come before the first route.
        .with_config(WebAppConfig::new().set_state(JsonConfig::default().limit(JSON_LIMIT)))
        // The first middleware is the outermost.
        .middleware(LogRequest)
        .middleware(ProblemDetails)
        .middleware(cors.middleware())
        .configure(api::routes)
        .default_service(web::to(unknown_endpoint))
        .build()
}

pub async fn run(config: Config) -> anyhow::Result<()> {
    let cors = CorsOrigins::parse(&config.server.cors_origins)?;
    // Made once and shared by every worker.
    let state = AppState::new(
        store::connect(&config.database)
            .await
            .context("failed to open the database")?,
    );

    let address = format!("{}:{}", config.server.host, config.server.port);
    let listener =
        TcpListener::bind(&address).with_context(|| format!("failed to listen on {address}"))?;
    let local_address = listener.local_addr()?;

    // Connection and protocol settings, read by every accepted connection.
    let connection = SharedCfg::new("API")
        .add(IoConfig::new().set_shutdown_timeout(Seconds(2)))
        .add(HttpServiceConfig::new().set_keepalive_timeout(Seconds(30)));

    // SIGTERM (`docker stop`) lets open requests finish; Ctrl+C stops at once.
    let server = web::server_with_config(
        async move || Ok(state.clone()),
        async move |_: &AppState| app(&cors),
    )
    // Under Docker's 10 seconds between SIGTERM and SIGKILL.
    .graceful_shutdown_timeout(Seconds(8))
    .listen(listener, connection)?
    .run();
    tracing::info!(
        address = %local_address,
        env = ?config.env,
        cors_origins = config.server.cors_origins,
        "server started"
    );

    server.await?;
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
        // An origin is `scheme://host[:port]`. ntex-cors would only reject what
        // `Url` cannot parse, and then by panicking when a worker builds the app.
        let origins = origins
            .into_iter()
            .map(|origin| match Url::try_from(origin) {
                Ok(url) if url.scheme().is_some() && url.host().is_some() => Ok(origin.to_owned()),
                _ => anyhow::bail!("invalid CORS origin `{origin}`"),
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        Ok(Self::Only(origins))
    }

    /// The web client is served from another origin, so browsers ask for
    /// permission before calling the API. Unlike tower-http's, this middleware
    /// refuses a request from any other origin with a 400.
    fn middleware(&self) -> CorsMiddleware<AppState> {
        let cors = Cors::new()
            .allowed_methods([Method::GET, Method::POST, Method::PATCH, Method::DELETE])
            .allowed_header(CONTENT_TYPE);
        match self {
            Self::Any => cors.send_wildcard(),
            Self::Only(origins) => origins
                .iter()
                .fold(cors, |cors, origin| cors.allowed_origin(origin)),
        }
        .build()
    }
}

/// One log line per request. Probes log at debug so they do not drown the rest.
pub struct LogRequest;

pub struct LogRequestService<S> {
    service: S,
}

impl<S, St> Middleware<S, St> for LogRequest {
    type Service = LogRequestService<S>;

    fn create(&self, _: &St, service: S) -> Self::Service {
        LogRequestService { service }
    }
}

impl<S, St, R> Service<St, WebRequest<R>> for LogRequestService<S>
where
    S: Service<St, WebRequest<R>, Res = WebResponse>,
{
    type Res = WebResponse;
    type Error = S::Error;

    ntex::forward_ready!(St, service);
    ntex::forward_shutdown!(St, service);

    async fn call(
        &self,
        request: WebRequest<R>,
        ctx: Ctx<'_, Self, St>,
    ) -> Result<WebResponse, S::Error> {
        let method = request.method().clone();
        let path = request.path().to_owned();
        let started = Instant::now();

        // An `Err` is rendered further out; this app's middleware never fails.
        let response = ctx.call(&self.service, request).await?;

        let status = response.status().as_u16();
        let latency_ms = started.elapsed().as_micros() as f64 / 1000.0;
        if path.starts_with("/health/") {
            tracing::debug!(%method, path, status, latency_ms, "request");
        } else {
            tracing::info!(%method, path, status, latency_ms, "request");
        }
        Ok(response)
    }
}

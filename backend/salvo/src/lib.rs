//! The todo list's JSON API on Salvo, the same API as `backend/axum/`, described
//! by the same OpenAPI document (`contract/openapi.json`).

pub mod api;
pub mod cli;
pub mod config;
pub mod domain;
pub mod logging;
pub mod store;

use std::time::{Duration, Instant};

use anyhow::Context;
use salvo::{
    catch_panic::CatchPanic,
    catcher::Catcher,
    conn::Acceptor,
    cors::{Any, CallNext, Cors, CorsHandler},
    http::{HeaderValue, Method, StatusCode, header},
    prelude::*,
    server::ServerHandle,
};

use crate::{config::Config, store::Store};

/// How long open requests may take to finish once SIGTERM or Ctrl+C
/// arrives: under Docker's 10 seconds before it kills the process.
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(8);

/// The whole application. `cors_origins` lists the sites whose pages may call
/// the API from a browser, separated by commas; `*` allows any site.
///
/// Service hoops see every request, including those no route matches, so
/// logging and CORS are here rather than on the router. They run in order:
/// the first is the outermost.
pub fn app(store: Store, cors_origins: &str) -> anyhow::Result<Service> {
    let router = Router::new()
        .hoop(CatchPanic::new())
        .push(api::router(store));
    Ok(Service::new(router)
        .hoop(log_request)
        .hoop(cors(cors_origins)?)
        .hoop(answer_preflight)
        .hoop(no_empty_segments)
        // Every error left without a body becomes a problem.
        .catcher(Catcher::new(api::problem::catch)))
}

pub async fn run(config: Config) -> anyhow::Result<()> {
    let app = app(
        store::connect(&config.database)
            .await
            .context("failed to open the database")?,
        &config.server.cors_origins,
    )?;

    let address = format!("{}:{}", config.server.host, config.server.port);
    let acceptor = TcpListener::new(address.clone())
        .try_bind()
        .await
        .with_context(|| format!("failed to listen on {address}"))?;
    // Salvo writes its addresses as `socket://...`; the std form is the usual one.
    let local_address = acceptor
        .holdings()
        .first()
        .and_then(|holding| holding.local_addr.clone().into_std())
        .map_or(address, |address| address.to_string());
    tracing::info!(
        address = local_address,
        env = ?config.env,
        cors_origins = config.server.cors_origins,
        "server started"
    );

    let server = Server::new(acceptor);
    tokio::spawn(stop_on_signal(server.handle()));
    server.serve(app).await;
    tracing::info!("server stopped");
    Ok(())
}

/// The web client is served from another origin, so browsers ask for
/// permission before calling the API. A request from an origin that is not
/// listed is served without the CORS header, as tower-http does in the axum
/// backend.
///
/// `CallNext::After` adds the headers before the routes run, so every
/// response has them, errors too, or the client could not read the problem.
fn cors(origins: &str) -> anyhow::Result<CorsHandler> {
    let origins: Vec<&str> = origins
        .split(',')
        .map(|origin| origin.trim().trim_end_matches('/'))
        .filter(|origin| !origin.is_empty())
        .collect();

    let cors = Cors::new()
        .allow_methods([Method::GET, Method::POST, Method::PATCH, Method::DELETE])
        .allow_headers(vec![header::CONTENT_TYPE]);
    let cors = if origins.contains(&"*") {
        cors.allow_origin(Any)
    } else {
        // Checked here: salvo-cors would panic on an origin that is not a
        // header value.
        let origins = origins
            .into_iter()
            .map(|origin| {
                origin
                    .parse::<HeaderValue>()
                    .with_context(|| format!("invalid CORS origin `{origin}`"))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        cors.allow_origin(origins)
    };
    Ok(CorsHandler::new(cors, CallNext::After))
}

/// A preflight is answered by the CORS headers alone. Without this the routes
/// would run for it as well and answer 405, as none of them has OPTIONS.
#[handler]
async fn answer_preflight(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
    ctrl: &mut FlowCtrl,
) {
    let is_preflight = req.method() == Method::OPTIONS
        && req
            .headers()
            .contains_key(header::ACCESS_CONTROL_REQUEST_METHOD);
    if is_preflight {
        res.status_code(StatusCode::NO_CONTENT);
        ctrl.skip_rest();
    } else {
        ctrl.call_next(req, depot, res).await;
    }
}

/// Salvo's router skips empty segments, so `//api/v1/todos` would be served
/// like `/api/v1/todos`. An empty segment is answered with 404 instead, which
/// the catcher turns into the "No such endpoint." problem. A trailing slash is
/// fine: `/api/v1/todos/` is served like `/api/v1/todos`.
#[handler]
async fn no_empty_segments(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
    ctrl: &mut FlowCtrl,
) {
    if req.uri().path().contains("//") {
        res.status_code(StatusCode::NOT_FOUND);
        ctrl.skip_rest();
    } else {
        ctrl.call_next(req, depot, res).await;
    }
}

/// One log line per request. Probes log at debug so they do not drown the rest.
#[handler]
async fn log_request(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
    ctrl: &mut FlowCtrl,
) {
    let method = req.method().clone();
    let path = req.uri().path().to_owned();
    let started = Instant::now();

    ctrl.call_next(req, depot, res).await;

    // Unset means the default, 200.
    let status = res.status_code.unwrap_or(StatusCode::OK).as_u16();
    let latency_ms = started.elapsed().as_micros() as f64 / 1000.0;
    if path.starts_with("/health/") {
        tracing::debug!(%method, path, status, latency_ms, "request");
    } else {
        tracing::info!(%method, path, status, latency_ms, "request");
    }
}

/// On Ctrl+C, or SIGTERM on Unix, stops taking connections and gives open
/// requests `SHUTDOWN_TIMEOUT` to finish.
async fn stop_on_signal(handle: ServerHandle) {
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
    handle.stop_graceful(SHUTDOWN_TIMEOUT);
}

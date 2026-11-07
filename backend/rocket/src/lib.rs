//! The todo list's JSON API on Rocket, the same API as `backend/axum/`,
//! described by the same OpenAPI document (`contract/openapi.json`).
//!
//! Rocket brings most of what the other backends assemble themselves, and
//! this backend uses it: its configuration (a figment, like ours), managed
//! state, fairings, catchers, data limits, graceful shutdown and the local
//! test client. What Rocket does not have is kept as in the other backends:
//! the store behind `TodoStore` (Rocket's `rocket_db_pools` is on sqlx 0.7 and
//! has no Turso driver; a fairing opens the store instead, as it would its
//! pool), the health checks, CORS (a fairing written after tower-http's
//! behaviour) and logging with `tracing` (Rocket's own logger is turned off).

pub mod api;
pub mod cli;
pub mod config;
pub mod domain;
pub mod logging;
pub mod store;

use std::{
    io::Cursor,
    net::{IpAddr, SocketAddr},
    time::Instant,
};

use anyhow::{Context, anyhow};
use figment::{Figment, providers::Serialized};
use rocket::{
    Build, Request, Response, Rocket,
    config::{Ident, LogLevel, Shutdown},
    data::{Data, Limits, ToByteUnit},
    error::ErrorKind,
    fairing::{AdHoc, Fairing, Info, Kind},
    http::{Header, Method, Status},
};

use crate::{
    api::AppState,
    config::{Config, DatabaseConfig},
};

/// Where our settings are in Rocket's figment.
const APP_KEY: &str = "app";

/// The largest JSON body accepted, axum's default. Rocket's own is 1 MiB.
const JSON_LIMIT_MIB: u64 = 2;

/// How long open requests may take to finish once SIGTERM or Ctrl+C arrives,
/// in seconds: under Docker's 10 seconds before it kills the process. Rocket
/// gives connections `grace` to finish, then `mercy` to close.
const SHUTDOWN_GRACE: u32 = 6;
const SHUTDOWN_MERCY: u32 = 2;

/// The whole application, shared by `run()` and the tests.
///
/// Our settings are merged into Rocket's own figment under `app` (at the top
/// they would be read as Rocket's deprecated `env` and `log` keys), so the
/// fairings read them from there. Rocket's settings come from ours, and
/// nothing else: no `Rocket.toml`, no `ROCKET_*` variables.
pub fn rocket(config: &Config) -> anyhow::Result<Rocket<Build>> {
    let cors = CorsOrigins::parse(&config.server.cors_origins)?;
    let address: IpAddr = config.server.host.parse().with_context(|| {
        format!(
            "invalid server.host `{}`: expected an IP address",
            config.server.host
        )
    })?;

    let rocket_config = rocket::Config {
        address,
        port: config.server.port,
        limits: Limits::default().limit("json", JSON_LIMIT_MIB.mebibytes()),
        // No `Server` header, as in the other backends.
        ident: Ident::none(),
        // Requests are logged by `RequestLog`, in our format.
        log_level: LogLevel::Off,
        cli_colors: false,
        shutdown: Shutdown {
            grace: SHUTDOWN_GRACE,
            mercy: SHUTDOWN_MERCY,
            ..Shutdown::default()
        },
        ..rocket::Config::default()
    };
    let figment = Figment::from(rocket_config).merge(Serialized::defaults(config).key(APP_KEY));

    let rocket = rocket::custom(figment)
        .attach(open_store())
        .attach(cors)
        // Last, so it sees the response as the client gets it.
        .attach(RequestLog)
        .attach(AdHoc::on_liftoff("Started", |rocket| {
            Box::pin(async move {
                let address = SocketAddr::new(rocket.config().address, rocket.config().port);
                // Always there: `rocket()` put it in.
                let Ok(config) = rocket.figment().extract_inner::<Config>(APP_KEY) else {
                    return;
                };
                tracing::info!(
                    %address,
                    env = ?config.env,
                    cors_origins = config.server.cors_origins,
                    "server started"
                );
            })
        }));
    Ok(api::mount(rocket))
}

pub async fn run(config: Config) -> anyhow::Result<()> {
    let address = format!("{}:{}", config.server.host, config.server.port);
    // Rocket handles the signals: SIGTERM (`docker stop`) and Ctrl+C both let
    // open requests finish.
    rocket(&config)?
        .launch()
        .await
        .map_err(|error| launch_error(&error, &address))?;
    tracing::info!("server stopped");
    Ok(())
}

/// Rocket's error says little in `Display` (and panics when dropped unseen):
/// this reads what went wrong out of it.
fn launch_error(error: &rocket::Error, address: &str) -> anyhow::Error {
    match error.kind() {
        ErrorKind::Bind(error) => anyhow!("failed to listen on {address}: {error}"),
        ErrorKind::Io(error) => anyhow!("failed to serve: {error}"),
        ErrorKind::Config(error) => anyhow!("invalid configuration: {error}"),
        ErrorKind::FailedFairings(fairings) => {
            let names: Vec<&str> = fairings.iter().map(|info| info.name).collect();
            anyhow!("failed to start: {} (see the log)", names.join(", "))
        }
        _ => anyhow!("{error}"),
    }
}

/// Opens the store named by `app.database`, with the health checks of the
/// same database, and makes them managed state before the server starts; the
/// launch fails when it cannot. Rocket's `rocket_db_pools`
/// works the same way for the pools it knows.
fn open_store() -> AdHoc {
    AdHoc::try_on_ignite("Open the database", |rocket| async move {
        let database = match rocket
            .figment()
            .extract_inner::<DatabaseConfig>("app.database")
        {
            Ok(database) => database,
            Err(error) => {
                tracing::error!(error = %error, "invalid database configuration");
                return Err(rocket);
            }
        };
        let health = match api::health::checks(&database) {
            Ok(health) => health,
            Err(error) => {
                tracing::error!(error = %error, "invalid database health check");
                return Err(rocket);
            }
        };
        match store::connect(&database).await {
            Ok(store) => Ok(rocket.manage(AppState::new(store, health))),
            Err(error) => {
                tracing::error!(error = %error, "failed to open the database");
                Err(rocket)
            }
        }
    })
}

/// The sites whose pages may call the API from a browser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CorsOrigins {
    /// `*`: any site.
    Any,
    Only(Vec<String>),
}

impl CorsOrigins {
    const ALLOW_METHODS: &str = "GET,POST,PATCH,DELETE";
    const ALLOW_HEADERS: &str = "content-type";
    const VARY: &str = "origin, access-control-request-method, access-control-request-headers";

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
        // An origin is `scheme://host[:port]`.
        let origins = origins
            .into_iter()
            .map(|origin| {
                let valid = origin.split_once("://").is_some_and(|(scheme, host)| {
                    !scheme.is_empty()
                        && scheme
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
                        && !host.is_empty()
                        && !host.contains(['/', ' ', '?', '#'])
                });
                if valid {
                    Ok(origin.to_owned())
                } else {
                    Err(anyhow!("invalid CORS origin `{origin}`"))
                }
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        Ok(Self::Only(origins))
    }

    /// What `Access-Control-Allow-Origin` says to a request from `origin`.
    fn allow(&self, origin: &str) -> Option<String> {
        match self {
            Self::Any => Some("*".to_owned()),
            Self::Only(origins) => origins
                .iter()
                .any(|allowed| allowed == origin)
                .then(|| origin.to_owned()),
        }
    }
}

/// The web client is served from another origin, so browsers ask for
/// permission before calling the API. Rocket has no CORS of its own, so this
/// fairing does what tower-http does in the axum backend: a preflight is
/// answered with 200 whatever the route, and a request from an origin that is
/// not listed is served without the CORS headers.
#[rocket::async_trait]
impl Fairing for CorsOrigins {
    fn info(&self) -> Info {
        Info {
            name: "CORS",
            kind: Kind::Response,
        }
    }

    async fn on_response<'r>(&self, request: &'r Request<'_>, response: &mut Response<'r>) {
        response.set_header(Header::new("Vary", Self::VARY));
        let Some(origin) = request.headers().get_one("Origin") else {
            return;
        };
        let preflight = request.method() == Method::Options
            && request.headers().contains("Access-Control-Request-Method");

        if preflight {
            // Whatever routing made of it (a 405, a 404): an empty 200.
            response.set_status(Status::Ok);
            response.remove_header("Content-Type");
            response.remove_header("Allow");
            response.set_sized_body(0, Cursor::new(Vec::new()));
            response.set_header(Header::new(
                "Access-Control-Allow-Methods",
                Self::ALLOW_METHODS,
            ));
            response.set_header(Header::new(
                "Access-Control-Allow-Headers",
                Self::ALLOW_HEADERS,
            ));
        }
        if let Some(allowed) = self.allow(origin) {
            response.set_header(Header::new("Access-Control-Allow-Origin", allowed));
        }
    }
}

/// One log line per request. Probes log at debug so they do not drown the rest.
struct RequestLog;

/// When the request arrived, in the request-local cache.
struct Started(Instant);

#[rocket::async_trait]
impl Fairing for RequestLog {
    fn info(&self) -> Info {
        Info {
            name: "Request log",
            kind: Kind::Request | Kind::Response,
        }
    }

    async fn on_request(&self, request: &mut Request<'_>, _: &mut Data<'_>) {
        request.local_cache(|| Started(Instant::now()));
    }

    async fn on_response<'r>(&self, request: &'r Request<'_>, response: &mut Response<'r>) {
        let latency = request.local_cache(|| Started(Instant::now())).0.elapsed();
        let method = request.method();
        let path = request.uri().path().as_str();
        let status = response.status().code;
        let latency_ms = latency.as_micros() as f64 / 1000.0;
        if path.starts_with("/health/") {
            tracing::debug!(%method, path, status, latency_ms, "request");
        } else {
            tracing::info!(%method, path, status, latency_ms, "request");
        }
    }
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

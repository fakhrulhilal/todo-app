//! Health probes on the `health-checks` crate (ASP.NET Core's health checks
//! for Rust), used as its guide shows: register checks with tags, and give
//! each probe the options that pick its checks and its writer.
//!
//! - `self` always answers Healthy: the process serves requests.
//! - `database` asks the configured database, with the crate's own
//!   `PostgresCheck` or `SqliteCheck`. It is tagged `dependency`.
//!
//! `/health/live` runs `self` only, `/health/ready` every check tagged
//! `dependency`. Both answer the status as plain text, or the crate's JSON
//! with `?format=json`.

use std::{io::Cursor, sync::Arc, time::Duration};

use health_checks::{
    BoxError, HealthCheckOptions, HealthCheckRegistration, HealthCheckResult as CheckResult,
    HealthCheckService, HealthChecks, HealthResponse,
    checks::{PostgresCheck, SqliteCheck},
};
use rocket::{
    Request, Response, Route, State,
    http::{Method, Status},
    response::{self, Responder},
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use super::{
    AppState,
    extract::query,
    problem::{ApiError, PROBLEM_JSON, Problem, method_not_allowed},
};
use crate::config::DatabaseConfig;

pub const TAG: &str = "health";

const CHECK_TIMEOUT: Duration = Duration::from_secs(2);
const SELF: &str = "self";
const DEPENDENCY: &str = "dependency";

/// The checks, by name. Add a dependency with the `dependency` tag and the
/// readiness probe runs it.
pub fn checks(database: &DatabaseConfig) -> Result<HealthCheckService, BoxError> {
    Ok(HealthChecks::builder()
        .default_timeout(CHECK_TIMEOUT)
        .add_fn(SELF, CheckResult::healthy)
        .add(
            HealthCheckRegistration::from_arc("database", database_check(&database.url)?)
                .tags([DEPENDENCY]),
        )
        .build())
}

/// The crate's check for the database that `url` names, as `store::connect`
/// reads it.
fn database_check(url: &str) -> Result<Arc<dyn health_checks::HealthCheck>, BoxError> {
    let url = url.trim();
    if url.starts_with("postgres://") || url.starts_with("postgresql://") {
        return Ok(Arc::new(PostgresCheck::new(url)?));
    }
    match url.strip_prefix("sqlite:") {
        Some(path) => Ok(Arc::new(SqliteCheck::new(
            path.strip_prefix("//").unwrap_or(path),
        )?)),
        None => Err("unsupported database URL".into()),
    }
}

/// How a probe answers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum HealthFormat {
    /// The status alone, as plain text.
    #[default]
    Text,
    /// The status and the result of each check.
    Json,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct HealthQuery {
    /// `json` for the result of each check. The status alone otherwise.
    #[serde(default)]
    pub format: HealthFormat,
}

/// Runs the checks that `options` picks, in the asked format.
async fn probe(
    health: &HealthCheckService,
    options: HealthCheckOptions,
    format: Option<String>,
) -> Result<ProbeResponse, ApiError> {
    let options = match query(format.as_deref())? {
        HealthFormat::Text => options,
        HealthFormat::Json => options.writer(health_checks::ResponseWriter::Json),
    };
    let response = options.execute(health).await;
    if response.status != health_checks::HealthStatus::Healthy {
        tracing::warn!(status = %response.status, "health check failed");
    }
    Ok(ProbeResponse(response))
}

/// The crate's answer (status code, no-cache headers, body) as a Rocket response.
pub struct ProbeResponse(HealthResponse);

impl<'r> Responder<'r, 'static> for ProbeResponse {
    fn respond_to(self, _: &'r Request<'_>) -> response::Result<'static> {
        let HealthResponse {
            status_code,
            headers,
            body,
            ..
        } = self.0;
        let mut response = Response::build();
        response.status(Status::new(status_code));
        for (name, value) in headers {
            response.raw_header(name, value);
        }
        response.sized_body(body.len(), Cursor::new(body)).ok()
    }
}

/// `/health/live` and `/health/ready`. GET answers HEAD too.
pub fn routes() -> Vec<Route> {
    let mut routes = rocket::routes![live, ready];
    routes.extend(method_not_allowed("/health/live", &[Method::Get]));
    routes.extend(method_not_allowed("/health/ready", &[Method::Get]));
    routes
}

/// Liveness probe
///
/// Healthy for as long as the server answers requests. Runs no checks.
#[utoipa::path(
    get,
    path = "/health/live",
    tag = TAG,
    params(HealthQuery),
    responses(
        (status = 200, description = "The process is running", content(
            (String = "text/plain", example = "Healthy"),
            (HealthReport = "application/json"),
        )),
        (status = 400, description = "Invalid query", body = Problem, content_type = PROBLEM_JSON),
    )
)]
#[rocket::get("/health/live?<format>")]
async fn live(state: &State<AppState>, format: Option<String>) -> Result<ProbeResponse, ApiError> {
    let options = HealthCheckOptions::new().predicate(|check| check.name == SELF);
    probe(&state.health, options, format).await
}

/// Readiness probe
///
/// Runs every registered check and reports the worst status: `Healthy`,
/// `Degraded` (still 200) or `Unhealthy` (503).
#[utoipa::path(
    get,
    path = "/health/ready",
    tag = TAG,
    params(HealthQuery),
    responses(
        (status = 200, description = "Ready to serve requests: Healthy or Degraded", content(
            (String = "text/plain", example = "Healthy"),
            (HealthReport = "application/json"),
        )),
        (status = 400, description = "Invalid query", body = Problem, content_type = PROBLEM_JSON),
        (status = 503, description = "A check is Unhealthy", content(
            (String = "text/plain", example = "Unhealthy"),
            (HealthReport = "application/json"),
        )),
    )
)]
#[rocket::get("/health/ready?<format>")]
async fn ready(state: &State<AppState>, format: Option<String>) -> Result<ProbeResponse, ApiError> {
    let options = HealthCheckOptions::new().tags([DEPENDENCY]);
    probe(&state.health, options, format).await
}

// The probe JSON as `contract/openapi.json` describes it. Only for the
// document, which must equal the contract: the body is the crate's JSON.

/// Ordered from worst to best, so the status of a report is the minimum of
/// its checks.
#[derive(ToSchema)]
#[allow(dead_code)]
enum HealthStatus {
    /// Not working. The probe answers 503.
    Unhealthy,
    /// Working, but not as it should (slow, a fallback in use). Still 200.
    Degraded,
    Healthy,
}

/// What one check found.
#[derive(ToSchema)]
#[allow(dead_code)]
struct HealthCheckResult {
    status: HealthStatus,
    /// Safe to show to whoever can call the probe: no connection strings,
    /// host names or raw error messages.
    #[schema(example = "The database answered.")]
    description: Option<String>,
    /// Extra values worth showing, such as a queue length.
    #[schema(value_type = Object)]
    data: std::collections::BTreeMap<String, serde_json::Value>,
}

/// The outcome of a probe.
#[derive(ToSchema)]
#[allow(dead_code)]
struct HealthReport {
    /// The worst status among the checks; `Healthy` when there are none.
    status: HealthStatus,
    /// One entry per check, by name.
    results: std::collections::BTreeMap<String, HealthCheckResult>,
}

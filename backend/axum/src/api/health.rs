//! Health probes, built on the `health-checks` crate (ASP.NET Core's health
//! checks for Rust).
//!
//! A probe runs the registered checks and answers with the worst status as
//! plain text: `Healthy`, `Degraded` or `Unhealthy`. `?format=json` adds the
//! result of each check. Liveness runs no checks, so it only says the process
//! serves HTTP; readiness runs the checks of what the service depends on.
//!
//! The crate runs the checks (concurrently, with a timeout, a panic or error
//! counting as the registration's failure status) and sets the status code and
//! no-cache headers. The JSON body keeps the shape of the shared contract
//! (`status` + `results`), not the crate's own `UIResponseWriter` shape.

use std::{collections::BTreeMap, time::Duration};

use axum::{
    extract::State,
    response::{IntoResponse, Response},
};
use health_checks::{
    BoxError, HealthCheck, HealthCheckContext, HealthCheckOptions, HealthCheckService,
    HealthChecks, ResponseWriter, async_trait,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use super::{
    AppState,
    extract::Query,
    problem::{PROBLEM_JSON, Problem},
};
use crate::store::Store;

pub const TAG: &str = "health";

const CHECK_TIMEOUT: Duration = Duration::from_secs(2);

/// The checks the readiness probe runs. Add one per dependency.
pub fn checks(store: Store) -> HealthCheckService {
    HealthChecks::builder()
        .default_timeout(CHECK_TIMEOUT)
        .add_check("database", DatabaseCheck(store))
        .build()
}

/// Checks that the database answers a trivial query.
pub struct DatabaseCheck(pub Store);

#[async_trait]
impl HealthCheck for DatabaseCheck {
    async fn check(
        &self,
        _ctx: &HealthCheckContext,
    ) -> Result<health_checks::HealthCheckResult, BoxError> {
        match self.0.ping().await {
            Ok(()) => Ok(health_checks::HealthCheckResult::healthy()
                .with_description("The database answered.")),
            Err(error) => {
                // The cause stays in the log; the probe may be public.
                tracing::warn!(error = %error, "database health check failed");
                Err("The database did not answer.".into())
            }
        }
    }
}

// The JSON body of a probe, as the contract (`contract/openapi.json`) has it.

/// Ordered from worst to best, so the status of a report is the minimum of
/// its checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
pub enum HealthStatus {
    /// Not working. The probe answers 503.
    Unhealthy,
    /// Working, but not as it should (slow, a fallback in use). Still 200.
    Degraded,
    Healthy,
}

impl From<health_checks::HealthStatus> for HealthStatus {
    fn from(status: health_checks::HealthStatus) -> Self {
        match status {
            health_checks::HealthStatus::Unhealthy => Self::Unhealthy,
            health_checks::HealthStatus::Degraded => Self::Degraded,
            health_checks::HealthStatus::Healthy => Self::Healthy,
        }
    }
}

/// What one check found.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct HealthCheckResult {
    pub status: HealthStatus,
    /// Safe to show to whoever can call the probe: no connection strings,
    /// host names or raw error messages.
    #[schema(example = "The database answered.")]
    pub description: Option<String>,
    /// Extra values worth showing, such as a queue length.
    #[schema(value_type = Object)]
    pub data: BTreeMap<String, serde_json::Value>,
}

/// The outcome of a probe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct HealthReport {
    /// The worst status among the checks; `Healthy` when there are none.
    pub status: HealthStatus,
    /// One entry per check, by name.
    pub results: BTreeMap<String, HealthCheckResult>,
}

impl From<&health_checks::HealthReport> for HealthReport {
    fn from(report: &health_checks::HealthReport) -> Self {
        let results = report
            .entries
            .iter()
            .map(|(name, entry)| {
                let result = HealthCheckResult {
                    status: entry.status.into(),
                    description: entry.description.clone(),
                    // `exception` (the error chain) is left out: the probe may
                    // be public.
                    data: entry
                        .data
                        .iter()
                        .map(|(key, value)| (key.clone(), value.clone()))
                        .collect(),
                };
                (name.clone(), result)
            })
            .collect();
        Self {
            status: report.status.into(),
            results,
        }
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

impl HealthFormat {
    fn writer(self) -> ResponseWriter {
        match self {
            Self::Text => ResponseWriter::Plain,
            Self::Json => ResponseWriter::custom(|report| {
                let body = serde_json::to_string(&HealthReport::from(report))
                    .expect("a health report always serializes");
                ("application/json".to_owned(), body)
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct HealthQuery {
    /// `json` for the result of each check. The status alone otherwise.
    #[serde(default)]
    pub format: HealthFormat,
}

/// Which checks a probe runs.
#[derive(Debug, Clone, Copy)]
enum Probe {
    /// None.
    Live,
    /// All of them.
    Ready,
}

/// Runs the checks of `kind` and answers in `format`. Status code (200, or
/// 503 when `Unhealthy`) and no-cache headers come from the crate.
async fn probe(health: &HealthCheckService, kind: Probe, format: HealthFormat) -> Response {
    let report = match kind {
        Probe::Live => health.check_health_filtered(|_| false).await,
        Probe::Ready => health.check_health().await,
    };
    log_failures(&report);
    HealthCheckOptions::new()
        .writer(format.writer())
        .render(&report)
        .into_response()
}

/// Logs every check that did not pass. The probe body has the safe
/// description only; the log also has the error chain.
fn log_failures(report: &health_checks::HealthReport) {
    for (name, entry) in &report.entries {
        if entry.status != health_checks::HealthStatus::Healthy {
            tracing::warn!(
                check = name,
                status = %entry.status,
                description = entry.description,
                exception = entry.exception,
                "health check failed"
            );
        }
    }
}

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(live))
        .routes(routes!(ready))
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
async fn live(State(state): State<AppState>, Query(query): Query<HealthQuery>) -> Response {
    probe(&state.health, Probe::Live, query.format).await
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
async fn ready(State(state): State<AppState>, Query(query): Query<HealthQuery>) -> Response {
    probe(&state.health, Probe::Ready, query.format).await
}

#[cfg(test)]
mod tests {
    use axum::http::{
        StatusCode,
        header::{CACHE_CONTROL, CONTENT_TYPE},
    };
    use serde_json::json;

    use super::*;

    type CrateResult = health_checks::HealthCheckResult;

    struct Hangs;

    #[async_trait]
    impl HealthCheck for Hangs {
        async fn check(&self, _ctx: &HealthCheckContext) -> Result<CrateResult, BoxError> {
            std::future::pending().await
        }
    }

    struct Panics;

    #[async_trait]
    impl HealthCheck for Panics {
        async fn check(&self, _ctx: &HealthCheckContext) -> Result<CrateResult, BoxError> {
            panic!("boom")
        }
    }

    async fn report(health: HealthCheckService) -> HealthReport {
        HealthReport::from(&health.check_health().await)
    }

    #[tokio::test(start_paused = true)]
    async fn a_check_that_hangs_or_panics_has_failed() {
        let health = HealthChecks::builder()
            .default_timeout(CHECK_TIMEOUT)
            .add_check("slow", Hangs)
            .add_check("broken", Panics)
            .build();
        let report = report(health).await;
        assert_eq!(report.status, HealthStatus::Unhealthy);
        assert_eq!(report.results["slow"].status, HealthStatus::Unhealthy);
        assert_eq!(report.results["broken"].status, HealthStatus::Unhealthy);
    }

    #[tokio::test]
    async fn the_answer_is_the_status_alone_unless_json_is_asked_for() {
        let health = HealthChecks::builder()
            .add_fn("database", || {
                CrateResult::healthy()
                    .with_description("fine")
                    .with_data("latency_ms", 3)
            })
            .add_async_fn("queue", || async { Err("down".into()) })
            .build();

        let text = probe(&health, Probe::Ready, HealthFormat::Text).await;
        assert_eq!(text.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(text.headers()[CACHE_CONTROL], "no-store, no-cache");
        assert!(
            text.headers()[CONTENT_TYPE]
                .to_str()
                .unwrap()
                .starts_with("text/plain")
        );
        let body = axum::body::to_bytes(text.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body, "Unhealthy");

        let detail = probe(&health, Probe::Ready, HealthFormat::Json).await;
        assert_eq!(detail.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(detail.headers()[CONTENT_TYPE], "application/json");
        let body = axum::body::to_bytes(detail.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
            json!({
                "status": "Unhealthy",
                "results": {
                    "database": {
                        "status": "Healthy",
                        "description": "fine",
                        "data": { "latency_ms": 3 }
                    },
                    "queue": { "status": "Unhealthy", "description": "down", "data": {} }
                }
            })
        );
    }

    #[tokio::test]
    async fn a_degraded_service_still_answers_200() {
        let health = HealthChecks::builder()
            .add_fn("database", CrateResult::healthy)
            .add(
                health_checks::HealthCheckRegistration::new(
                    "cache",
                    health_checks::AsyncFnCheck(|| async { Err("down".into()) }),
                )
                .failure_status(health_checks::HealthStatus::Degraded),
            )
            .build();
        let response = probe(&health, Probe::Ready, HealthFormat::Text).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body, "Degraded");
    }
}

//! Health probes, modelled on ASP.NET Core's health checks.
//!
//! A probe runs the registered checks and answers with the worst status as
//! plain text: `Healthy`, `Degraded` or `Unhealthy`. `?format=json` adds the
//! result of each check. Liveness runs no checks, so it only says the process
//! serves HTTP; readiness runs the checks of what the service depends on.

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use async_trait::async_trait;
use axum::{
    extract::State,
    http::{
        StatusCode,
        header::{CACHE_CONTROL, EXPIRES, PRAGMA},
    },
    response::{IntoResponse, Response},
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

impl HealthStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unhealthy => "Unhealthy",
            Self::Degraded => "Degraded",
            Self::Healthy => "Healthy",
        }
    }

    fn status_code(self) -> StatusCode {
        match self {
            Self::Healthy | Self::Degraded => StatusCode::OK,
            Self::Unhealthy => StatusCode::SERVICE_UNAVAILABLE,
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

impl HealthCheckResult {
    pub fn new(status: HealthStatus, description: impl Into<String>) -> Self {
        Self {
            status,
            description: Some(description.into()),
            data: BTreeMap::new(),
        }
    }

    pub fn healthy(description: impl Into<String>) -> Self {
        Self::new(HealthStatus::Healthy, description)
    }

    pub fn degraded(description: impl Into<String>) -> Self {
        Self::new(HealthStatus::Degraded, description)
    }

    pub fn unhealthy(description: impl Into<String>) -> Self {
        Self::new(HealthStatus::Unhealthy, description)
    }

    pub fn with_data(
        mut self,
        key: impl Into<String>,
        value: impl Into<serde_json::Value>,
    ) -> Self {
        self.data.insert(key.into(), value.into());
        self
    }
}

/// One thing the service depends on.
#[async_trait]
pub trait HealthCheck: Send + Sync + 'static {
    /// `Err` carries a description of the failure and is reported with the
    /// failure status the check was registered with.
    async fn check(&self) -> Result<HealthCheckResult, String>;
}

struct Registration {
    name: String,
    failure_status: HealthStatus,
    check: Arc<dyn HealthCheck>,
}

/// The registered checks.
#[derive(Default)]
pub struct HealthChecks {
    registrations: Vec<Registration>,
}

impl HealthChecks {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a check whose failure makes the service `Unhealthy`.
    pub fn add(self, name: impl Into<String>, check: impl HealthCheck) -> Self {
        self.add_with_failure_status(name, HealthStatus::Unhealthy, check)
    }

    /// Registers a check and what its failure means. Use `Degraded` for a
    /// dependency the service can work without.
    pub fn add_with_failure_status(
        mut self,
        name: impl Into<String>,
        failure_status: HealthStatus,
        check: impl HealthCheck,
    ) -> Self {
        self.registrations.push(Registration {
            name: name.into(),
            failure_status,
            check: Arc::new(check),
        });
        self
    }

    /// Runs every check at the same time. One that fails, panics or takes
    /// longer than the timeout gets its failure status.
    pub async fn run(&self) -> HealthReport {
        let running: Vec<_> = self
            .registrations
            .iter()
            .map(|registration| {
                let check = registration.check.clone();
                tokio::spawn(
                    async move { tokio::time::timeout(CHECK_TIMEOUT, check.check()).await },
                )
            })
            .collect();

        let mut results = BTreeMap::new();
        for (registration, running) in self.registrations.iter().zip(running) {
            let failed = |description: String| {
                tracing::warn!(
                    check = registration.name,
                    description,
                    "health check failed"
                );
                HealthCheckResult::new(registration.failure_status, description)
            };
            let result = match running.await {
                Ok(Ok(Ok(result))) => result,
                Ok(Ok(Err(description))) => failed(description),
                Ok(Err(_)) => failed("The check timed out.".to_owned()),
                Err(_) => failed("The check failed unexpectedly.".to_owned()),
            };
            results.insert(registration.name.clone(), result);
        }
        HealthReport::new(results)
    }
}

/// The outcome of a probe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct HealthReport {
    /// The worst status among the checks; `Healthy` when there are none.
    pub status: HealthStatus,
    /// One entry per check, by name.
    pub results: BTreeMap<String, HealthCheckResult>,
}

impl HealthReport {
    pub fn new(results: BTreeMap<String, HealthCheckResult>) -> Self {
        let status = results
            .values()
            .map(|result| result.status)
            .min()
            .unwrap_or(HealthStatus::Healthy);
        Self { status, results }
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

fn respond(report: HealthReport, format: HealthFormat) -> Response {
    let status = report.status.status_code();
    let body = match format {
        HealthFormat::Text => report.status.as_str().into_response(),
        HealthFormat::Json => axum::Json(report).into_response(),
    };
    // A probe must be answered by this process, now: never from a cache.
    let no_cache = [
        (CACHE_CONTROL, "no-store, no-cache"),
        (PRAGMA, "no-cache"),
        (EXPIRES, "Thu, 01 Jan 1970 00:00:00 GMT"),
    ];
    (status, no_cache, body).into_response()
}

/// Checks that the database answers a trivial query.
pub struct DatabaseCheck(pub Store);

#[async_trait]
impl HealthCheck for DatabaseCheck {
    async fn check(&self) -> Result<HealthCheckResult, String> {
        match self.0.ping().await {
            Ok(()) => Ok(HealthCheckResult::healthy("The database answered.")),
            Err(error) => {
                // The cause stays in the log; the probe may be public.
                tracing::warn!(error = %error, "database health check failed");
                Err("The database did not answer.".to_owned())
            }
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
async fn live(Query(query): Query<HealthQuery>) -> Response {
    respond(HealthReport::new(BTreeMap::new()), query.format)
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
    respond(state.health.run().await, query.format)
}

#[cfg(test)]
mod tests {
    use axum::http::header::CONTENT_TYPE;
    use serde_json::json;

    use super::*;

    struct Fixed(Result<HealthCheckResult, String>);

    #[async_trait]
    impl HealthCheck for Fixed {
        async fn check(&self) -> Result<HealthCheckResult, String> {
            self.0.clone()
        }
    }

    struct Hangs;

    #[async_trait]
    impl HealthCheck for Hangs {
        async fn check(&self) -> Result<HealthCheckResult, String> {
            std::future::pending().await
        }
    }

    struct Panics;

    #[async_trait]
    impl HealthCheck for Panics {
        async fn check(&self) -> Result<HealthCheckResult, String> {
            panic!("boom")
        }
    }

    fn healthy() -> Fixed {
        Fixed(Ok(HealthCheckResult::healthy("fine")))
    }

    #[tokio::test]
    async fn no_checks_is_healthy() {
        let report = HealthChecks::new().run().await;
        assert_eq!(report.status, HealthStatus::Healthy);
        assert!(report.results.is_empty());
    }

    #[tokio::test]
    async fn the_report_has_the_worst_status_of_its_checks() {
        let report = HealthChecks::new()
            .add("database", healthy())
            .add("cache", Fixed(Ok(HealthCheckResult::degraded("slow"))))
            .run()
            .await;
        assert_eq!(report.status, HealthStatus::Degraded);
        assert_eq!(report.results["database"].status, HealthStatus::Healthy);

        let report = HealthChecks::new()
            .add("database", Fixed(Err("down".to_owned())))
            .add("cache", Fixed(Ok(HealthCheckResult::degraded("slow"))))
            .add("search", healthy())
            .run()
            .await;
        assert_eq!(report.status, HealthStatus::Unhealthy);
        assert_eq!(
            report.results["database"],
            HealthCheckResult::unhealthy("down")
        );
    }

    #[tokio::test]
    async fn a_failure_gets_the_status_the_check_was_registered_with() {
        let report = HealthChecks::new()
            .add("database", healthy())
            .add_with_failure_status(
                "cache",
                HealthStatus::Degraded,
                Fixed(Err("down".to_owned())),
            )
            .run()
            .await;
        assert_eq!(report.status, HealthStatus::Degraded);
        assert_eq!(report.status.status_code(), StatusCode::OK);
    }

    #[tokio::test(start_paused = true)]
    async fn a_check_that_hangs_or_panics_has_failed() {
        let report = HealthChecks::new()
            .add("slow", Hangs)
            .add("broken", Panics)
            .run()
            .await;
        assert_eq!(report.status, HealthStatus::Unhealthy);
        assert_eq!(
            report.results["slow"].description.as_deref(),
            Some("The check timed out.")
        );
        assert_eq!(report.results["broken"].status, HealthStatus::Unhealthy);
    }

    #[tokio::test]
    async fn the_answer_is_the_status_alone_unless_json_is_asked_for() {
        let report = HealthChecks::new()
            .add(
                "database",
                Fixed(Ok(
                    HealthCheckResult::healthy("fine").with_data("latency_ms", 3)
                )),
            )
            .add("queue", Fixed(Err("down".to_owned())))
            .run()
            .await;

        let text = respond(report.clone(), HealthFormat::Text);
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

        let detail = respond(report, HealthFormat::Json);
        assert_eq!(detail.status(), StatusCode::SERVICE_UNAVAILABLE);
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
}

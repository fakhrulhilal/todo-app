//! Health probes on the `health-checks` crate (ASP.NET Core-style). They
//! answer as the axum backend's do.
//!
//! A probe runs the registered checks and answers with the worst status as
//! plain text: `Healthy`, `Degraded` or `Unhealthy` (503). `?format=json`
//! adds the result of each check. Liveness runs no checks, so it only says
//! the process serves HTTP; readiness runs the checks of what the service
//! depends on.

use std::time::Duration;

use axum::{
    Router,
    extract::{Query, State},
    routing::get,
};
use health_checks::{
    BoxError, HealthCheck, HealthCheckContext, HealthCheckOptions, HealthCheckRegistration,
    HealthCheckResult, HealthCheckService, HealthChecks, HealthReport, HealthResponse,
    ResponseWriter, async_trait,
};
use serde::Deserialize;

use crate::store::Store;

const CHECK_TIMEOUT: Duration = Duration::from_secs(2);

/// How a probe answers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthFormat {
    /// The status alone, as plain text.
    #[default]
    Text,
    /// The status and the result of each check.
    Json,
}

impl HealthFormat {
    fn options(self) -> HealthCheckOptions {
        match self {
            Self::Text => HealthCheckOptions::new(),
            Self::Json => HealthCheckOptions::new().writer(ResponseWriter::custom(|report| {
                ("application/json".to_owned(), contract_json(report))
            })),
        }
    }
}

/// The JSON body in the shape every backend uses (`status` + `results`), not
/// the crate's own. `exception` (the error chain) is left out: the probe may be
/// public.
fn contract_json(report: &HealthReport) -> String {
    let results: serde_json::Map<String, serde_json::Value> = report
        .entries
        .iter()
        .map(|(name, entry)| {
            let data: serde_json::Map<String, serde_json::Value> = entry
                .data
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect();
            let result = serde_json::json!({
                "status": entry.status,
                "description": entry.description,
                "data": data,
            });
            (name.clone(), result)
        })
        .collect();
    serde_json::json!({ "status": report.status, "results": results }).to_string()
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub struct HealthQuery {
    /// `json` for the result of each check. The status alone otherwise.
    #[serde(default)]
    pub format: HealthFormat,
}

/// Checks that the database answers a trivial query.
pub struct DatabaseCheck(pub Store);

#[async_trait]
impl HealthCheck for DatabaseCheck {
    async fn check(&self, ctx: &HealthCheckContext) -> Result<HealthCheckResult, BoxError> {
        Ok(match self.0.ping().await {
            Ok(()) => HealthCheckResult::healthy().with_description("The database answered."),
            Err(error) => {
                // The cause stays in the log; the probe may be public, so it
                // is not returned as an error (which would expose it).
                tracing::warn!(error = %error, "database health check failed");
                HealthCheckResult::new(ctx.failure_status())
                    .with_description("The database did not answer.")
            }
        })
    }
}

/// The checks of what the app depends on.
pub fn checks(store: Store) -> HealthCheckService {
    HealthChecks::builder()
        .default_timeout(CHECK_TIMEOUT)
        .add(HealthCheckRegistration::new("database", DatabaseCheck(store)))
        .build()
}

/// `/health/live` and `/health/ready`.
pub fn router(store: Store) -> Router {
    Router::new()
        .route("/health/live", get(live))
        .route("/health/ready", get(ready))
        .with_state(checks(store))
}

/// Healthy for as long as the server answers requests. Runs no checks.
async fn live(
    State(checks): State<HealthCheckService>,
    Query(query): Query<HealthQuery>,
) -> HealthResponse {
    query
        .format
        .options()
        .predicate(|_| false)
        .execute(&checks)
        .await
}

/// Runs every registered check and reports the worst status: `Healthy`,
/// `Degraded` (still 200) or `Unhealthy` (503).
async fn ready(
    State(checks): State<HealthCheckService>,
    Query(query): Query<HealthQuery>,
) -> HealthResponse {
    query.format.options().execute(&checks).await
}

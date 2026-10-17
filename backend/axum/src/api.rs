//! The JSON API and its OpenAPI document. No documentation UI is served: any
//! OpenAPI viewer (Scalar in compose.yaml) reads the document instead.

mod extract;
pub mod health;
pub mod problem;
pub mod todos;

use std::sync::Arc;

use axum::{Json, Router, middleware, response::Redirect, routing::get};
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;

use self::{
    health::{DatabaseCheck, HealthChecks},
    problem::ApiError,
};
use crate::store::Store;

pub const OPENAPI_PATH: &str = "/api/openapi.json";

#[derive(Clone)]
pub struct AppState {
    pub store: Store,
    pub health: Arc<HealthChecks>,
}

#[derive(OpenApi)]
#[openapi(
    info(title = "Todo API", description = "A todo list. Errors are RFC 9457 problem details."),
    tags(
        (name = todos::TAG, description = "Manage todos"),
        (name = health::TAG, description = "Liveness and readiness probes"),
    )
)]
struct ApiDoc;

/// Routes under `/api` and `/health`, and the OpenAPI document.
pub fn router(store: Store) -> Router {
    let (router, openapi) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .nest("/api/v1", todos::router())
        .merge(health::router())
        .with_state(AppState {
            // What the readiness probe checks. Add one per dependency.
            health: Arc::new(HealthChecks::new().add("database", DatabaseCheck(store.clone()))),
            store,
        })
        .split_for_parts();

    router
        .route(
            OPENAPI_PATH,
            get(move || {
                let document = openapi.clone();
                async move { Json(document) }
            }),
        )
        .route("/", get(|| async { Redirect::temporary(OPENAPI_PATH) }))
        .fallback(unknown_endpoint)
        .layer(middleware::from_fn(problem::problem_details))
}

async fn unknown_endpoint() -> ApiError {
    ApiError::NotFound("No such endpoint.".to_owned())
}

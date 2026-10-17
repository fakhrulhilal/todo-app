//! The JSON API, its OpenAPI document and Swagger UI.

mod extract;
pub mod health;
pub mod problem;
pub mod todos;

use std::sync::Arc;

use axum::{Router, middleware, response::Redirect, routing::get};
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_swagger_ui::SwaggerUi;

use self::{
    health::{DatabaseCheck, HealthChecks},
    problem::ApiError,
};
use crate::store::Store;

pub const OPENAPI_PATH: &str = "/api/openapi.json";
pub const SWAGGER_UI_PATH: &str = "/swagger-ui";

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

/// Routes under `/api`, `/health` and `/swagger-ui`.
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
        .route("/", get(|| async { Redirect::temporary(SWAGGER_UI_PATH) }))
        .fallback(unknown_endpoint)
        .layer(middleware::from_fn(problem::problem_details))
        .merge(SwaggerUi::new(SWAGGER_UI_PATH).url(OPENAPI_PATH, openapi))
}

async fn unknown_endpoint() -> ApiError {
    ApiError::NotFound("No such endpoint.".to_owned())
}

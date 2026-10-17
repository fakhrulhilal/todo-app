//! The JSON API and its OpenAPI document. No documentation UI is served: any
//! OpenAPI viewer (Scalar in compose.yaml) reads the document instead.

pub mod health;
pub mod problem;
pub mod todos;

use std::sync::Arc;

use ntex::{
    http::{Method, StatusCode, header::LOCATION},
    web::{self, HttpResponse},
};
use utoipa::OpenApi;

use self::{
    health::{DatabaseCheck, HealthChecks},
    problem::{ApiError, method_not_allowed, unknown_endpoint},
};
use crate::store::Store;

pub const OPENAPI_PATH: &str = "/api/openapi.json";

/// Application state. Every worker gets a clone, and everything in it is
/// shared by all of them: the store and the checks are `Arc`s, made once.
#[derive(Clone)]
pub struct AppState {
    pub store: Store,
    pub health: Arc<HealthChecks>,
    /// The OpenAPI document, serialized once.
    openapi: Arc<str>,
}

impl AppState {
    pub fn new(store: Store) -> Self {
        Self {
            // What the readiness probe checks. Add one per dependency.
            health: Arc::new(HealthChecks::new().add("database", DatabaseCheck(store.clone()))),
            store,
            openapi: ApiDoc::openapi()
                .to_json()
                .expect("the OpenAPI document serializes")
                .into(),
        }
    }
}

/// Selects the error domain: every error a request can end in is rendered
/// as a problem (`problem.rs`).
impl web::State for AppState {
    type Error = ApiError;
}

#[derive(OpenApi)]
#[openapi(
    info(title = "Todo API", description = "A todo list. Errors are RFC 9457 problem details."),
    paths(
        todos::list_todos,
        todos::create_todo,
        todos::get_todo,
        todos::update_todo,
        todos::delete_todo,
        health::live,
        health::ready,
    ),
    tags(
        (name = todos::TAG, description = "Manage todos"),
        (name = health::TAG, description = "Liveness and readiness probes"),
    )
)]
struct ApiDoc;

/// GET, which also answers HEAD, as axum's does: ntex leaves the body out.
fn get<In: 'static>() -> web::Route<AppState, In> {
    web::get().method(Method::HEAD)
}

/// Routes under `/api` and `/health`, the OpenAPI document and the root
/// redirect.
pub fn routes(config: &mut web::ServiceConfig<AppState>) {
    config.service(
        web::scope("/api/v1")
            .configure(todos::routes)
            // A matched scope never falls back to the app's default service.
            .default_service(web::to(unknown_endpoint)),
    );
    health::routes(config);
    config.service(
        web::resource(OPENAPI_PATH)
            .route(get().to_with_state(openapi))
            .default_service(web::to(method_not_allowed("GET,HEAD"))),
    );
    config.service(
        web::resource("/")
            .route(get().to(async || {
                HttpResponse::builder(StatusCode::TEMPORARY_REDIRECT)
                    .header(LOCATION, OPENAPI_PATH)
                    .build()
            }))
            .default_service(web::to(method_not_allowed("GET,HEAD"))),
    );
}

async fn openapi(state: &AppState, _: ()) -> HttpResponse {
    HttpResponse::Ok()
        .content_type("application/json")
        .body(state.openapi.to_string())
}

//! The JSON API and its OpenAPI document. No documentation UI is served: any
//! OpenAPI viewer (Scalar in compose.yaml) reads the document instead.

pub mod health;
pub mod problem;
pub mod todos;

use actix_web::{
    HttpResponse, Route, guard,
    http::header::ContentType,
    web::{self, Bytes, Data, Redirect},
};
use health_checks::HealthCheckService;
use utoipa::OpenApi;

use self::problem::method_not_allowed;
use crate::store::Store;

pub const OPENAPI_PATH: &str = "/api/openapi.json";

/// Application state, registered once as `web::Data` and shared by every
/// worker. Cloning it is cheap: everything in it is reference-counted.
#[derive(Clone)]
pub struct AppState {
    pub store: Store,
    pub health: HealthCheckService,
    /// The OpenAPI document, serialized once.
    openapi: Bytes,
}

impl AppState {
    pub fn new(store: Store) -> Self {
        Self {
            // What the readiness probe checks: see `health::checks`.
            health: health::checks(store.clone()),
            store,
            openapi: ApiDoc::openapi()
                .to_json()
                .expect("the OpenAPI document serializes")
                .into(),
        }
    }
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

/// GET, which also answers HEAD, as axum's does. actix's `web::get()` only
/// matches GET; for HEAD, actix-http sends the headers and leaves the body out.
fn get() -> Route {
    web::route().guard(guard::Any(guard::Get()).or(guard::Head()))
}

/// Routes under `/api` and `/health`, the OpenAPI document and the root
/// redirect. Each path is a resource whose default service answers 405 for
/// the methods it does not have; a path without a resource reaches the app's
/// default service (404).
pub fn routes(config: &mut web::ServiceConfig) {
    config.service(web::scope("/api/v1").configure(todos::routes));
    health::routes(config);
    config.service(
        web::resource(OPENAPI_PATH)
            .route(get().to(openapi))
            .default_service(method_not_allowed("GET,HEAD")),
    );
    config.service(
        web::resource("/")
            .route(get().to(|| async { Redirect::to(OPENAPI_PATH) }))
            .default_service(method_not_allowed("GET,HEAD")),
    );
}

async fn openapi(state: Data<AppState>) -> HttpResponse {
    HttpResponse::Ok()
        .content_type(ContentType::json())
        .body(state.openapi.clone())
}

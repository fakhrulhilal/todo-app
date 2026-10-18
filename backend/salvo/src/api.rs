//! The JSON API and its OpenAPI document. No documentation UI is served: any
//! OpenAPI viewer (Scalar in compose.yaml) reads the document instead.

mod extract;
pub mod health;
pub mod problem;
pub mod todos;

use std::sync::Arc;

use salvo::{affix_state, prelude::*, writing::Text};
use utoipa::OpenApi;

use self::{
    health::{DatabaseCheck, HealthChecks},
    problem::MethodNotAllowed,
};
use crate::store::Store;

pub const OPENAPI_PATH: &str = "/api/openapi.json";

/// Application state, put in every request's depot by `affix_state`. Cloning
/// it is cheap: everything in it is behind an `Arc`.
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

/// The state `router()` injected. Missing means the app was wired wrong, not
/// that the client did something wrong.
fn state(depot: &Depot) -> &AppState {
    depot
        .get_typed::<AppState>()
        .expect("AppState is injected by api::router")
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

/// Routes under `/api` and `/health`, the OpenAPI document and the root
/// redirect.
pub fn router(store: Store) -> Router {
    Router::new()
        .hoop(affix_state::inject(AppState::new(store)))
        .get(root)
        .head(root)
        .push(Router::with_path("api/v1").push(todos::router()))
        .push(health::router())
        .push(
            Router::with_path(OPENAPI_PATH.trim_start_matches('/'))
                .get(openapi_document)
                .head(openapi_document)
                .push(Router::new().goal(MethodNotAllowed("GET,HEAD"))),
        )
        .push(Router::new().goal(MethodNotAllowed("GET,HEAD")))
}

#[handler]
async fn openapi_document(depot: &mut Depot) -> Text<String> {
    Text::Json(state(depot).openapi.to_string())
}

#[handler]
async fn root() -> Redirect {
    Redirect::temporary(OPENAPI_PATH)
}

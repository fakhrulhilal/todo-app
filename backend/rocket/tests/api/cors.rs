//! The CORS fairing in `src/lib.rs`.

use rocket::http::{Header, Status};
use serde_json::json;

use crate::{WEB_ORIGIN, client};

#[rocket::async_test]
async fn only_the_web_client_origin_may_call_from_a_browser() {
    let client = client().await;
    let allowed = |response: &rocket::local::asynchronous::LocalResponse<'_>| {
        response
            .headers()
            .get_one("access-control-allow-origin")
            .map(str::to_owned)
    };

    // The preflight a browser sends before a JSON POST from another site.
    let preflight = client
        .options("/api/v1/todos")
        .header(Header::new("origin", WEB_ORIGIN))
        .header(Header::new("access-control-request-method", "POST"))
        .header(Header::new(
            "access-control-request-headers",
            "content-type",
        ))
        .dispatch()
        .await;
    assert_eq!(preflight.status(), Status::Ok);
    assert_eq!(allowed(&preflight).as_deref(), Some(WEB_ORIGIN));
    let methods = preflight
        .headers()
        .get_one("access-control-allow-methods")
        .unwrap()
        .to_owned();
    for method in ["GET", "POST", "PATCH", "DELETE"] {
        assert!(methods.contains(method), "{methods}");
    }
    assert!(preflight.into_bytes().await.unwrap_or_default().is_empty());

    // Errors carry the header too, or the client could not read the problem.
    let invalid = client
        .post("/api/v1/todos")
        .header(Header::new("origin", WEB_ORIGIN))
        .json(&json!({ "title": "" }))
        .dispatch()
        .await;
    assert_eq!(invalid.status(), Status::UnprocessableEntity);
    assert_eq!(allowed(&invalid).as_deref(), Some(WEB_ORIGIN));

    let stranger = client
        .get("/api/v1/todos")
        .header(Header::new("origin", "https://elsewhere.example"))
        .dispatch()
        .await;
    assert_eq!(stranger.status(), Status::Ok);
    assert_eq!(allowed(&stranger), None);

    // Without the preflight headers, OPTIONS is just another method.
    let options = client.options("/api/v1/todos").dispatch().await;
    assert_eq!(options.status(), Status::MethodNotAllowed);
}

//! The CORS middleware in `src/lib.rs`.

use ntex::http::{Method, StatusCode};
use serde_json::json;
use todo_backend_ntex::CorsOrigins;

use crate::{WEB_ORIGIN, problem, spawn_app};

#[ntex::test]
async fn only_the_web_client_origin_may_call_from_a_browser() {
    let server = spawn_app().await;
    let allowed = |response: &ntex::client::ClientResponse| {
        response
            .header("access-control-allow-origin")
            .map(|origin| origin.to_str().unwrap().to_owned())
    };

    // The preflight a browser sends before a JSON POST from another site.
    let preflight = server
        .request(Method::OPTIONS, "/api/v1/todos")
        .header("origin", WEB_ORIGIN)
        .header("access-control-request-method", "POST")
        .header("access-control-request-headers", "content-type")
        .send()
        .await
        .unwrap();
    assert_eq!(preflight.status(), StatusCode::OK);
    assert_eq!(allowed(&preflight).as_deref(), Some(WEB_ORIGIN));
    let methods = preflight
        .header("access-control-allow-methods")
        .unwrap()
        .to_str()
        .unwrap();
    for method in ["GET", "POST", "PATCH", "DELETE"] {
        assert!(methods.contains(method), "{methods}");
    }

    // Errors carry the header too, or the client could not read the problem.
    let invalid = server
        .post("/api/v1/todos")
        .header("origin", WEB_ORIGIN)
        .send_json(&json!({ "title": "" }))
        .await
        .unwrap();
    assert_eq!(invalid.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(allowed(&invalid).as_deref(), Some(WEB_ORIGIN));

    // ntex-cors refuses other sites outright, where tower-http only leaves
    // the header out.
    let stranger = server
        .get("/api/v1/todos")
        .header("origin", "https://elsewhere.example")
        .send()
        .await
        .unwrap();
    assert_eq!(allowed(&stranger), None);
    let body = problem(stranger).await;
    assert_eq!(body.status, 400);
    assert_eq!(body.instance.as_deref(), Some("/api/v1/todos"));

    // Requests without an origin do not come from a browser page.
    let curl = server.get("/api/v1/todos").send().await.unwrap();
    assert_eq!(curl.status(), StatusCode::OK);
}

#[test]
fn origins_are_checked_when_the_server_starts() {
    assert_eq!(CorsOrigins::parse(" * ").unwrap(), CorsOrigins::Any);
    assert_eq!(
        CorsOrigins::parse("https://a.example/, https://b.example").unwrap(),
        CorsOrigins::Only(vec![
            "https://a.example".to_owned(),
            "https://b.example".to_owned()
        ])
    );
    assert!(CorsOrigins::parse("not an origin").is_err());
}

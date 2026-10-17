//! The CORS layer in `src/lib.rs`.

use reqwest::{Client, Method, StatusCode};
use serde_json::json;

use crate::{WEB_ORIGIN, spawn_app};

#[tokio::test]
async fn only_the_web_client_origin_may_call_from_a_browser() {
    let base_url = spawn_app().await;
    let client = Client::new();
    let todos = format!("{base_url}/api/v1/todos");
    let allowed = |response: &reqwest::Response| {
        response
            .headers()
            .get("access-control-allow-origin")
            .map(|origin| origin.to_str().unwrap().to_owned())
    };

    // The preflight a browser sends before a JSON POST from another site.
    let preflight = client
        .request(Method::OPTIONS, &todos)
        .header("origin", WEB_ORIGIN)
        .header("access-control-request-method", "POST")
        .header("access-control-request-headers", "content-type")
        .send()
        .await
        .unwrap();
    assert_eq!(preflight.status(), StatusCode::OK);
    assert_eq!(allowed(&preflight).as_deref(), Some(WEB_ORIGIN));
    let methods = preflight.headers()["access-control-allow-methods"]
        .to_str()
        .unwrap();
    for method in ["GET", "POST", "PATCH", "DELETE"] {
        assert!(methods.contains(method), "{methods}");
    }

    // Errors carry the header too, or the client could not read the problem.
    let invalid = client
        .post(&todos)
        .header("origin", WEB_ORIGIN)
        .json(&json!({ "title": "" }))
        .send()
        .await
        .unwrap();
    assert_eq!(invalid.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(allowed(&invalid).as_deref(), Some(WEB_ORIGIN));

    let stranger = client
        .get(&todos)
        .header("origin", "https://elsewhere.example")
        .send()
        .await
        .unwrap();
    assert_eq!(allowed(&stranger), None);
}

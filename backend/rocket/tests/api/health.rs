//! `src/api/health.rs`

use rocket::http::{ContentType, Status};
use serde_json::{Value, json};

use crate::{client, problem};

#[rocket::async_test]
async fn probes_answer_with_the_status_alone() {
    let client = client().await;

    for path in ["/health/live", "/health/ready"] {
        let response = client.get(path).dispatch().await;
        assert_eq!(response.status(), Status::Ok, "{path}");
        assert_eq!(
            response.headers().get_one("cache-control"),
            Some("no-store, no-cache")
        );
        let content_type = response.headers().get_one("content-type").unwrap();
        assert!(content_type.starts_with("text/plain"), "{content_type}");
        assert_eq!(response.into_string().await.unwrap(), "Healthy", "{path}");
    }
}

/// The crate's JSON (HealthChecks.UI's): `self` alone for liveness, the
/// `dependency` checks for readiness.
#[rocket::async_test]
async fn format_json_adds_the_result_of_each_check() {
    let client = client().await;
    let detail = async |path: &str| {
        let response = client.get(format!("{path}?format=json")).dispatch().await;
        assert_eq!(response.status(), Status::Ok);
        assert_eq!(response.content_type(), Some(ContentType::JSON));
        response.into_json::<Value>().await.unwrap()
    };

    let live = detail("/health/live").await;
    assert_eq!(live["status"], "Healthy");
    assert_eq!(entries(&live), ["self"]);
    assert_eq!(live["entries"]["self"]["status"], "Healthy");

    let ready = detail("/health/ready").await;
    assert_eq!(ready["status"], "Healthy");
    assert_eq!(entries(&ready), ["database"]);
    assert_eq!(ready["entries"]["database"]["tags"], json!(["dependency"]));
}

fn entries(report: &Value) -> Vec<String> {
    report["entries"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect()
}

#[rocket::async_test]
async fn an_unknown_format_is_a_problem() {
    let client = client().await;

    let response = client.get("/health/ready?format=xml").dispatch().await;
    let body = problem(response).await;
    assert_eq!(body.status, 400);
    assert_eq!(body.instance.as_deref(), Some("/health/ready"));
}

//! Reading the request, with failures reported as problem details.
//!
//! Salvo's extractors answer every failure with a bare 400 ("parse http data
//! failed"). These read the same parts and fail with the statuses and wording
//! of axum's rejections, as the other backends do: a body that is not JSON is
//! 400, JSON of the wrong shape 422, another content type 415.

use salvo::{
    http::{ParseError, StatusCode},
    prelude::*,
};
use serde::de::DeserializeOwned;
use uuid::Uuid;

use super::problem::ApiError;

/// The largest JSON body accepted, axum's default.
pub const JSON_LIMIT: usize = 2 * 1024 * 1024;

/// The body as JSON. The content type must be `application/json` or
/// another `+json` type.
pub async fn json<T: DeserializeOwned>(req: &mut Request) -> Result<T, ApiError> {
    let is_json = req.content_type().is_some_and(|mime| {
        mime.type_() == "application"
            && (mime.subtype() == "json" || mime.suffix().is_some_and(|suffix| suffix == "json"))
    });
    if !is_json {
        return Err(ApiError::Rejected(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Expected request with `Content-Type: application/json`".to_owned(),
        ));
    }

    let body = req
        .payload_with_max_size(JSON_LIMIT)
        .await
        .map_err(|error| match error {
            ParseError::PayloadTooLarge => ApiError::Rejected(
                StatusCode::PAYLOAD_TOO_LARGE,
                "Failed to buffer the request body: length limit exceeded".to_owned(),
            ),
            error => ApiError::Rejected(
                StatusCode::BAD_REQUEST,
                format!("Failed to buffer the request body: {error}"),
            ),
        })?;

    serde_json::from_slice(body).map_err(|error| {
        if error.is_data() {
            ApiError::Rejected(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("Failed to deserialize the JSON body into the target type: {error}"),
            )
        } else {
            ApiError::Rejected(
                StatusCode::BAD_REQUEST,
                format!("Failed to parse the request body as JSON: {error}"),
            )
        }
    })
}

/// A UUID from the path, such as `{id}` in `/todos/{id}`.
pub fn uuid_param(req: &Request, name: &str) -> Result<Uuid, ApiError> {
    let value = req
        .params()
        .get(name)
        .map(String::as_str)
        .unwrap_or_default();
    Uuid::parse_str(value).map_err(|error| {
        ApiError::Rejected(
            StatusCode::BAD_REQUEST,
            format!("Invalid URL: Cannot parse `{name}` with value `{value}` as a UUID: {error}"),
        )
    })
}

/// The query string as `T`.
pub fn query<T: DeserializeOwned>(req: &mut Request) -> Result<T, ApiError> {
    req.parse_queries().map_err(|error| {
        ApiError::Rejected(
            StatusCode::BAD_REQUEST,
            format!("Failed to deserialize query string: {error}"),
        )
    })
}

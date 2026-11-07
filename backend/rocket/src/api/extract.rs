//! Reading requests, with failures reported as problem details in the
//! statuses and wording of axum's rejections.
//!
//! - The body: `Json<T>` wraps Rocket's own `Json` guard, whose statuses
//!   already match axum's (400 for a body that is not JSON, 422 for JSON of
//!   the wrong shape, 413 above the `json` limit). It adds what Rocket's does
//!   not check, the content type (415), and gives each failure its detail.
//! - The path: Rocket parses `<id>` with `FromParam`. The handler takes a
//!   `Result`, as a failed parameter would otherwise forward to a 404 (or
//!   Rocket's 422), and `path` turns the error into a 400.
//! - The query: Rocket hands a `?<name>` parameter over as text, and `query`
//!   reads it with serde, so an unknown value is a 400 that names the choices.

use rocket::{
    Request,
    data::{self, Data, FromData},
    http::Status,
    serde::json,
};
use serde::de::{
    DeserializeOwned,
    value::{Error as ValueError, StrDeserializer},
};

use super::problem::{ApiError, Rejection};

/// A JSON request body.
pub struct Json<T>(pub T);

#[rocket::async_trait]
impl<'r, T: DeserializeOwned> FromData<'r> for Json<T> {
    type Error = ();

    async fn from_data(request: &'r Request<'_>, data: Data<'r>) -> data::Outcome<'r, Self> {
        // application/json, or a JSON type such as application/merge-patch+json.
        let is_json = request.content_type().is_some_and(|content_type| {
            content_type.is_json()
                || (content_type.top() == "application"
                    && content_type.sub().as_str().ends_with("+json"))
        });
        if !is_json {
            return fail(
                request,
                Status::UnsupportedMediaType,
                "Expected request with `Content-Type: application/json`".to_owned(),
            );
        }

        match json::Json::<T>::from_data(request, data).await {
            data::Outcome::Success(json::Json(value)) => data::Outcome::Success(Json(value)),
            data::Outcome::Forward(forward) => data::Outcome::Forward(forward),
            data::Outcome::Error((status, error)) => {
                let detail = match (status, error) {
                    (Status::PayloadTooLarge, _) => {
                        "Failed to buffer the request body: length limit exceeded".to_owned()
                    }
                    (Status::UnprocessableEntity, json::Error::Parse(_, error)) => {
                        format!("Failed to deserialize the JSON body into the target type: {error}")
                    }
                    (_, json::Error::Parse(_, error)) => {
                        format!("Failed to parse the request body as JSON: {error}")
                    }
                    (_, json::Error::Io(error)) => {
                        format!("Failed to buffer the request body: {error}")
                    }
                };
                fail(request, status, detail)
            }
        }
    }
}

fn fail<'r, T>(request: &Request<'_>, status: Status, detail: String) -> data::Outcome<'r, T, ()> {
    data::Outcome::Error((
        Rejection::keep(request, ApiError::Rejected(status, detail)),
        (),
    ))
}

/// A path parameter that `FromParam` could not parse, such as an id that is
/// not a UUID.
pub fn path<T, E: std::fmt::Display>(value: Result<T, E>) -> Result<T, ApiError> {
    value.map_err(|error| ApiError::Rejected(Status::BadRequest, format!("Invalid URL: {error}")))
}

/// A query parameter, read the way serde reads a string: an enum by its
/// variant names. Left out, it is the type's default.
pub fn query<T: DeserializeOwned + Default>(value: Option<&str>) -> Result<T, ApiError> {
    let Some(value) = value else {
        return Ok(T::default());
    };
    T::deserialize(StrDeserializer::<ValueError>::new(value)).map_err(|error| {
        ApiError::Rejected(
            Status::BadRequest,
            format!("Failed to deserialize query string: {error}"),
        )
    })
}

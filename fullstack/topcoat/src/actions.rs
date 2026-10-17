//! What the web UI can do to the todos. The browser supplies every argument,
//! so each action validates its input. An `Err` is a message that is safe to
//! show to the user.

use uuid::Uuid;

use crate::{
    domain::{UpdateTodo, new_todo, validate_title},
    store::{Store, StoreError},
};

pub type Outcome = Result<(), String>;

pub async fn add(store: &Store, title: &str) -> Outcome {
    let title = validate_title(title).map_err(|reason| format!("The title {reason}."))?;
    store.insert(&new_todo(title)).await.map_err(unexpected)
}

pub async fn set_completed(store: &Store, id: &str, completed: bool) -> Outcome {
    update(
        store,
        id,
        UpdateTodo {
            completed: Some(completed),
            ..UpdateTodo::default()
        },
    )
    .await
}

pub async fn rename(store: &Store, id: &str, title: &str) -> Outcome {
    let title = validate_title(title).map_err(|reason| format!("The title {reason}."))?;
    update(
        store,
        id,
        UpdateTodo {
            title: Some(title),
            ..UpdateTodo::default()
        },
    )
    .await
}

pub async fn remove(store: &Store, id: &str) -> Outcome {
    let id = parse_id(id)?;
    // A todo that is already gone is what the user asked for.
    store.delete(id).await.map(|_| ()).map_err(unexpected)
}

async fn update(store: &Store, id: &str, changes: UpdateTodo) -> Outcome {
    let id = parse_id(id)?;
    match store.update(id, &changes).await.map_err(unexpected)? {
        Some(_) => Ok(()),
        None => Err("That todo no longer exists.".to_owned()),
    }
}

fn parse_id(id: &str) -> Result<Uuid, String> {
    id.parse().map_err(|_| "That is not a todo.".to_owned())
}

/// Database messages can leak schema and credentials: log them, show nothing.
fn unexpected(error: StoreError) -> String {
    tracing::error!(error = %error, "could not change the todos");
    "Something went wrong. Try again shortly.".to_owned()
}

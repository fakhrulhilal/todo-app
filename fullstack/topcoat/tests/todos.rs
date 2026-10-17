//! The actions behind the web UI's procedures, against an in-memory database.

use todo_fullstack::{
    actions,
    config::DatabaseConfig,
    domain::StatusFilter,
    store::{self, Store},
};

async fn memory_store() -> Store {
    store::connect(&DatabaseConfig {
        url: "sqlite::memory:".to_owned(),
        max_connections: 1,
        migration: true,
    })
    .await
    .unwrap()
}

async fn titles(store: &Store, filter: StatusFilter) -> Vec<String> {
    store
        .list(filter)
        .await
        .unwrap()
        .into_iter()
        .map(|todo| todo.title)
        .collect()
}

#[tokio::test]
async fn todo_lifecycle() {
    let store = memory_store().await;

    actions::add(&store, "  Buy milk  ").await.unwrap();
    actions::add(&store, "Walk the dog").await.unwrap();
    assert_eq!(
        titles(&store, StatusFilter::All).await,
        ["Buy milk", "Walk the dog"]
    );

    let milk = store.list(StatusFilter::All).await.unwrap().remove(0);
    let id = milk.id.to_string();
    actions::set_completed(&store, &id, true).await.unwrap();
    assert_eq!(titles(&store, StatusFilter::Active).await, ["Walk the dog"]);
    assert_eq!(titles(&store, StatusFilter::Completed).await, ["Buy milk"]);

    actions::rename(&store, &id, "Buy oat milk").await.unwrap();
    assert_eq!(
        titles(&store, StatusFilter::Completed).await,
        ["Buy oat milk"]
    );

    actions::remove(&store, &id).await.unwrap();
    assert_eq!(titles(&store, StatusFilter::All).await, ["Walk the dog"]);
    // Deleting it again is what the user asked for, not an error.
    actions::remove(&store, &id).await.unwrap();
}

#[tokio::test]
async fn input_from_the_browser_is_validated() {
    let store = memory_store().await;

    let message = actions::add(&store, "   ").await.unwrap_err();
    assert_eq!(message, "The title must not be empty.");
    assert!(actions::add(&store, &"x".repeat(201)).await.is_err());
    assert!(titles(&store, StatusFilter::All).await.is_empty());

    assert!(
        actions::set_completed(&store, "not-a-uuid", true)
            .await
            .is_err()
    );
    let unknown = "019a0000-0000-7000-8000-000000000000";
    assert!(actions::rename(&store, unknown, "Anything").await.is_err());
}

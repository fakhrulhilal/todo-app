//! The todo model lives in `todo-contract`, where the frontend reads it too.
//! What only the server does with it is here.

use chrono::{DateTime, Utc};
use uuid::Uuid;

pub use todo_contract::*;

pub fn new_todo(title: String) -> Todo {
    let now = now();
    Todo {
        id: Uuid::now_v7(),
        title,
        completed: false,
        created_at: now,
        updated_at: now,
    }
}

/// The current time at the precision every backend stores (microseconds).
pub fn now() -> DateTime<Utc> {
    use chrono::SubsecRound;
    Utc::now().trunc_subsecs(6)
}

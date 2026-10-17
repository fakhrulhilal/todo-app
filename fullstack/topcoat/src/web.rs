//! The web UI, rendered by Topcoat. The page and the list are rendered on the
//! server; the browser only runs the small expressions Topcoat compiles from
//! `$(...)`. Whatever needs data goes back to the server:
//!
//! - the todo list is a *shard*, rendered again whenever its inputs change;
//! - adding, completing, renaming and deleting are *procedures*, called from
//!   the click handlers.
//!
//! See `docs/web-ui.md`.

use topcoat::{
    Result,
    asset::{AssetBundle, RouterBuilderAssetExt},
    context::{Cx, app_context},
    router::{
        Router, RouterBuilderDiscoverExt, Slot, StatusCode, error::NotFoundError, href, layout,
        module_router, not_found, page, query_params,
    },
    runtime::{Event, RouterBuilderRuntimeExt, Signal, procedure, shard, signal},
    view::{View, class, component, error_boundary, view},
};

use crate::{
    actions,
    domain::{MAX_TITLE_CHARS, StatusFilter},
    store::{Store, StoreError},
};

pub fn router(store: Store, assets: AssetBundle) -> Router {
    module_router!()
        .app_context(store)
        .discover()
        .assets(assets)
        // Register application layers before this.
        .runtime()
        .build()
}

// Unmatched URLs render the 404 page through the layout.
not_found!();

#[query_params]
struct ListQuery {
    filter: Option<String>,
}

#[layout]
async fn shell(slot: Slot<'_>) -> Result<impl View> {
    Ok(view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <title>"Todos"</title>
                <link rel="stylesheet" href=(crate::STYLESHEET_PATH)>
                topcoat::runtime::script()
            </head>
            <body>
                <header class="site">
                    <a class="brand" href=(href!(home))>"Todos"</a>
                </header>
                <main>
                    error_boundary(
                        fallback: |error| {
                            let (status, title, detail) = if error
                                .downcast_ref::<NotFoundError>()
                                .is_some() {
                                (
                                    StatusCode::NOT_FOUND,
                                    "Not found",
                                    "That page does not exist.",
                                )
                            } else if let Some(error) = error.downcast_ref::<StoreError>(

                            ) {
                                tracing::error!(error = %error, "could not load the todos");
                                (
                                    StatusCode::INTERNAL_SERVER_ERROR,
                                    "Something went wrong",
                                    "The todos could not be loaded. Try again shortly.",
                                )
                            } else {
                                return Err(error);
                            };

                            Ok(
                                view! {
                                    (status)
                                    <section class="notice">
                                        <h1>(title)</h1>
                                        <p>(detail)</p>
                                        <a href=(href!(home))>"Back to the list"</a>
                                    </section>
                                },
                            )
                        },
                        (slot)
                    )
                </main>
            </body>
        </html>
    })
}

#[page]
pub async fn home(cx: &Cx) -> Result<impl View> {
    // `/?filter=active` opens the list on that filter.
    let initial = query_params::<ListQuery>(cx)
        .ok()
        .and_then(|query| query.filter.as_deref())
        .and_then(|filter| filter.parse::<StatusFilter>().ok())
        .unwrap_or_default();

    // Browser state. The shard below reads `revision` and `filter`, so it is
    // rendered again when either changes.
    let filter = signal(cx, || initial.as_str().to_owned());
    let revision = signal(cx, || 0usize);
    let draft = signal(cx, String::new);
    let error = signal(cx, String::new);

    Ok(view! {
        <div class="add">
            <input
                type="text"
                placeholder="What needs doing?"
                aria-label="New todo"
                maxlength=(MAX_TITLE_CHARS)
                autofocus=""
                :value=$(draft.get())
                @input=$(|e: Event| draft.set(e.target.value))
                @keydown=$(async |e: Event| {
                    if e.key == "Enter" {
                        let message = add_todo(draft.get()).await;
                        if message.is_empty() {
                            draft.set("".to_owned());
                            revision.increment();
                        }
                        error.set(message);
                    }
                })
            >
            <button
                type="button"
                class="primary"
                @click=$(async |_e| {
                    let message = add_todo(draft.get()).await;
                    if message.is_empty() {
                        draft.set("".to_owned());
                        revision.increment();
                    }
                    error.set(message);
                })
            >
                "Add"
            </button>
        </div>
        <p class="form-error" role="alert" :hidden=$(error.get().is_empty())>
            $(error.get())
        </p>

        <nav class="filters" aria-label="Filter todos">
            for option in StatusFilter::VALUES {
                let name = option.as_str();
                <button
                    type="button"
                    :aria-current=$(if filter.get() == name { "page" } else { "false" })
                    @click=$(|_e| filter.set(name.to_owned()))
                >
                    (filter_label(option))
                </button>
            }
        </nav>

        todo_list(
            filter: $(filter.get()),
            revision: revision.clone(),
            error: error.clone()
        )
    })
}

fn filter_label(filter: StatusFilter) -> &'static str {
    match filter {
        StatusFilter::All => "All",
        StatusFilter::Active => "Active",
        StatusFilter::Completed => "Completed",
    }
}

/// The list. Rendered on the server, and again whenever the filter changes or
/// an action bumps `revision`.
#[shard]
async fn todo_list(
    cx: &Cx,
    filter: String,
    revision: Signal<usize>,
    error: Signal<String>,
) -> Result<impl View> {
    // Reading it here is what makes the shard depend on it.
    let _ = revision.get();
    // The filter comes from the browser: anything unknown means "all".
    let todos = app_context::<Store>(cx)
        .list(filter.parse().unwrap_or_default())
        .await?;
    let count = todos.len();

    Ok(view! {
        <ul class="todos">
            #[key(todo.id.to_string())]
            for todo in &todos {
                todo_item(
                    id: todo.id.to_string(),
                    title: todo.title.clone(),
                    completed: todo.completed,
                    revision: &revision,
                    error: &error
                )
            }
        </ul>
        if count == 0 {
            <p class="empty">"Nothing to show here."</p>
        } else {
            <p class="count">(count) (if count == 1 { " todo" } else { " todos" })</p>
        }
    })
}

#[component]
async fn todo_item(
    cx: &Cx,
    id: String,
    title: String,
    completed: bool,
    revision: &Signal<usize>,
    error: &Signal<String>,
) -> Result<impl View> {
    let editing = signal(cx, || false);
    let draft = signal(cx, || title.clone());

    Ok(view! {
        <li class=(class!("todo", "done" if completed)) id=(id.clone())>
            <button
                type="button"
                class="check"
                aria-label="Done"
                aria-pressed=(if completed { "true" } else { "false" })
                @click=$(async move |_e| {
                    let message = set_completed(id.to_owned(), !completed).await;
                    if message.is_empty() {
                        revision.increment();
                    }
                    error.set(message);
                })
            ></button>

            <span class="title" :hidden=$(editing.get())>(&title)</span>
            <input
                class="title-edit"
                type="text"
                aria-label="Title"
                maxlength=(MAX_TITLE_CHARS)
                :hidden=$(!editing.get())
                :value=$(draft.get())
                @input=$(|e: Event| draft.set(e.target.value))
            >

            <button
                type="button"
                class="action"
                :hidden=$(editing.get())
                @click=$(|_e| editing.set(true))
            >
                "Edit"
            </button>
            <button
                type="button"
                class="action"
                :hidden=$(!editing.get())
                @click=$(async move |_e| {
                    let message = rename_todo(id.to_owned(), draft.get()).await;
                    if message.is_empty() {
                        editing.set(false);
                        revision.increment();
                    }
                    error.set(message);
                })
            >
                "Save"
            </button>
            <button
                type="button"
                class="action"
                :hidden=$(!editing.get())
                @click=$(|_e| editing.set(false))
            >
                "Cancel"
            </button>
            <button
                type="button"
                class="action danger"
                @click=$(async move |_e| {
                    let message = delete_todo(id.to_owned()).await;
                    if message.is_empty() {
                        revision.increment();
                    }
                    error.set(message);
                })
            >
                "Delete"
            </button>
        </li>
    })
}

// The procedures take what the browser sends and answer with a message for
// the user. An empty message means it worked. Each has a fixed path, named
// after it, rather than a hash of where it is in the source, so it can be
// called from outside the page too (bench.sh posts to add_todo).

#[procedure("/_topcoat/runtime/procedures/add_todo")]
async fn add_todo(cx: &Cx, title: String) -> Result<String> {
    let outcome = actions::add(app_context::<Store>(cx), &title).await;
    Ok(outcome.err().unwrap_or_default())
}

#[procedure("/_topcoat/runtime/procedures/set_completed")]
async fn set_completed(cx: &Cx, id: String, completed: bool) -> Result<String> {
    let outcome = actions::set_completed(app_context::<Store>(cx), &id, completed).await;
    Ok(outcome.err().unwrap_or_default())
}

#[procedure("/_topcoat/runtime/procedures/rename_todo")]
async fn rename_todo(cx: &Cx, id: String, title: String) -> Result<String> {
    let outcome = actions::rename(app_context::<Store>(cx), &id, &title).await;
    Ok(outcome.err().unwrap_or_default())
}

#[procedure("/_topcoat/runtime/procedures/delete_todo")]
async fn delete_todo(cx: &Cx, id: String) -> Result<String> {
    let outcome = actions::remove(app_context::<Store>(cx), &id).await;
    Ok(outcome.err().unwrap_or_default())
}

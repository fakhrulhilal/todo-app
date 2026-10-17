//! The web client: a static single-page app. Every read and write is a call
//! to the JSON API, and the list is updated in place.

mod api_client;

use std::future::Future;

use dioxus::prelude::*;
use todo_contract::{CreateTodo, MAX_TITLE_CHARS, StatusFilter, Todo, UpdateTodo};
use uuid::Uuid;

const STYLESHEET: Asset = asset!("/assets/app.css");

fn main() {
    dioxus::launch(App);
}

#[derive(Debug, Clone, PartialEq, Routable)]
#[rustfmt::skip]
enum Route {
    #[layout(Shell)]
        // The filter is a query parameter, so every list is the same static
        // page and reloads, bookmarks and the back button work on any host.
        #[route("/?:filter")]
        Home { filter: Option<StatusFilter> },
        #[route("/:..segments")]
        NotFound { segments: Vec<String> },
}

impl Route {
    /// Where the list under a filter lives: `/` or `/?filter=active`.
    fn list(filter: StatusFilter) -> NavigationTarget {
        match filter {
            // Written out, because the route would render as `/?`.
            StatusFilter::All => "/".into(),
            filter => Self::Home {
                filter: Some(filter),
            }
            .into(),
        }
    }
}

#[component]
fn App() -> Element {
    rsx! {
        document::Title { "Todos" }
        document::Stylesheet { href: STYLESHEET }
        Router::<Route> {}
    }
}

#[component]
fn Shell() -> Element {
    rsx! {
        header { class: "site",
            Link { class: "brand", to: Route::list(StatusFilter::All), "Todos" }
            a { href: "/docs/", "API docs" }
        }
        main { Outlet::<Route> {} }
    }
}

#[component]
fn NotFound(segments: Vec<String>) -> Element {
    rsx! {
        section { class: "notice",
            h1 { "Not found" }
            p { "That page does not exist." }
            Link { to: Route::list(StatusFilter::All), "Back to the list" }
        }
    }
}

#[component]
fn Home(filter: Option<StatusFilter>) -> Element {
    rsx! {
        TodoList { filter: filter.unwrap_or_default() }
    }
}

/// The list on screen and what the components do to it.
#[derive(Clone, Copy, PartialEq)]
struct Todos {
    items: Signal<Vec<Todo>>,
    error: Signal<String>,
    filter: ReadSignal<StatusFilter>,
}

impl Todos {
    /// Awaits an API call, showing its error next to the form if it fails.
    async fn run<T>(mut self, call: impl Future<Output = Result<T, String>>) -> Option<T> {
        self.error.set(String::new());
        match call.await {
            Ok(value) => Some(value),
            Err(message) => {
                self.error.set(message);
                None
            }
        }
    }

    /// Applies a created or updated todo: it may not belong under this filter.
    fn place(mut self, todo: Todo) {
        let shown = self.filter.peek().matches(&todo);
        let mut items = self.items.write();
        match (items.iter().position(|item| item.id == todo.id), shown) {
            (Some(index), true) => items[index] = todo,
            (Some(index), false) => drop(items.remove(index)),
            (None, true) => items.push(todo),
            (None, false) => {}
        }
    }

    fn remove(mut self, id: Uuid) {
        self.items.write().retain(|item| item.id != id);
    }
}

#[component]
fn TodoList(filter: ReadSignal<StatusFilter>) -> Element {
    let mut items = use_signal(Vec::<Todo>::new);
    let error = use_signal(String::new);
    let mut loaded = use_signal(|| false);
    let mut title = use_signal(String::new);
    let todos = Todos {
        items,
        error,
        filter,
    };

    // Loads the list at first, and again whenever the filter changes.
    use_resource(move || {
        let filter = filter();
        async move {
            if let Some(list) = todos.run(api_client::list(filter)).await {
                items.set(list);
                loaded.set(true);
            }
        }
    });

    let total = items.read().len();

    rsx! {
        form {
            class: "add",
            onsubmit: move |event| async move {
                event.prevent_default();
                let input = CreateTodo { title: title() };
                if let Some(todo) = todos.run(api_client::create(&input)).await {
                    title.set(String::new());
                    todos.place(todo);
                }
            },
            input {
                r#type: "text",
                name: "title",
                placeholder: "What needs doing?",
                aria_label: "New todo",
                maxlength: MAX_TITLE_CHARS as i64,
                required: true,
                value: "{title}",
                oninput: move |event| title.set(event.value()),
                onmounted: move |event| async move {
                    let _ = event.set_focus(true).await;
                },
            }
            button { r#type: "submit", class: "primary", "Add" }
        }
        if !error.read().is_empty() {
            p { class: "form-error", role: "alert", "{error}" }
        }

        nav { class: "filters", aria_label: "Filter todos",
            for option in StatusFilter::VALUES {
                // The link for the list on screen gets aria-current="page".
                Link { to: Route::list(option), {filter_label(option)} }
            }
        }

        ul { class: "todos",
            for todo in items() {
                TodoItem { key: "{todo.id}", todo, todos }
            }
        }
        if loaded() {
            if total == 0 {
                p { class: "empty", "Nothing to show here." }
            } else {
                p { class: "count",
                    "{total} "
                    if total == 1 { "todo" } else { "todos" }
                }
            }
        }
    }
}

fn filter_label(filter: StatusFilter) -> &'static str {
    match filter {
        StatusFilter::All => "All",
        StatusFilter::Active => "Active",
        StatusFilter::Completed => "Completed",
    }
}

#[component]
fn TodoItem(todo: Todo, todos: Todos) -> Element {
    let Todo { id, completed, .. } = todo;
    let mut error = todos.error;
    // The text in the rename box, while the todo is being renamed.
    let mut draft = use_signal(|| None::<String>);
    let mut saving = use_signal(|| false);

    let current_title = todo.title.clone();
    let save = use_callback(move |()| {
        // Removing the box blurs it, which lands here once more.
        let Some(title) = draft.peek().clone() else {
            return;
        };
        if *saving.peek() {
            return;
        }
        if title.trim() == current_title {
            draft.set(None);
            return;
        }

        saving.set(true);
        spawn(async move {
            let changes = UpdateTodo {
                title: Some(title),
                ..UpdateTodo::default()
            };
            if let Some(todo) = todos.run(api_client::update(id, &changes)).await {
                draft.set(None);
                todos.place(todo);
            }
            saving.set(false);
        });
    });

    rsx! {
        li { class: if completed { "todo done" } else { "todo" },
            button {
                r#type: "button",
                class: "check",
                aria_label: "Done",
                aria_pressed: "{completed}",
                onclick: move |_| async move {
                    let changes = UpdateTodo {
                        completed: Some(!completed),
                        ..UpdateTodo::default()
                    };
                    if let Some(todo) = todos.run(api_client::update(id, &changes)).await {
                        todos.place(todo);
                    }
                },
            }
            if let Some(value) = draft() {
                input {
                    r#type: "text",
                    class: "title-edit",
                    aria_label: "Title",
                    maxlength: MAX_TITLE_CHARS as i64,
                    value,
                    oninput: move |event| draft.set(Some(event.value())),
                    onmounted: move |event| async move {
                        let _ = event.set_focus(true).await;
                    },
                    onblur: move |_| save(()),
                    onkeydown: move |event| match event.key() {
                        Key::Enter => {
                            event.prevent_default();
                            save(());
                        }
                        Key::Escape => {
                            error.set(String::new());
                            draft.set(None);
                        }
                        _ => {}
                    },
                }
            } else {
                span { class: "title", "{todo.title}" }
            }
            button {
                r#type: "button",
                class: "action",
                onclick: move |_| draft.set(Some(todo.title.clone())),
                "Edit"
            }
            button {
                r#type: "button",
                class: "action danger",
                onclick: move |_| async move {
                    if todos.run(api_client::delete(id)).await.is_some() {
                        todos.remove(id);
                    }
                },
                "Delete"
            }
        }
    }
}

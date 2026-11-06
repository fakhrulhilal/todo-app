//! The list page. The list is loaded whenever the filter changes; every change
//! is a call to the JSON API whose answer is written into the list on screen.

use std::future::Future;

use sycamore::futures::spawn_local_scoped;
use sycamore::prelude::*;
use sycamore::web::js_sys;
use sycamore::web::wasm_bindgen::{JsCast, UnwrapThrowExt, prelude::Closure};
use sycamore_router::use_search_query;
use todo_contract::{CreateTodo, MAX_TITLE_CHARS, StatusFilter, Todo, UpdateTodo};
use uuid::Uuid;
use web_sys::{Event, HtmlInputElement, KeyboardEvent, SubmitEvent, UrlSearchParams};

use crate::api;

/// The list on screen and what the components do to it. Signals are `Copy`,
/// so this is passed to every row as it is.
#[derive(Clone, Copy)]
struct Todos {
    items: Signal<Vec<Todo>>,
    /// The message about the last failed change, shown next to the form.
    error: Signal<String>,
    filter: ReadSignal<StatusFilter>,
}

impl Todos {
    /// Awaits an API call, showing its error next to the form if it fails.
    async fn run<T>(self, call: impl Future<Output = Result<T, String>>) -> Option<T> {
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
    fn place(self, todo: Todo) {
        let shown = self.filter.get_untracked().matches(&todo);
        self.items.update(|items| {
            match (items.iter().position(|item| item.id == todo.id), shown) {
                (Some(index), true) => items[index] = todo,
                (Some(index), false) => drop(items.remove(index)),
                (None, true) => items.push(todo),
                (None, false) => {}
            }
        });
    }

    fn remove(self, id: Uuid) {
        self.items
            .update(|items| items.retain(|item| item.id != id));
    }
}

/// The filter in the address (`/?filter=active`), so reloads, bookmarks and
/// the back button work on any static host.
///
/// sycamore-router 0.9.4 tells `use_search_query` when a link changes only
/// the query, but not when Back or Forward does: its `popstate` handler
/// compares the path alone. So `popstate` is listened to here as well, and
/// the address is read again after either.
fn use_filter() -> ReadSignal<StatusFilter> {
    let followed_link = use_search_query("filter");
    let went_back = create_signal(());

    let on_popstate = Closure::<dyn Fn()>::new(move || went_back.set(()));
    let listener = on_popstate
        .as_ref()
        .unchecked_ref::<js_sys::Function>()
        .clone();
    window()
        .add_event_listener_with_callback("popstate", &listener)
        .unwrap_throw();
    on_cleanup(move || {
        let _ = window().remove_event_listener_with_callback("popstate", &listener);
        drop(on_popstate);
    });

    // A selector, not a memo: a memo tells its dependents every time it is
    // recomputed, a selector only when the value changed, so another
    // `popstate` under the same filter does not load the list again.
    create_selector(move || {
        followed_link.track();
        went_back.track();
        let search = window().location().search().unwrap_or_default();
        UrlSearchParams::new_with_str(&search)
            .ok()
            .and_then(|params| params.get("filter"))
            .and_then(|value| value.parse().ok())
            .unwrap_or_default()
    })
}

/// How far the list on screen has loaded.
#[derive(Clone, PartialEq)]
enum Load {
    Loading,
    Loaded,
    Failed(String),
}

#[component]
pub fn TodoPage() -> View {
    let filter = use_filter();
    let todos = Todos {
        items: create_signal(Vec::new()),
        error: create_signal(String::new()),
        filter,
    };

    // Loads the list at first, again whenever the filter changes, and on
    // Retry. A load still running when the effect runs again is aborted with
    // the effect's previous run, so a late answer never replaces a newer one.
    let load = create_signal(Load::Loading);
    let reload = create_signal(());
    create_effect(move || {
        let filter = filter.get();
        reload.track();
        spawn_local_scoped(async move {
            match api::list(filter).await {
                Ok(list) => batch(|| {
                    todos.items.set(list);
                    load.set(Load::Loaded);
                }),
                Err(message) => load.set(Load::Failed(message)),
            }
        });
    });

    let title = create_signal(String::new());
    let adding = create_signal(false);
    let on_submit = move |event: SubmitEvent| {
        event.prevent_default();
        let input = CreateTodo {
            title: title.get_clone_untracked(),
        };
        adding.set(true);
        spawn_local_scoped(async move {
            if let Some(todo) = todos.run(api::create(input)).await {
                title.set(String::new());
                todos.place(todo);
            }
            adding.set(false);
        });
    };

    // Sycamore 0.9.4 declares `autofocus` twice for `input`, which makes the
    // attribute ambiguous in `view!`, so the box is focused once it is mounted.
    let new_todo = create_node_ref();
    on_mount(move || focus(new_todo, false));

    view! {
        form(class="add", on:submit=on_submit) {
            input(
                r#ref=new_todo,
                r#type="text",
                name="title",
                placeholder="What needs doing?",
                aria-label="New todo",
                maxlength=MAX_TITLE_CHARS.to_string(),
                required=true,
                bind:value=title,
            )
            button(r#type="submit", class="primary", disabled=adding) { "Add" }
        }
        if !todos.error.with(String::is_empty) {
            p(class="form-error", role="alert") { (todos.error) }
        }

        FilterLinks(current=filter)

        if let Load::Failed(message) = load.get_clone() {
            div(class="load-error", role="alert") {
                span { (message) }
                button(r#type="button", class="action", on:click=move |_| reload.set(())) {
                    "Retry"
                }
            }
        }
        ul(class="todos") {
            // Keyed by the version as well: a changed todo gets a fresh row.
            Keyed(
                list=todos.items,
                view=move |todo| view! { TodoItem(todo=todo, todos=todos) },
                key=|todo| (todo.id, todo.updated_at),
            )
        }
        if load.get_clone() == Load::Loaded {
            ListFooter(count=todos.items.with(Vec::len))
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

/// Where the list under a filter lives: `/` or `/?filter=active`.
fn filter_href(filter: StatusFilter) -> String {
    match filter {
        StatusFilter::All => "/".to_owned(),
        filter => format!("/?filter={filter}"),
    }
}

/// Links rather than buttons: each filter is its own address, which the
/// router follows without reloading the page.
#[component(inline_props)]
fn FilterLinks(current: ReadSignal<StatusFilter>) -> View {
    let links = StatusFilter::VALUES
        .into_iter()
        .map(|option| {
            view! {
                a(
                    href=filter_href(option),
                    aria-current=move || (current.get() == option).then_some("page"),
                ) { (filter_label(option)) }
            }
        })
        .collect::<Vec<_>>();
    view! {
        nav(class="filters", aria-label="Filter todos") { (links) }
    }
}

#[component(inline_props)]
fn ListFooter(count: usize) -> View {
    match count {
        0 => view! { p(class="empty") { "Nothing to show here." } },
        1 => view! { p(class="count") { "1 todo" } },
        total => view! { p(class="count") { (total) " todos" } },
    }
}

#[component(inline_props)]
fn TodoItem(todo: Todo, todos: Todos) -> View {
    let Todo {
        id,
        title,
        completed,
        ..
    } = todo;
    // A row's own signals are disposed with it, and `place` and `remove`
    // replace or drop this row. So each call below updates the row first and
    // ends with them; touching a signal of a disposed row panics, which
    // stops the whole app.
    let busy = create_signal(false);

    let on_toggle = move |event: Event| {
        let checkbox: HtmlInputElement = event_target(&event);
        let changes = UpdateTodo {
            completed: Some(checkbox.checked()),
            ..UpdateTodo::default()
        };
        busy.set(true);
        spawn_local_scoped(async move {
            let saved = todos.run(api::update(id, changes)).await;
            busy.set(false);
            match saved {
                Some(todo) => todos.place(todo),
                // The box was ticked by the click; it was not saved.
                None => checkbox.set_checked(completed),
            }
        });
    };

    let on_delete = move |_| {
        busy.set(true);
        spawn_local_scoped(async move {
            let deleted = todos.run(api::delete(id)).await.is_some();
            busy.set(false);
            if deleted {
                todos.remove(id);
            }
        });
    };

    // The rename box: shown while `editing`, holding `draft`.
    let editing = create_signal(false);
    let draft = create_signal(String::new());
    // A signal, so `save` is `Copy` (it goes to the rename box) and the title
    // can be shown from inside the reactive `if` below.
    let current_title = create_signal(title.clone());
    let save = move || {
        // Removing the box blurs it, which lands here once more.
        if !editing.get_untracked() || busy.get_untracked() {
            return;
        }
        let new_title = draft.get_clone_untracked();
        if current_title.with_untracked(|current| new_title.trim() == current) {
            editing.set(false);
            return;
        }
        let changes = UpdateTodo {
            title: Some(new_title),
            ..UpdateTodo::default()
        };
        busy.set(true);
        spawn_local_scoped(async move {
            let saved = todos.run(api::update(id, changes)).await;
            busy.set(false);
            if let Some(todo) = saved {
                editing.set(false);
                todos.place(todo);
            }
        });
    };
    let cancel = move || {
        todos.error.set(String::new());
        editing.set(false);
    };
    let start_editing = {
        let title = title.clone();
        move |_| {
            draft.set(title.clone());
            editing.set(true);
        }
    };

    view! {
        li(class=if completed { "todo done" } else { "todo" }) {
            input(
                r#type="checkbox",
                class="check",
                aria-label="Done",
                checked=completed,
                disabled=busy,
                on:change=on_toggle,
            )
            if editing.get() {
                RenameBox(draft=draft, save=save, cancel=cancel)
            } else {
                span(class="title") { (current_title) }
            }
            button(r#type="button", class="action", disabled=busy, on:click=start_editing) {
                "Edit"
            }
            button(r#type="button", class="action danger", disabled=busy, on:click=on_delete) {
                "Delete"
            }
        }
    }
}

/// The box a title is renamed in: Enter or leaving it saves, Escape cancels.
#[component(inline_props)]
fn RenameBox<S, C>(draft: Signal<String>, save: S, cancel: C) -> View
where
    S: Fn() + Copy + 'static,
    C: Fn() + Copy + 'static,
{
    let on_keydown = move |event: KeyboardEvent| match event.key().as_str() {
        "Enter" => {
            event.prevent_default();
            save();
        }
        "Escape" => cancel(),
        _ => {}
    };

    let node = create_node_ref();
    on_mount(move || focus(node, true));

    view! {
        input(
            r#ref=node,
            r#type="text",
            class="title-edit",
            aria-label="Title",
            maxlength=MAX_TITLE_CHARS.to_string(),
            bind:value=draft,
            on:keydown=on_keydown,
            on:blur=move |_| save(),
        )
    }
}

/// Focuses an input, selecting its text if asked to.
fn focus(node: NodeRef, select: bool) {
    let input: HtmlInputElement = node.get().unchecked_into();
    let _ = input.focus();
    if select {
        input.select();
    }
}

fn event_target<T: JsCast>(event: &Event) -> T {
    event
        .target()
        .expect("an event from an element")
        .unchecked_into()
}

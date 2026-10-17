//! The list page. Lists are loaded and cached with leptos-fetch; every change
//! is a call to the JSON API whose answer is written back into the cache.

use std::time::Duration;

use leptos::{ev, html, prelude::*};
use leptos_fetch::{QueryClient, QueryOptions, QueryScopeLocal};
use leptos_router::hooks::use_query_map;
use todo_contract::{CreateTodo, MAX_TITLE_CHARS, StatusFilter, Todo, UpdateTodo};
use uuid::Uuid;

use crate::api;
use crate::components::ui::alert::{Alert, AlertDescription};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::ui::card::{Card, CardContent};
use crate::components::ui::checkbox::Checkbox;
use crate::components::ui::empty::{Empty, EmptyDescription, EmptyHeader};
use crate::components::ui::input::Input;
use crate::components::ui::skeleton::Skeleton;

type TodoList = Result<Vec<Todo>, String>;

/// The todos under each filter, cached per filter. Changes made on this page
/// are written into every cached list, so switching filters is instant; a list
/// older than 30 seconds is refetched in the background when it is shown again.
fn list_query() -> QueryScopeLocal<StatusFilter, TodoList> {
    QueryScopeLocal::new(api::list)
        .with_options(QueryOptions::new().with_stale_time(Duration::from_secs(30)))
}

/// The cached lists and the message about the last failed call.
#[derive(Clone, Copy)]
struct Todos {
    client: QueryClient,
    error: RwSignal<String>,
}

impl Todos {
    /// Awaits an API call, showing its error above the list if it fails.
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

    /// Writes a created or updated todo into every cached list, adding it
    /// where its filter now selects it and dropping it where it no longer does.
    fn place(self, todo: &Todo) {
        for filter in StatusFilter::VALUES {
            self.client
                .update_query(list_query(), filter, |list: Option<&mut TodoList>| {
                    let Some(Ok(items)) = list else { return };
                    let shown = filter.matches(todo);
                    match (items.iter().position(|item| item.id == todo.id), shown) {
                        (Some(index), true) => items[index] = todo.clone(),
                        (Some(index), false) => drop(items.remove(index)),
                        (None, true) => items.push(todo.clone()),
                        (None, false) => {}
                    }
                });
        }
    }

    fn remove(self, id: Uuid) {
        for filter in StatusFilter::VALUES {
            self.client
                .update_query(list_query(), filter, |list: Option<&mut TodoList>| {
                    if let Some(Ok(items)) = list {
                        items.retain(|item| item.id != id);
                    }
                });
        }
    }
}

#[component]
pub fn TodoPage() -> impl IntoView {
    let client: QueryClient = expect_context();
    let todos = Todos {
        client,
        error: RwSignal::new(String::new()),
    };

    // The filter is a query parameter, so every list is the same static page
    // and reloads, bookmarks and the back button work on any host.
    let query = use_query_map();
    let filter = Memo::new(move |_| {
        query
            .read()
            .get("filter")
            .and_then(|value| value.parse().ok())
            .unwrap_or_default()
    });
    let list = client.local_resource(list_query(), move || filter.get());

    let title = RwSignal::new(String::new());
    let create = Action::new_local(move |input: &CreateTodo| {
        let input = input.clone();
        async move {
            if let Some(todo) = todos.run(api::create(input)).await {
                title.set(String::new());
                todos.place(&todo);
            }
        }
    });
    let on_submit = move |event: ev::SubmitEvent| {
        event.prevent_default();
        create.dispatch(CreateTodo {
            title: title.get_untracked(),
        });
    };

    // The rows on screen. Empty while loading or after a failed load.
    let items = move || list.get().and_then(Result::ok).unwrap_or_default();

    view! {
        <Card class="gap-5 py-5">
            <CardContent class="flex flex-col gap-5 px-5">
                <form class="flex gap-2" on:submit=on_submit>
                    <Input
                        name="title"
                        placeholder="What needs doing?"
                        required=true
                        autofocus=true
                        bind_value=title
                        attr:aria-label="New todo"
                        attr:maxlength=MAX_TITLE_CHARS
                    />
                    <Button attr:r#type="submit" attr:disabled=move || create.pending().get()>
                        "Add"
                    </Button>
                </form>

                <Show when=move || !todos.error.read().is_empty()>
                    <Alert class="border-destructive/50 text-destructive" attr:role="alert">
                        <AlertDescription>{move || todos.error.get()}</AlertDescription>
                    </Alert>
                </Show>

                <FilterLinks current=filter />

                <Transition fallback=LoadingRows>
                    {move || {
                        list.get()
                            .and_then(Result::err)
                            .map(|message| view! { <LoadFailed message filter=filter.get() /> })
                    }}
                    <ul class="flex flex-col divide-y">
                        // Keyed by the version as well: a changed todo gets a fresh row.
                        <For
                            each=items
                            key=|todo| (todo.id, todo.updated_at)
                            let(todo)
                        >
                            <TodoItem todo todos />
                        </For>
                    </ul>
                    <ListFooter count=Signal::derive(move || list.read().as_ref().and_then(|l| l.as_ref().ok()).map(Vec::len)) />
                </Transition>
            </CardContent>
        </Card>
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

/// Links rather than buttons: each filter is its own address. Rust/UI's
/// `Button href` marks the current page by path alone, and all three share
/// the path `/`, so `aria-current` is set here from the query instead.
#[component]
fn FilterLinks(current: Memo<StatusFilter>) -> impl IntoView {
    view! {
        <nav
            aria-label="Filter todos"
            class="bg-muted text-muted-foreground inline-flex w-fit rounded-lg p-1 text-sm font-medium"
        >
            {StatusFilter::VALUES
                .into_iter()
                .map(|option| {
                    view! {
                        <a
                            href=filter_href(option)
                            aria-current=move || (current.get() == option).then_some("page")
                            class="rounded-md px-3 py-1 transition-colors hover:text-foreground aria-[current=page]:bg-background aria-[current=page]:text-foreground aria-[current=page]:shadow-sm"
                        >
                            {filter_label(option)}
                        </a>
                    }
                })
                .collect_view()}
        </nav>
    }
}

#[component]
fn LoadingRows() -> impl IntoView {
    view! {
        <div class="flex flex-col gap-3" aria-busy="true" aria-label="Loading">
            <Skeleton class="h-9 w-full" />
            <Skeleton class="h-9 w-full" />
            <Skeleton class="h-9 w-2/3" />
        </div>
    }
}

/// A failed load stays in the cache until it is retried.
#[component]
fn LoadFailed(message: String, filter: StatusFilter) -> impl IntoView {
    let client: QueryClient = expect_context();
    view! {
        <Alert class="flex items-center justify-between gap-4 border-destructive/50 text-destructive" attr:role="alert">
            <AlertDescription>{message}</AlertDescription>
            <Button
                variant=ButtonVariant::Outline
                size=ButtonSize::Sm
                on:click=move |_| {
                    client.invalidate_query(list_query(), filter);
                }
            >
                "Retry"
            </Button>
        </Alert>
    }
}

#[component]
fn ListFooter(count: Signal<Option<usize>>) -> impl IntoView {
    move || match count.get() {
        None => ().into_any(),
        Some(0) => view! {
            <Empty class="p-6">
                <EmptyHeader>
                    <EmptyDescription>"Nothing to show here."</EmptyDescription>
                </EmptyHeader>
            </Empty>
        }
        .into_any(),
        Some(total) => view! {
            <p class="text-muted-foreground text-sm">
                {total} {if total == 1 { " todo" } else { " todos" }}
            </p>
        }
        .into_any(),
    }
}

#[component]
fn TodoItem(todo: Todo, todos: Todos) -> impl IntoView {
    let Todo {
        id,
        title,
        completed,
        ..
    } = todo;

    let toggle = Action::new_local(move |completed: &bool| {
        let changes = UpdateTodo {
            completed: Some(*completed),
            ..UpdateTodo::default()
        };
        async move {
            if let Some(todo) = todos.run(api::update(id, changes)).await {
                todos.place(&todo);
            }
        }
    });
    let delete = Action::new_local(move |_: &()| async move {
        if todos.run(api::delete(id)).await.is_some() {
            todos.remove(id);
        }
    });

    // The rename box: shown while `editing`, holding `draft`.
    let editing = RwSignal::new(false);
    let draft = RwSignal::new(String::new());
    let rename = Action::new_local(move |new_title: &String| {
        let changes = UpdateTodo {
            title: Some(new_title.clone()),
            ..UpdateTodo::default()
        };
        async move {
            if let Some(todo) = todos.run(api::update(id, changes)).await {
                editing.set(false);
                todos.place(&todo);
            }
        }
    });

    let current_title = StoredValue::new(title.clone());
    let save = move || {
        // Removing the box blurs it, which lands here once more.
        if !editing.get_untracked() || rename.pending().get_untracked() {
            return;
        }
        let new_title = draft.get_untracked();
        if new_title.trim() == current_title.read_value().as_str() {
            editing.set(false);
        } else {
            rename.dispatch(new_title);
        }
    };
    let on_keydown = move |event: ev::KeyboardEvent| match event.key().as_str() {
        "Enter" => {
            event.prevent_default();
            save();
        }
        "Escape" => {
            todos.error.set(String::new());
            editing.set(false);
        }
        _ => {}
    };

    // Focuses the rename box once it is on screen.
    let rename_box = NodeRef::<html::Input>::new();
    Effect::new(move |_| {
        if let Some(input) = rename_box.get() {
            let _ = input.focus();
            input.select();
        }
    });

    let busy = move || toggle.pending().get() || delete.pending().get() || rename.pending().get();

    view! {
        <li class="flex items-center gap-3 py-2">
            <Checkbox
                checked=completed
                disabled=Signal::derive(busy)
                aria_label="Done"
                on_checked_change=Callback::new(move |checked| {
                    toggle.dispatch(checked);
                })
            />
            <Show
                when=move || editing.get()
                fallback=move || {
                    view! {
                        <span class=if completed {
                            "min-w-0 flex-1 break-words text-muted-foreground line-through"
                        } else {
                            "min-w-0 flex-1 break-words"
                        }>{current_title.get_value()}</span>
                    }
                }
            >
                <Input
                    class="h-8 flex-1"
                    bind_value=draft
                    node_ref=rename_box
                    attr:aria-label="Title"
                    attr:maxlength=MAX_TITLE_CHARS
                    on:keydown=on_keydown
                    on:blur=move |_| save()
                />
            </Show>
            <Button
                variant=ButtonVariant::Ghost
                size=ButtonSize::Sm
                attr:r#type="button"
                attr:disabled=busy
                on:click=move |_| {
                    draft.set(current_title.get_value());
                    editing.set(true);
                }
            >
                "Edit"
            </Button>
            <Button
                variant=ButtonVariant::Ghost
                size=ButtonSize::Sm
                class="text-destructive hover:text-destructive"
                attr:r#type="button"
                attr:disabled=busy
                on:click=move |_| {
                    delete.dispatch(());
                }
            >
                "Delete"
            </Button>
        </li>
    }
}

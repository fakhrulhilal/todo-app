//! The web client: a static single-page app built with Leptos and Rust/UI
//! components. Every read and write is a call to the JSON API.

mod api;
mod components;
mod todos;

use leptos::prelude::*;
use leptos_fetch::QueryClient;
use leptos_router::{
    components::{Route, Router, Routes},
    path,
};

use crate::components::ui::button::{Button, ButtonVariant};
use crate::components::ui::empty::{
    Empty, EmptyContent, EmptyDescription, EmptyHeader, EmptyTitle,
};

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}

#[component]
fn App() -> impl IntoView {
    // One cache for every API query on the page.
    QueryClient::new().provide();

    view! {
        <Router>
            <header class="mx-auto flex max-w-xl items-baseline justify-between px-4 pt-8 pb-4">
                <a href="/" class="text-2xl font-bold tracking-tight">
                    "Todos"
                </a>
                // rel=external: the router would otherwise treat a same-origin
                // API address as one of its own pages.
                <a
                    href=api::docs_url()
                    rel="external"
                    class="text-primary text-sm underline-offset-4 hover:underline"
                >
                    "API docs"
                </a>
            </header>
            <main class="mx-auto max-w-xl px-4 pb-12">
                <Routes fallback=NotFound>
                    <Route path=path!("/") view=todos::TodoPage />
                </Routes>
            </main>
        </Router>
    }
}

#[component]
fn NotFound() -> impl IntoView {
    view! {
        <Empty>
            <EmptyHeader>
                <EmptyTitle>"Not found"</EmptyTitle>
                <EmptyDescription>"That page does not exist."</EmptyDescription>
            </EmptyHeader>
            <EmptyContent>
                <Button href="/" variant=ButtonVariant::Outline>
                    "Back to the list"
                </Button>
            </EmptyContent>
        </Empty>
    }
}

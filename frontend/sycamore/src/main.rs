//! The web client: a static single-page app built with Sycamore. Every read
//! and write is a call to the JSON API, and the list is updated in place.

mod api;
mod todos;

use sycamore::prelude::*;
use sycamore_router::{HistoryIntegration, Route, Router};

#[derive(Route, Clone)]
enum AppRoute {
    // The filter is a query parameter (`/?filter=active`), read on the page.
    #[to("/")]
    List,
    #[not_found]
    NotFound,
}

fn main() {
    console_error_panic_hook::set_once();
    sycamore::render(App);
}

#[component]
fn App() -> View {
    view! {
        Router(
            integration=HistoryIntegration::new(),
            view=|route: ReadSignal<AppRoute>| view! {
                header(class="site") {
                    a(class="brand", href="/") { "Todos" }
                    // Served by the deployment (compose.yaml: Caddy passes /docs
                    // on). rel=external: the router would otherwise treat it
                    // as one of its own pages.
                    a(href="/docs/", rel="external") { "API docs" }
                }
                main {
                    match route.get_clone() {
                        AppRoute::List => todos::TodoPage(),
                        AppRoute::NotFound => NotFound(),
                    }
                }
            },
        )
    }
}

#[component]
fn NotFound() -> View {
    view! {
        section(class="notice") {
            h1 { "Not found" }
            p { "That page does not exist." }
            a(href="/") { "Back to the list" }
        }
    }
}

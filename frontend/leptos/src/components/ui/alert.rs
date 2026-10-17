// Rust/UI `alert`: copied from https://github.com/rust-ui/leptos-ui at 9cb8e9f
// (public/registry/styles/default/alert.md). The code is ours now; compare it
// with the registry by hand before taking upstream changes.

use leptos::prelude::*;
use leptos_ui::clx;

mod components {
    use super::*;
    clx! {Alert, div, "relative w-full rounded-lg border px-4 py-3 text-sm [&>svg+div]:translate-y-[-3px] [&>svg]:absolute [&>svg]:left-4 [&>svg]:top-4 [&>svg]:text-foreground [&>svg~*]:pl-7"}
    clx! {AlertTitle, h4, "mb-1 font-medium tracking-tight leading-none"}
    clx! {AlertDescription, p, "text-sm [&_p]:leading-relaxed"}
}

pub use components::*;

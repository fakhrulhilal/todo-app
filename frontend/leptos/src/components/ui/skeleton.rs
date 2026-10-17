// Rust/UI `skeleton`: copied from https://github.com/rust-ui/leptos-ui at 9cb8e9f
// (public/registry/styles/default/skeleton.md). The code is ours now; compare it
// with the registry by hand before taking upstream changes.

use leptos::prelude::*;
use leptos_ui::void;

const PULSE_ANIMATION: &str = "animate-pulse";

// TODO UI. Skeleton should be able to receive children (or not).

mod components {
    use super::*;
    void! {Skeleton, div, PULSE_ANIMATION, "rounded-md bg-muted"}
}

pub use components::*;

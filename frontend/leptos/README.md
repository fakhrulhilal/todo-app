# Leptos web client

The same client as [`frontend/dioxus`](../dioxus), written with
[Leptos](https://leptos.dev) 0.8: a static single-page app (client-side
rendering only) that calls the axum JSON API. It looks and behaves the same:
add, complete, rename (Enter saves, Escape cancels, leaving the box saves),
delete, and filter with `?filter=active|completed` in the address.

| Part | Choice |
|------|--------|
| Framework | Leptos 0.8, `csr` feature, stable Rust |
| Build | [Trunk](https://trunkrs.dev) 0.21 (`Trunk.toml`, `index.html`) |
| Styling | [Tailwind CSS](https://tailwindcss.com) 4, compiled by Trunk with the standalone CLI, so no Node |
| Components | [Rust/UI](https://www.rust-ui.com), copied into [`src/components/ui/`](src/components/ui) |
| API data | [leptos-fetch](https://github.com/zakstucke/leptos-fetch) 0.4 for caching, [gloo-net](https://crates.io/crates/gloo-net) for HTTP |
| Shared types | the [`contract`](../../contract) crate |

## Run

From this folder, with the backend running on port 8080:

```sh
trunk serve                                   # http://127.0.0.1:8082, rebuilds on save
TODO_API_URL=http://127.0.0.1:8090 trunk serve   # when the API is elsewhere
trunk build --release                         # static site in dist/
```

`TODO_API_URL` is read when the client is compiled. Empty means the address
the site itself is served from, which is how the compose image works.

## How it is put together

- [`src/main.rs`](src/main.rs): mounting, the router, the header and the
  not-found page.
- [`src/todos.rs`](src/todos.rs): the list page. Each filter's list is a
  leptos-fetch query keyed by the filter. Adding, completing, renaming and
  deleting call the API and then write the returned todo into every cached
  list, so switching filters shows the right list at once without a request.
  A cached list older than 30 seconds is refetched in the background when it is
  shown again. A failed load shows its message with a Retry button.
- [`src/api.rs`](src/api.rs): every HTTP call. Failures become messages for the
  user, including the field errors of the API's RFC 9457 problem documents.
- [`tailwind.css`](tailwind.css): the Tailwind entry point and the Rust/UI theme
  (Neutral base, Blue accent). Dark mode follows the system setting.

gloo-net futures are not `Send`, so the page uses the local variants:
`QueryScopeLocal`, `client.local_resource`, `Action::new_local`.

## Rust/UI components

Rust/UI is a registry of components to copy, not a crate, in the style of
shadcn/ui. The files in `src/components/ui/` are copies of the registry at
commit `9cb8e9f` of [rust-ui/leptos-ui](https://github.com/rust-ui/leptos-ui),
kept unchanged apart from their header and `cargo fmt`, so they can be
compared with upstream.
They depend on the small crates `leptos_ui` (the `clx!` and `variants!`
macros), `tw_merge`, `icons` and `strum`.

To add a component, copy it from <https://www.rust-ui.com> (or from
`public/registry/styles/default/<name>.md` in that repository) into
`src/components/ui/` and list it in `mod.rs`. The `ui` CLI (`ui add`) expects
a cargo-leptos project with `tailwind-input-file` set, so it does not fit this
Trunk setup as is.

Things learned while wiring it in:

- `leptos_ui` turns on Leptos's `nightly` feature. On a stable compiler that
  feature does nothing, so stable Rust is fine.
- Rust/UI's dialog, popover, select, dropdown and other overlays open and close
  through inline `<script>` code rather than signals. None of them are used
  here; check before adding one that the page must control from Rust.
- `Button` with `href` marks itself `aria-current="page"` by comparing paths
  only. The filter links share the path `/`, so they are plain links that set
  `aria-current` from the query.

## Size

A release build is about 660 KB of WebAssembly (about 270 KB gzipped) and 30 KB
of CSS. Two things keep it there: the `wasm-release` profile in the root
`Cargo.toml` (`opt-level = "z"`, full LTO), which Trunk uses through
`data-cargo-profile-release`, and using gloo-net 0.6, the version Leptos
itself depends on, so only one copy is bundled.

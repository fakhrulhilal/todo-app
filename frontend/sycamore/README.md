# Sycamore web client

The same client as [`frontend/dioxus`](../dioxus), written with
[Sycamore](https://sycamore.dev) 0.9: a static single-page app (client-side
rendering only) that calls the JSON API. It looks and behaves the same:
add, complete, rename (Enter saves, Escape cancels, leaving the box saves),
delete, and filter with `?filter=active|completed` in the address.

| Part | Choice |
|------|--------|
| Framework | Sycamore 0.9, stable Rust; every view written with the `view!` DSL, not the builder API |
| Routing | [sycamore-router](https://crates.io/crates/sycamore-router) 0.9 with `HistoryIntegration` |
| Build | [Trunk](https://trunkrs.dev) 0.21 (`Trunk.toml`, `index.html`) |
| Styling | plain CSS in [`assets/app.css`](assets/app.css), the Dioxus client's stylesheet with a native checkbox |
| HTTP | [gloo-net](https://crates.io/crates/gloo-net) |
| Shared types | the [`contract`](../../contract) crate |

## Run

From this folder, with the backend running on port 8080:

```sh
trunk serve                                      # http://127.0.0.1:8086, rebuilds on save
TODO_API_URL=http://127.0.0.1:8090 trunk serve   # when the API is elsewhere
trunk build --release                            # static site in dist/
```

`TODO_API_URL` is read when the client is compiled. Empty means the address
the site itself is served from, which is how the compose image works (on
<http://localhost:8085>). The "API docs" link opens `/docs/` on the same
site, which the deployment serves (compose.yaml), not this client.

## How it is put together

- [`src/main.rs`](src/main.rs): mounting, the router, the header and the
  not-found page.
- [`src/todos.rs`](src/todos.rs): the list page. An effect loads the list
  whenever the filter changes or Retry is pressed. Adding, completing,
  renaming and deleting call the API and write the returned todo into the
  list on screen, which `Keyed` renders by id and `updated_at`, so a changed
  todo gets a fresh row.
- [`src/api.rs`](src/api.rs): every HTTP call. Failures become messages for the
  user, including the field errors of the API's RFC 9457 problem documents.

## Things learned while writing it

- **Disposed signals panic.** A row's signals belong to the row, and a
  changed or deleted todo replaces or drops its row. Touching such a signal
  afterwards panics with "signal was disposed", and after a panic the
  WebAssembly app stops reacting altogether. So each row's async call resets
  the row's own state first and ends with the change to the list.
- **A load is cancelled with its effect.** `spawn_local_scoped` aborts the
  future when its scope is disposed, and an effect disposes what its previous
  run made. A list load started in the effect is therefore cancelled when the
  filter changes again, so a late answer never replaces a newer one.
- **Back and Forward between filters.** sycamore-router 0.9.4 tells
  `use_search_query` when a link changes only the query, but its `popstate`
  handler compares the path alone, so Back to `/?filter=active` from
  `/?filter=completed` changed nothing. `use_filter` in `todos.rs` listens to
  `popstate` as well and reads the address after either.
- **`create_memo` always notifies.** In 0.9.4 a memo tells its dependents
  each time it is recomputed, even to the same value. `create_selector`
  compares with `PartialEq` and stays quiet when nothing changed, so the
  filter is a selector.
- **Which parts of `view!` are reactive.** `if`, `match` and `(...)` are
  wrapped in a closure, and so re-run, when they contain a method call or
  another non-trivial expression; a literal, a variable or a field is used
  once. `if editing.get() { ... }` is therefore reactive and
  `class=if completed { ... }` (a plain `bool`) is not. A value used inside
  such a closure is moved into it, so a `String` shown in a reactive branch
  is best kept in a signal, which is `Copy`.
- **`autofocus` does not compile on `input`.** Sycamore 0.9.4 declares it both
  as a global and as an input attribute, so `view!` finds two methods. The
  new-todo box is focused through a `NodeRef` in `on_mount` instead.
- **The router handles links on its top-level nodes.** Its click handler is
  attached to the nodes the router's view returns, so links inside them
  (the filters, "Back to the list") navigate without reloading the page, and
  `rel="external"` opts a link out, as for "API docs".

## Size

A release build is about 220 KB of WebAssembly (about 95 KB gzipped) and 4 KB
of CSS, through the `wasm-release` profile in the root `Cargo.toml`
(`opt-level = "z"`, full LTO), which Trunk uses through
`data-cargo-profile-release`.

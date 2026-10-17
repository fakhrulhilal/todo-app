# How the web UI works

The app is server-rendered with [Topcoat](https://github.com/tokio-rs/topcoat).
The browser never talks to a JSON API: there is none. Everything that needs
data goes back to the server through Topcoat's own two mechanisms.

| Piece | Where it runs | What it does here |
|-------|---------------|-------------------|
| `#[page]`, `#[layout]`, `#[component]` | Server | Produce the HTML |
| Signals and `$(...)` expressions | Browser, as JavaScript generated from Rust | Hold the filter, the text being typed, the error message and which todo is being renamed |
| `#[shard]` | Server, asked by the browser | Renders the todo list again when its inputs change |
| `#[procedure]` | Server, called by the browser | Adds, completes, renames and deletes a todo |

```text
first load:   browser ──▶ GET /                              page and list, as HTML
a click:      browser ──▶ POST /_topcoat/.../procedures/...  procedure ──▶ database
              browser ──▶ POST /_topcoat/.../shards/...      list as HTML, patched into the page
```

## The flow of one action

1. The click handler (`@click=$(async move |_e| ...)` in `src/web.rs`) calls a
   procedure, for example `set_completed`, with the todo's id.
2. The procedure checks its input and changes the database
   (`src/actions.rs`). It answers with a message for the user; an empty
   message means it worked.
3. The handler shows the message, or, when it worked, bumps the `revision`
   signal.
4. The list is a shard that reads `revision` and the `filter`. When either
   changes the browser asks the server to render the list again, and Topcoat
   patches the new HTML in. Inputs and focus survive the patch.

## Rules the code follows

- **Everything from the browser is input.** Procedure arguments, shard
  arguments and signal values are validated in `src/actions.rs` and
  `todo_list`: ids are parsed as UUIDs, titles are trimmed and length-checked,
  an unknown filter means "all". Database errors are logged and never shown.
- **Pages, shards and procedures do not share guards.** There is no
  authentication here. If it is added, check it inside every procedure and
  shard, not only in the page.
- **Procedures answer with data, not errors.** A server `Err` would make the
  browser expression fail silently, so failures the user can fix are returned
  as a message.

## Assets

The browser runtime is a script that Topcoat embeds in the binary and bundles
at build time. The app loads the bundle from the `assets` directory next to
the executable and refuses to start without it:

```sh
cargo build --release -p todo-fullstack
topcoat asset bundle --release --package todo-fullstack   # target/release/assets
```

The container image builds the bundle with the binary and copies it to
`/bin/assets`.

## Testing

The procedures are thin wrappers over `actions`, which `tests/todos.rs` drives
against an in-memory database. The rendered page needs the asset bundle, so it
is checked by running the app rather than by an automated test.

# todo-fullstack (Topcoat)

A todo list in Rust, rendered on the server with
[Topcoat](https://github.com/tokio-rs/topcoat). One binary serves the page
and everything it needs: there is no JSON API, no OpenAPI document and no
hand-written JavaScript.

The server renders the page, including the list. The browser runs the small
expressions Topcoat compiles from Rust, and goes back to the server for data:
the list is a *shard* that is rendered again when the filter or the data
changes, and adding, completing, renaming and deleting are *procedures*.
[`docs/web-ui.md`](docs/web-ui.md) explains how the pieces fit.

| URL             | What                                               |
|-----------------|----------------------------------------------------|
| `/`             | The web UI (needs JavaScript). `/?filter=active` opens a filter |
| `/health/live`  | Liveness probe: healthy while the server answers   |
| `/health/ready` | Readiness probe: healthy when the database answers |

## Run

The app is a member of the repository's Cargo workspace; run these from the
repository root. It needs the [Topcoat CLI](https://crates.io/crates/topcoat-cli)
(`cargo install topcoat-cli`, the same version as the `topcoat` crate) because
the browser script is bundled at build time.

```sh
topcoat asset bundle --package todo-fullstack   # builds, then writes target/debug/assets
cargo run -p todo-fullstack
```

Open <http://127.0.0.1:8080>. Todos are stored in `data/todo.db`. Bundle again
after changing the code, and bundle with the profile you run: a release binary
needs `topcoat asset bundle --release`.

```sh
cargo run -p todo-fullstack -- --server-port 9000 --database-url postgres://todo:todo@localhost:5432/todo
```

The binary refuses to start without the `assets` directory next to it. Tests do
not need it: `cargo test -p todo-fullstack`.

## Configuration

The same settings as the axum backend
([configuration](../../README.md#configuration)), without
`server.cors_origins`: there is no API for other sites to call. Settings are
read from four places. Later ones win:

1. Built-in defaults: [`config/default.yaml`](config/default.yaml)
2. A YAML file: `config.yaml` in the working directory, or the file named by
   `--config` / `CONFIG`, which must exist
3. Environment variables: the key in upper case, with each dot as `__`
4. Command-line arguments: the key with each dot and underscore as `-`

| YAML key                   | Environment variable        | Argument                     | Default                 |
|----------------------------|-----------------------------|------------------------------|-------------------------|
| `env`                      | `ENV`                       | `--env`                      | `local`                 |
| `server.host`              | `SERVER__HOST`              | `--server-host`              | `127.0.0.1`             |
| `server.port`              | `SERVER__PORT`              | `--server-port`, `-p`        | `8080`                  |
| `database.url`             | `DATABASE__URL`             | `--database-url`             | `sqlite://data/todo.db` |
| `database.max_connections` | `DATABASE__MAX_CONNECTIONS` | `--database-max-connections` | `10`                    |
| `database.migration`       | `DATABASE__MIGRATION`       | `--database-migration`       | `true`                  |
| `log.level`                | `LOG__LEVEL`                | `--log-level`                | `info`                  |
| `log.format`               | `LOG__FORMAT`               | `--log-format`               | `auto`                  |

`todo-fullstack --help` lists the arguments. A value of the wrong type stops
the server at startup with a message that names the setting and where the
value came from.

### Database

| URL                              | Storage                                                        |
|----------------------------------|----------------------------------------------------------------|
| `sqlite://data/todo.db`          | SQLite-compatible file, using the embedded [Turso](https://github.com/tursodatabase/turso) engine |
| `sqlite:///var/lib/todo/todo.db` | The same, with an absolute path                                |
| `sqlite::memory:`                | In memory, lost on exit (quote it in YAML)                     |
| `postgres://user:pass@host/db`   | PostgreSQL                                                     |

The schema is the one every implementation shares, in [`db/`](../../db).
With `database.migration` on (the default) the app applies the pending
migrations on startup, from `db/sqlite/` or `db/postgres/migrations/`. Compose
turns it off, because pgschema applies `db/postgres/schema.sql` there before
the app starts (see the [root README](../../README.md#schema)).

### Logging

`log.format` is `text` (plain text), `json` (one JSON object per line) or
`auto`, which picks text when `env` is `local` and JSON when it is
`production`. `log.level` takes a level or filter directives such as
`info,sqlx=warn`. Requests for the health probes are logged at `debug`.

## Container

The image is `FROM scratch` with two static binaries: the app and
[tiny-hc](https://github.com/fakhrulhilal/tiny-hc) for the health check. It
builds for `linux/amd64` and `linux/arm64` (Docker on Apple silicon) by
cross-compiling, without emulation. The build context is the repository root,
because the app is a member of the Cargo workspace.

```sh
# For this machine
docker build -f fullstack/topcoat/Dockerfile -t todo-fullstack .
docker run --rm -p 8080:8080 -e DATABASE__URL=postgres://todo:todo@<host>:5432/todo todo-fullstack

# Both platforms (needs a docker-container builder or the containerd image store)
docker buildx build --platform linux/amd64,linux/arm64 \
  -f fullstack/topcoat/Dockerfile -t <registry>/todo-fullstack --push .

# With the rest of the repository's stack, on its PostgreSQL database
docker compose up -d
```

In the image the app runs as user `10001`, listens on `0.0.0.0:8080` and uses
`env: production` (JSON logs). Give it `DATABASE__URL` for PostgreSQL, as
compose does: in containers every implementation shares one PostgreSQL
database, so they stay compatible (see the
[root README](../../README.md#database)). SQLite is for local development only;
without `DATABASE__URL` the image falls back to it, in the `/data` volume.

The image's `HEALTHCHECK` calls `/health/ready`, so the container is reported
unhealthy while its database is unreachable. If you change
`SERVER__PORT`, override the health check to match.

The probes answer as the axum backend's do: `Healthy`, `Degraded` or
`Unhealthy` as plain text (503 when `Unhealthy`), and the result of each check
with `?format=json`.

## Layout

```text
src/
  main.rs        entry point
  lib.rs         wires the health probes and the web UI into one server
  cli.rs         command-line arguments
  config.rs      layered configuration
  logging.rs     text / JSON log output
  domain.rs      the todo model (from todo-contract) and what only the server does with it
  actions.rs     what the UI can do to the todos, with input validation
  store.rs       storage trait; store/ has the PostgreSQL and Turso backends
  health.rs      liveness and readiness probes
  web.rs         the page, the todo list shard and the procedures;
                 web/ has the stylesheet
config/          default configuration
tests/           the actions and the probes, against an in-memory database
docs/            design notes
Dockerfile       image of the app
```

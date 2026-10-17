# Todo List app

A todo list in Rust, split into parts. The **contract** is what the others
agree on; each implementation is named after its framework, so other Rust
projects of the same app can sit next to it and share the contract. An app
that serves its own page and API in one service is a **fullstack**
implementation, in a folder beside `backend/` and `frontend/`.

| Folder             | What                                                                                                             | Built with |
|--------------------|------------------------------------------------------------------------------------------------------------------|------------|
| `contract/`        | The todo model and validation, compiled by both sides                                                            | serde      |
| `backend/axum/`    | The JSON API server with Swagger docs, shipped as a container image                                              | [axum](https://github.com/tokio-rs/axum) |
| `frontend/dioxus/` | The web client. It compiles to WebAssembly and static files, so it can be hosted on GitHub Pages or any other static host | [Dioxus](https://dioxuslabs.com) |
| `frontend/leptos/` | The same web client, styled with Tailwind CSS and [Rust/UI](https://www.rust-ui.com) components, with API data cached by [leptos-fetch](https://github.com/zakstucke/leptos-fetch). See its [README](frontend/leptos/README.md) | [Leptos](https://leptos.dev) |
| `fullstack/topcoat/` | Server-rendered page in one binary, with no JSON API: the list is a Topcoat shard and the actions are procedures. See its [README](fullstack/topcoat/README.md) | [Topcoat](https://github.com/tokio-rs/topcoat) |

```text
browser ──▶ GitHub Pages            static files (HTML, CSS, WASM), loaded once
browser ──▶ /api/v1/todos (JSON) ──▶ backend ──▶ database
```

The frontend has no server of its own. Adding, completing, renaming, deleting
and filtering are calls to the JSON API, and the page is updated in place.
The HTTP API itself is described by the OpenAPI document at
`/api/openapi.json`. `cargo test` writes a copy to
[`contract/openapi.json`](contract/openapi.json) from the axum backend; commit
it when it changes. A second backend or frontend only has to follow that
document, and reuse the `contract/` crate where it is also written in Rust.

## Run locally

Needs the [Dioxus CLI](https://dioxuslabs.com/learn/0.7/getting_started/)
(`dx`), the same version as the `dioxus` crate.

The repository is one Cargo workspace. Run every command in this README from
the repository root, and pick the part with `-p <package>`:

| Folder             | Package         |
|--------------------|-----------------|
| `contract/`        | `todo-contract` |
| `backend/axum/`    | `todo-backend`  |
| `frontend/dioxus/` | `todo-frontend` |
| `frontend/leptos/` | `todo-frontend-leptos` |
| `fullstack/topcoat/` | `todo-fullstack` |

```sh
cargo run -p todo-backend                    # the backend, on http://127.0.0.1:8080
dx serve -p todo-frontend --port 8081        # the frontend, on http://127.0.0.1:8081
(cd frontend/leptos && trunk serve)          # the Leptos frontend, on http://127.0.0.1:8082
```

The Leptos frontend needs [Trunk](https://trunkrs.dev) instead of `dx`; Trunk
runs from the frontend's folder, where its `Trunk.toml` is.

Todos are stored in `data/todo.db`, relative to where the backend is started
(the repository root). SQLite is for local work only; containers use
PostgreSQL (see [Database](#database)). `cargo test` runs the contract and backend tests; the
frontend only builds for WebAssembly, so Cargo skips it unless asked.

The client is told where the API is when it is built, through `TODO_API_URL`.
Without it, it calls `http://127.0.0.1:8080`. Set to nothing
(`TODO_API_URL=`), it calls the address the site itself is served from.

If port 8080 is taken, move the API and tell the client:

```sh
cargo run -p todo-backend -- --server-port 8090
TODO_API_URL=http://127.0.0.1:8090 dx serve -p todo-frontend --port 8081
(cd frontend/leptos && TODO_API_URL=http://127.0.0.1:8090 trunk serve)
```

## Build

The two parts are built separately, with different tools, both from the
repository root.

### Backend (`backend/axum/`)

1. Install [Rust](https://rustup.rs).
2. Run the tests:

   ```sh
   cargo test -p todo-backend
   ```

3. Build the release binary:

   ```sh
   cargo build --release -p todo-backend
   ```

4. The result is one executable, `target/release/todo-backend`, with the default
   configuration and the migrations of both databases compiled in. Run it:

   ```sh
   ./target/release/todo-backend --server-host 0.0.0.0 --server-port 8080
   ```

To ship it as a container instead, see
[API container image](#api-container-image).

### Frontend (`frontend/dioxus/`)

1. Add the WebAssembly target:

   ```sh
   rustup target add wasm32-unknown-unknown
   ```

2. Install the Dioxus CLI, the same version as the `dioxus` crate:

   ```sh
   cargo install dioxus-cli --version 0.7.10 --locked
   # or, without compiling it: cargo binstall dioxus-cli@0.7.10
   ```

3. Build the site, giving it the address of the API:

   ```sh
   TODO_API_URL=https://todo-api.example.com \
     dx bundle -p todo-frontend --platform web --release --out-dir dist
   ```

   If the site will not be served from the root of its domain, add
   `--base-path <path>`. A GitHub Pages project site needs
   `--base-path <repository>`.

4. The result is the static site in `dist/public`: `index.html`, the
   stylesheet and the WebAssembly. Copy it to any static host.

5. Have the host serve `index.html` for unknown paths. On GitHub Pages that
   is done with a copy:

   ```sh
   cp dist/public/index.html dist/public/404.html
   ```

The first `dx` build is slow: it compiles every dependency for WebAssembly,
and `dx` may make rustup download the nightly toolchain.

### Frontend (`frontend/leptos/`)

1. Add the WebAssembly target:

   ```sh
   rustup target add wasm32-unknown-unknown
   ```

2. Install [Trunk](https://trunkrs.dev):

   ```sh
   cargo install trunk --version 0.21.14 --locked
   # or, without compiling it: cargo binstall trunk@0.21.14
   ```

   Nothing else: Trunk downloads the Tailwind CSS CLI, `wasm-bindgen` and
   `wasm-opt` on the first build. Node is not needed.

3. Build the site, giving it the address of the API:

   ```sh
   cd frontend/leptos
   TODO_API_URL=https://todo-api.example.com trunk build --release
   ```

   The release build uses the `wasm-release` profile from the root
   `Cargo.toml`, which optimises for size.

4. The result is the static site in `frontend/leptos/dist`: `index.html`, the
   stylesheet and the WebAssembly. Copy it to any static host, and have the
   host serve `index.html` for unknown paths.

The site expects to be served from the root of its domain.

## Database

Every implementation reads and writes the same `todos` table. Its schema is
kept in one place, [`db/`](db), for every implementation. Two databases are
supported, for different purposes:

| Where                       | Database   | Why                                                    |
|-----------------------------|------------|--------------------------------------------------------|
| Local development and tests | SQLite (embedded [Turso](https://github.com/tursodatabase/turso)), the default `sqlite://data/todo.db` | Nothing to install or start; each app keeps its own file |
| Containers                  | PostgreSQL | One database that every service shares                 |

In a container, every service uses PostgreSQL. This keeps the
implementations compatible with each other in both ways they can meet:

- **Through the API:** a frontend talks to any backend, and sees the same
  todos whichever one answers.
- **Directly through the database:** several apps share one database at the
  same time, as the axum backend and the Topcoat app do in
  [`compose.yaml`](compose.yaml). A todo added in one shows up in the other.

SQLite cannot do the second: each container would have its own file, and the
apps would drift apart. So in [`compose.yaml`](compose.yaml) every service gets
`DATABASE__URL` pointing at PostgreSQL. The images still default to SQLite in
their `/data` volume, so a bare `docker run` works without a database server.

### Schema

The schema is applied in one of two ways, chosen by the `database.migration`
setting (`DATABASE__MIGRATION`):

| `database.migration` | Who applies the schema                | Used by                               |
|----------------------|---------------------------------------|---------------------------------------|
| `true` (the default) | the app, on startup, from its numbered migrations | local development, a bare `docker run` |
| `false`              | something else, before the app starts | [`compose.yaml`](compose.yaml), where pgschema does it |

```text
db/
  sqlite/                    numbered migrations, applied by the app
  postgres/
    migrations/              numbered migrations, applied by the app (sqlx)
    schema.sql               the desired state, applied by pgschema
    .pgschemaignore          what pgschema leaves alone
```

The apps compile the numbered migrations in, so a binary or image migrates
without the repository next to it. With `database.migration` on, it applies
the ones not yet applied, in order, and records them (`_migrations` in SQLite,
`_sqlx_migrations` in PostgreSQL).

#### PostgreSQL in compose: pgschema

[pgschema](https://www.pgschema.com) is declarative. `schema.sql` is the
schema as it should be, not a list of steps: pgschema compares it with the
database and applies the difference, much like `terraform apply`. Compose runs
it on every `docker compose up` (the `db-init` service) and starts the apps,
with `database.migration` off, only after it succeeded. When nothing changed
it does nothing.

pgschema validates `schema.sql` in a scratch database before it compares.
Without `--plan-host` it would download an embedded PostgreSQL for that, so
compose gives it a `pgschema_plan` database on the `db` server instead (the
`db-plan-init` service creates it).

To see what it would change in a database of your own:

```sh
docker run --rm --network host -w /schema -e PGPASSWORD=todo \
  -v "$PWD/db/postgres:/schema:ro" pgplex/pgschema:1.13.3 \
  plan --host localhost --db todo --user todo --file /schema/schema.sql
```

Two things follow from the declarative model:

- Anything in the `public` schema that `schema.sql` does not describe is
  dropped, except what `.pgschemaignore` names (`_sqlx_migrations`, so a
  database the apps migrated can also be managed by pgschema).
- pgschema does not move data. A change that needs a backfill, such as a new
  `NOT NULL` column on a table with rows, goes in steps: add it nullable, fill
  it, then make it `NOT NULL`.

A database pgschema created has no `_sqlx_migrations`. Keep
`database.migration` off for apps that use it: with it on, an app would apply
`0001` again and stop at startup, because the table exists already.

#### Changing the schema

One change goes in three places, so every way of applying it ends with the
same schema:

1. `db/postgres/schema.sql`: edit it to the new state.
2. `db/postgres/migrations/000N_<name>.sql`: the step from the previous state.
3. `db/sqlite/000N_<name>.sql`: the same step for SQLite.

Never edit a migration that has been applied somewhere: sqlx compares
checksums and refuses to start on a changed file. To check that (1) and (2)
agree, let an app migrate an empty PostgreSQL, then run `pgschema plan`
against it: it must report no changes.

A new implementation reads the same tables. It applies the numbered
migrations of its database when `database.migration` is on, and nothing when
it is off.

## Run everything with Docker Compose

[`compose.yaml`](compose.yaml) builds and runs every implementation against
one PostgreSQL database:

- **Dioxus + axum**, on <http://localhost:8080>:
  [Caddy](https://caddyserver.com) serves the frontend and passes `/api`,
  `/swagger-ui` and `/health` on to the API. The browser sees a single site, so
  CORS is not involved.
- **Topcoat**, on <http://localhost:8081>: the server-rendered app.
- **Leptos + axum**, on <http://localhost:8083>: the Leptos client, set up
  like the Dioxus one, against the same API.

```sh
docker compose up -d
```

That is the only step. The first run builds the four images, which takes a
few minutes; later runs start at once. After changing the code, rebuild with
`docker compose up -d --build`.

The images are built from
- [`frontend/dioxus/Dockerfile`](frontend/dioxus/Dockerfile): the client,
  compiled with an empty `TODO_API_URL` so that it calls the address it was
  loaded from, and served by Caddy. Caddy's configuration is inside
  `compose.yaml`.
- [`frontend/leptos/Dockerfile`](frontend/leptos/Dockerfile): the Leptos
  client, built the same way with Trunk.
- [`backend/axum/Dockerfile`](backend/axum/Dockerfile): the API.
- [`fullstack/topcoat/Dockerfile`](fullstack/topcoat/Dockerfile): the
  fullstack app.

Every service uses the one PostgreSQL database (`db`), so a todo added in
one app shows up in the other. See [Database](#database).

| Variable            | What                                    | Default |
|---------------------|-----------------------------------------|---------|
| `TODO_WEB_PORT`     | Port the Dioxus site is published on    | `8080`  |
| `TODO_TOPCOAT_PORT` | Port the Topcoat app is published on    | `8081`  |
| `TODO_LEPTOS_PORT`  | Port the Leptos site is published on    | `8083`  |
| `TODO_DB_PASSWORD`  | Password of the PostgreSQL user         | `todo`  |

Todos are kept in the `db-data` volume; `docker compose down -v` deletes them.

Compose builds the images for the machine it runs on. Images to publish are
built for both platforms with `docker buildx bake`
(see [API container image](#api-container-image)).

## Deploy the frontend to GitHub Pages

[`.github/workflows/pages.yml`](.github/workflows/pages.yml) builds and
publishes the client on every push to `main`. Set up the repository once:

1. **Settings > Pages > Source**: GitHub Actions.
2. **Settings > Secrets and variables > Actions > Variables**: add
   `TODO_API_URL`, the public address of the API, such as
   `https://todo-api.example.com`. It must be `https`, because Pages is, and
   browsers block calls from an `https` page to an `http` API.
3. On the API, set `SERVER__CORS_ORIGINS` to the site's origin, such as
   `https://<user>.github.io`.

The workflow handles the two things Pages needs from a single-page app:

- A project site lives under `/<repository>/`, so the bundle is built with
  that base path.
- `index.html` is copied to `404.html`, so unknown paths are rendered by the
  app instead of GitHub's error page.

To build the same files by hand, follow the [frontend build](#frontend-frontenddioxus)
steps with `--base-path <repository>`.

## API

| URL                 | What                                                |
|---------------------|-----------------------------------------------------|
| `/api/v1/todos`     | JSON API                                            |
| `/swagger-ui`       | Swagger UI (`/` redirects here)                     |
| `/api/openapi.json` | OpenAPI 3.1 document                                |
| `/health/live`      | Liveness probe: is the process serving requests?    |
| `/health/ready`     | Readiness probe: do the things it depends on answer? |

```sh
curl -X POST localhost:8080/api/v1/todos -H 'content-type: application/json' -d '{"title":"Buy milk"}'
curl 'localhost:8080/api/v1/todos?status=active'
curl -X PATCH localhost:8080/api/v1/todos/<id> -H 'content-type: application/json' -d '{"completed":true}'
curl -X DELETE localhost:8080/api/v1/todos/<id>
```

### Errors

Every error is an [RFC 9457](https://www.rfc-editor.org/rfc/rfc9457) problem
details object with the `application/problem+json` content type, including
the ones the router raises itself (unknown path, wrong method, malformed
body, bad path or query value):

```json
{
  "type": "about:blank",
  "title": "Unprocessable Entity",
  "status": 422,
  "detail": "One or more members of the request are invalid.",
  "instance": "/api/v1/todos",
  "errors": { "title": ["must not be empty"] }
}
```

| Member     | What                                                              |
|------------|-------------------------------------------------------------------|
| `type`     | Always `about:blank`: the status code says what kind of problem   |
| `title`    | The name of the status code                                       |
| `status`   | The status code                                                   |
| `detail`   | What went wrong this time. Left out when there is nothing to add  |
| `instance` | The path that was requested, without the query string             |
| `errors`   | Validation failures only (422): member name to list of messages, as in ASP.NET Core's `ValidationProblemDetails` |

### Health checks

The probes follow [ASP.NET Core's health checks](https://learn.microsoft.com/aspnet/core/host-and-deploy/health-checks).
A probe answers with one word as plain text:

| Answer      | Status | Meaning                                              |
|-------------|--------|------------------------------------------------------|
| `Healthy`   | 200    | Every check passed                                   |
| `Degraded`  | 200    | Working, but a check reports it is not at its best   |
| `Unhealthy` | 503    | A check failed                                       |

With several checks, the answer is the worst of them. `/health/live` runs no
checks, so it is `Healthy` whenever the process answers. `/health/ready` runs
all of them (today: `database`). A check that takes longer than 2 seconds
counts as failed. Responses are never cached.

Add `?format=json` for the result of each check:

```sh
curl 'localhost:8080/health/ready?format=json'
```

```json
{
  "status": "Healthy",
  "results": {
    "database": {
      "status": "Healthy",
      "description": "The database answered.",
      "data": {}
    }
  }
}
```

Checks are registered in [`backend/axum/src/api.rs`](backend/axum/src/api.rs). To add one,
implement `HealthCheck` and register it by name. A check registered with
`add_with_failure_status(..., HealthStatus::Degraded, ...)` degrades the
service when it fails instead of making it unhealthy.

### Configuration

Settings are read from four sources and merged, in the manner of .NET's
options pattern. Later ones win:

1. Built-in defaults: [`backend/axum/config/default.yaml`](backend/axum/config/default.yaml)
2. A YAML file: `config.yaml` in the working directory if there is one, or the
   file named by `--config` / `CONFIG`, which must exist
3. Environment variables
4. Command-line arguments

Every setting has one name, its key in the YAML file. The other two follow
from it:

- **Environment variable:** the key in upper case, with each dot as `__`.
- **Argument:** the key with each dot and underscore as `-`.

| YAML key                   | Environment variable        | Argument                     | Default                 |
|----------------------------|-----------------------------|------------------------------|-------------------------|
| `env`                      | `ENV`                       | `--env`                      | `local`                 |
| `server.host`              | `SERVER__HOST`              | `--server-host`              | `127.0.0.1`             |
| `server.port`              | `SERVER__PORT`              | `--server-port`, `-p`        | `8080`                  |
| `server.cors_origins`      | `SERVER__CORS_ORIGINS`      | `--server-cors-origins`      | `*`                     |
| `database.url`             | `DATABASE__URL`             | `--database-url`             | `sqlite://data/todo.db` |
| `database.max_connections` | `DATABASE__MAX_CONNECTIONS` | `--database-max-connections` | `10`                    |
| `database.migration`       | `DATABASE__MIGRATION`       | `--database-migration`       | `true`                  |
| `log.level`                | `LOG__LEVEL`                | `--log-level`                | `info`                  |
| `log.format`               | `LOG__FORMAT`               | `--log-format`               | `auto`                  |

The same setting, three ways:

```yaml
# config.yaml
server:
  port: 9000
```

```sh
SERVER__PORT=9000 todo-backend
todo-backend --server-port 9000
```

`todo-backend --help` lists the arguments. A value of the wrong type stops the
server at startup with a message that names the setting and where the value
came from.

`server.cors_origins` names the sites whose pages may call the API from a
browser, separated by commas. The default `*` allows any site, which suits
local work; in production, name the site that hosts the frontend.

| Database URL                     | Storage                                                        |
|----------------------------------|----------------------------------------------------------------|
| `sqlite://data/todo.db`          | SQLite-compatible file, using the embedded [Turso](https://github.com/tursodatabase/turso) engine |
| `sqlite:///var/lib/todo/todo.db` | The same, with an absolute path                                |
| `sqlite::memory:`                | In memory, lost on exit (quote it in YAML)                     |
| `postgres://user:pass@host/db`   | PostgreSQL                                                     |

With `database.migration` on, the app applies the pending migrations on
startup, from [`db/sqlite/`](db/sqlite) or
[`db/postgres/migrations/`](db/postgres/migrations). Turn it off where
something else owns the schema, as compose does (see [Schema](#schema)).

`log.format` is `text` (plain text), `json` (one JSON object per line) or
`auto`, which picks text when `env` is `local` and JSON when it is
`production`. `log.level` takes a level or filter directives such as
`info,sqlx=warn`. Requests for the health probes are logged at `debug`.

## API container image

The image is `FROM scratch` with two static binaries: the API and
[tiny-hc](https://github.com/fakhrulhilal/tiny-hc) for the health check.

It is always built for `linux/amd64` and `linux/arm64` (Docker on Apple
silicon) together, as one multi-platform image under one tag. The
architecture is never part of the tag: Docker pulls the matching variant.
[`docker-bake.hcl`](docker-bake.hcl) fixes the platforms, so build with
`bake` instead of `docker build`:

```sh
docker buildx bake                                                    # build both platforms
IMAGE=ghcr.io/<user>/todo-dx-api TAG=1.0.0 docker buildx bake --push  # build and publish
docker buildx bake --load                                             # into the local image store
```

A multi-platform build needs a `docker-container` builder
(`docker buildx create --use`) or Docker's containerd image store, and
`--load` works only with the containerd image store. Both platforms are
cross-compiled, without emulation.

[`.github/workflows/image.yml`](.github/workflows/image.yml) runs the tests
and publishes the image to `ghcr.io/<owner>/<repository>-api`: `latest` and
the branch name on `main`, and the version for a `v1.2.3` tag.

```sh
docker run --rm -p 8080:8080 \
  -e DATABASE__URL=postgres://todo:todo@<host>:5432/todo \
  ghcr.io/<user>/todo-dx-api
```

In the image the API runs as user `10001`, listens on `0.0.0.0:8080` and uses
`env: production` (JSON logs). Give it `DATABASE__URL` for PostgreSQL, as
compose does: containers share one PostgreSQL database
(see [Database](#database)). Without it, the image falls back to SQLite in the
`/data` volume, which is only good for a quick try. Override any setting with
the environment variables above.

The image's `HEALTHCHECK` calls `/health/ready`, so the container is reported
unhealthy while its database is unreachable. If you change
`SERVER__PORT`, override the health check to match.

## Layout

```text
contract/              the todo model and validation, shared by every
                       implementation
  openapi.json         the API, written by the axum backend's tests
db/                    the database schema, shared by every implementation
  postgres/            PostgreSQL: numbered migrations and schema.sql for pgschema
  sqlite/              SQLite: numbered migrations
backend/
  axum/                the JSON API server
    src/               configuration, logging, storage backends, API and OpenAPI
    config/            default configuration
    tests/api/         end-to-end tests against an in-memory database,
                       one file per source file they cover
    Dockerfile         image of the backend
frontend/
  dioxus/              the web client
    src/main.rs        routes and components
    src/api_client.rs  calls to the JSON API
    assets/            stylesheet
    Dockerfile         image of the frontend, served by Caddy
  leptos/              the same web client in Leptos (see its README)
    src/components/ui/ Rust/UI components, copied into the repository
    tailwind.css       Tailwind entry point and theme
    Dockerfile         image of the frontend, served by Caddy
fullstack/
  topcoat/             server-rendered page in one service (see its README)
    Dockerfile         image of the app
docker-bake.hcl        the multi-platform build of the backend image
compose.yaml           every implementation and PostgreSQL
```

A new implementation goes next to the existing one, for example
`backend/<framework>/` with its own `Dockerfile`, and joins the
workspace in the root `Cargo.toml`. It uses the same contract: the types from
`contract/`, the schema in `db/`, and the configuration keys and health probes
of the axum backend.

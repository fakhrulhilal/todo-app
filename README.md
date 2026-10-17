# todo-dx

A todo list in Rust, in two parts that are deployed separately:

- **`web/`**: the web client, written with [Dioxus](https://dioxuslabs.com).
  It compiles to WebAssembly and static files, so it can be hosted on GitHub
  Pages or any other static host.
- **`api/`**: the JSON API server with Swagger docs, shipped as a container
  image.

```text
browser ──▶ GitHub Pages            static files (HTML, CSS, WASM), loaded once
browser ──▶ /api/v1/todos (JSON) ──▶ API server ──▶ database
```

The client has no server of its own. Adding, completing, renaming, deleting
and filtering are calls to the JSON API, and the page is updated in place.
`shared/` holds the todo model, which both sides compile against.

This is todo-rs with its server-rendered page and hand-written
JavaScript replaced by the Dioxus client.

## Run locally

Needs the [Dioxus CLI](https://dioxuslabs.com/learn/0.7/getting_started/)
(`dx`), the same version as the `dioxus` crate.

```sh
cargo run                               # the API, on http://127.0.0.1:8080
dx serve -p todo-web --port 8081        # the web client, on http://127.0.0.1:8081
```

Todos are stored in `data/todo.db`. `cargo test` runs the API tests.

The client is told where the API is when it is built, through `TODO_API_URL`.
Without it, it calls `http://127.0.0.1:8080`. Set to nothing
(`TODO_API_URL=`), it calls the address the site itself is served from.

If port 8080 is taken, move the API and tell the client:

```sh
cargo run -- --server-port 8090
TODO_API_URL=http://127.0.0.1:8090 dx serve -p todo-web --port 8081
```

## Build

The two parts are built separately, with different tools.

### Backend (`api/`)

1. Install [Rust](https://rustup.rs).
2. Run the tests:

   ```sh
   cargo test
   ```

3. Build the release binary:

   ```sh
   cargo build --release
   ```

4. The result is one executable, `target/release/todo-api`, with the default
   configuration and the database migrations compiled in. Run it:

   ```sh
   ./target/release/todo-api --server-host 0.0.0.0 --server-port 8080
   ```

To ship it as a container instead, see
[API container image](#api-container-image).

### Frontend (`web/`)

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
     dx bundle -p todo-web --platform web --release --out-dir dist
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

## Run everything with Docker Compose

[`compose.yaml`](compose.yaml) runs the whole app on one address:
[Caddy](https://caddyserver.com) serves the web client and passes `/api`,
`/swagger-ui` and `/health` on to the API, which stores todos in PostgreSQL.
The browser sees a single site, so CORS is not involved.

```sh
docker compose up -d      # then open http://localhost:8080
```

That is the only step. The first run builds the two images, which takes a
few minutes; later runs start at once. After changing the code, rebuild with
`docker compose up -d --build`.

The images are built from [`frontend.Dockerfile`](frontend.Dockerfile)
(the client, compiled with an empty `TODO_API_URL` so that it calls the
address it was loaded from, and served by Caddy) and
[`backend.Dockerfile`](backend.Dockerfile) (the API). Caddy's configuration
is inside `compose.yaml`.

| Variable           | What                            | Default |
|--------------------|---------------------------------|---------|
| `TODO_WEB_PORT`    | Port the site is published on   | `8080`  |
| `TODO_DB_PASSWORD` | Password of the PostgreSQL user | `todo`  |

Todos are kept in the `db-data` volume; `docker compose down -v` deletes them.

Compose builds the images for the machine it runs on. Images to publish are
built for both platforms with `docker buildx bake`
(see [API container image](#api-container-image)).

## Deploy the web client to GitHub Pages

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

To build the same files by hand, follow the [frontend build](#frontend-web)
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

Checks are registered in [`api/src/api.rs`](api/src/api.rs). To add one,
implement `HealthCheck` and register it by name. A check registered with
`add_with_failure_status(..., HealthStatus::Degraded, ...)` degrades the
service when it fails instead of making it unhealthy.

### Configuration

Settings are read from four sources and merged, in the manner of .NET's
options pattern. Later ones win:

1. Built-in defaults: [`api/config/default.yaml`](api/config/default.yaml)
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
| `log.level`                | `LOG__LEVEL`                | `--log-level`                | `info`                  |
| `log.format`               | `LOG__FORMAT`               | `--log-format`               | `auto`                  |

The same setting, three ways:

```yaml
# config.yaml
server:
  port: 9000
```

```sh
SERVER__PORT=9000 todo-api
todo-api --server-port 9000
```

`todo-api --help` lists the arguments. A value of the wrong type stops the
server at startup with a message that names the setting and where the value
came from.

`server.cors_origins` names the sites whose pages may call the API from a
browser, separated by commas. The default `*` allows any site, which suits
local work; in production, name the site that hosts the web client.

| Database URL                     | Storage                                                        |
|----------------------------------|----------------------------------------------------------------|
| `sqlite://data/todo.db`          | SQLite-compatible file, using the embedded [Turso](https://github.com/tursodatabase/turso) engine |
| `sqlite:///var/lib/todo/todo.db` | The same, with an absolute path                                |
| `sqlite::memory:`                | In memory, lost on exit (quote it in YAML)                     |
| `postgres://user:pass@host/db`   | PostgreSQL                                                     |

The schema is created and migrated on startup, from
[`api/migrations/`](api/migrations).

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
docker run --rm -p 8080:8080 -v todo-data:/data ghcr.io/<user>/todo-dx-api
```

In the image the API runs as user `10001`, listens on `0.0.0.0:8080`, uses
`env: production` (JSON logs) and stores SQLite data in the `/data` volume.
Override any of it with the environment variables above.

The image's `HEALTHCHECK` calls `/health/ready`, so the container is reported
unhealthy while its database is unreachable. If you change
`SERVER__PORT`, override the health check to match.

## Layout

```text
shared/          the todo model and validation, used by both sides
web/             the Dioxus web client
  src/main.rs    routes and components
  src/api.rs     calls to the JSON API
  assets/        stylesheet
api/             the JSON API server
  src/           configuration, logging, storage backends, API and OpenAPI
  config/        default configuration
  migrations/    SQL per database
  tests/api/     end-to-end tests against an in-memory database,
                 one file per source file they cover
frontend.Dockerfile  image of the web client, served by Caddy
backend.Dockerfile   image of the API
docker-bake.hcl      the multi-platform build of the API image
compose.yaml         Caddy (web client) + API + PostgreSQL
```

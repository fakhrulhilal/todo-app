# syntax=docker/dockerfile:1

# The web client, served by Caddy. The site is WebAssembly and static files,
# the same for every platform, so it is built once, on the build machine's
# own architecture. Caddy's configuration is supplied by compose.yaml.

ARG RUST_VERSION=1.98
ARG DIOXUS_CLI_VERSION=0.7.10

FROM --platform=$BUILDPLATFORM rust:${RUST_VERSION}-trixie AS build
ARG DIOXUS_CLI_VERSION
RUN rustup target add wasm32-unknown-unknown \
    && curl -L --proto '=https' --tlsv1.2 -sSf \
       https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash \
    && cargo binstall --no-confirm "dioxus-cli@${DIOXUS_CLI_VERSION}"

WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY contract contract
COPY frontend/dioxus frontend/dioxus
# The repository's workspace lists every implementation. Here it lists only
# what this image builds, so the other apps are neither copied nor needed.
# Cargo.lock lists their crates too; Cargo drops them, and every version still
# comes from Cargo.lock.
RUN sed -i -E 's#^(default-)?members = .*#\1members = ["contract", "frontend/dioxus"]#' Cargo.toml

# Where the client finds the API. Empty: at the address the site itself is
# served from, where Caddy passes API calls on.
ARG TODO_API_URL=""
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,target=/src/target,id=todo-dx-web-target,sharing=locked \
    dx bundle --package todo-frontend --platform web --release --out-dir /out

FROM caddy:2-alpine
COPY --from=build /out/public /srv

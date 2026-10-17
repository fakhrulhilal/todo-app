# syntax=docker/dockerfile:1

# The JSON API server on ntex. Built for linux/amd64 and linux/arm64:
#
#   docker buildx build --platform linux/amd64,linux/arm64 -f backend/ntex/Dockerfile .
#
# The compiler runs on the build machine's own architecture and
# cross-compiles, so no emulation.
#
# The workers run compio, which uses io_uring on Linux. Docker's default
# seccomp profile blocks io_uring; the server then falls back to epoll by
# itself (compio's `polling` feature), so nothing has to be allowed.

ARG RUST_VERSION=1.98
ARG XX_VERSION=1.9.0
ARG TINY_HC_VERSION=0.1.3

FROM --platform=$BUILDPLATFORM tonistiigi/xx:${XX_VERSION} AS xx
FROM iroel/tiny-hc:${TINY_HC_VERSION} AS tiny-hc

FROM --platform=$BUILDPLATFORM rust:${RUST_VERSION}-alpine AS build
COPY --from=xx / /
RUN apk add --no-cache clang lld musl-dev
ARG TARGETPLATFORM
# C toolchain pieces for the target: ring and Turso compile a little C.
RUN xx-apk add --no-cache musl-dev gcc

WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY contract contract
# The migrations of both databases, compiled in.
COPY db db
COPY backend/ntex backend/ntex
# The repository's workspace lists every implementation. Here it lists only
# what this image builds, so the other apps are neither copied nor needed.
RUN sed -i -E 's#^(default-)?members = .*#\1members = ["contract", "backend/ntex"]#' Cargo.toml

# No --locked: Cargo.lock lists the other apps' crates too, and Cargo drops
# them for this smaller workspace. Every version still comes from Cargo.lock.
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,target=/src/target,id=todo-ntex-target-${TARGETPLATFORM},sharing=locked \
    CARGO_PROFILE_RELEASE_STRIP=true \
    xx-cargo build --release --package todo-backend-ntex \
    && mkdir -p /out/data \
    && cp "target/$(xx-cargo --print-target-triple)/release/todo-backend-ntex" /out/todo-backend-ntex \
    && xx-verify --static /out/todo-backend-ntex

# Nothing but the static binaries: no shell, no libc, no package manager.
FROM scratch
COPY --from=tiny-hc /bin/tiny-hc /bin/tiny-hc
COPY --from=build /out/todo-backend-ntex /bin/todo-backend-ntex
# The SQLite database lives here; an empty volume inherits this ownership.
COPY --from=build --chown=10001:10001 /out/data /data

USER 10001:10001
ENV ENV=production \
    SERVER__HOST=0.0.0.0 \
    SERVER__PORT=8080 \
    DATABASE__URL=sqlite:///data/todo.db
EXPOSE 8080
VOLUME /data

# Readiness: the container is healthy when it can reach its database. Use
# /health/live instead to only check that the server answers. The port must
# match SERVER__PORT.
HEALTHCHECK --interval=10s --timeout=3s --start-period=10s --retries=3 \
    CMD ["/bin/tiny-hc", "http://127.0.0.1:8080/health/ready"]

ENTRYPOINT ["/bin/todo-backend-ntex"]

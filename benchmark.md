# Benchmark

Adding a todo on PostgreSQL, measured on every implementation on 2026-11-06,
after the Rocket backend was added, with each app given a pool of 50
database connections.

- **Rocket** is level with axum at 50 connections (18,400 requests/sec), about
  8% behind actix-web, and with axum and Salvo at 200 (16,000), where its p99
  (23 ms) is the second lowest. Its tail is the longest: p99.9 568 ms and one
  request of 3.0 s at 200 connections. On a single connection it was the
  slowest (1,408 against 1,554 to 1,750).
- **actix-web** is the fastest at 50 connections (20,000) and falls to 14,900
  at 200, with a p95 of 40 ms there.
- **Topcoat** is the fastest at 200 connections (17,900, the lowest p99) and on
  one connection, and second at 50.
- **axum** is the fastest at 10 connections (8,200), level with Rocket at 50,
  and holds 16,400 at 200.
- **ntex** is the slowest at 200 connections (11,300, p99.9 337 ms); at 10 it
  is close to axum.
- **Salvo** had a weak step at 50 connections (11,900, p99 36 ms); at 200 it is
  level with Rocket and axum. See [Notes](#notes).
- Every request succeeded: 201 from the backends, 200 from Topcoat.

## How it was measured

```sh
./bench.sh all
```

[`bench.sh`](bench.sh) builds each app from the current source and starts it
in a throwaway copy of the compose stack (project `todo-bench`) with its own
PostgreSQL 17, then runs [oha](https://github.com/hatoo/oha) against it: a 5 s
warm-up at 50 connections, then 15 s at each of 1, 10, 50 and 200
connections, without Caddy in front. Before measuring, it checks that the
rows land in PostgreSQL, and after the warm-up it counts the open database
connections (46 to 50 of the 50 in this run).

| Implementation       | Request                                                        | Runtime                                  |
|----------------------|----------------------------------------------------------------|------------------------------------------|
| `backend/actix/`     | `POST /api/v1/todos` with `{"title":"Benchmark todo"}`         | tokio, one runtime per worker (actix-rt); sqlx on tokio |
| `backend/axum/`      | the same                                                       | tokio; sqlx on tokio                     |
| `backend/ntex/`      | the same                                                       | compio (epoll in Docker); sqlx on smol (async-io), no tokio |
| `backend/rocket/`    | the same                                                       | tokio, multi-threaded (Rocket 0.5); sqlx on tokio |
| `backend/salvo/`     | the same                                                       | tokio; sqlx on tokio                     |
| `fullstack/topcoat/` | `POST /_topcoat/runtime/procedures/add_todo` with `["Benchmark todo"]`, what its page sends | tokio; sqlx on tokio |

Machine: Apple M3 Pro (11 cores), Docker in OrbStack with 8 CPUs and 4.9 GiB.
oha ran on the same machine, so it competed with the apps for CPU. The
numbers compare the implementations with each other.

## Results: pool of 50

| Connections | Implementation | Req/sec | p50     | p95     | p99     | p99.9   | Slowest  |
|-------------|----------------|---------|---------|---------|---------|---------|----------|
| 1           | actix-web      | 1,595   | 0.56 ms | 0.83 ms | 2.1 ms  | 6.4 ms  | 22 ms    |
| 1           | axum           | 1,619   | 0.58 ms | 0.83 ms | 1.4 ms  | 3.3 ms  | 51 ms    |
| 1           | ntex           | 1,554   | 0.60 ms | 0.77 ms | 1.5 ms  | 5.4 ms  | 7.8 ms   |
| 1           | Rocket         | 1,408   | 0.56 ms | 0.78 ms | 3.0 ms  | 12 ms   | 729 ms   |
| 1           | Salvo          | 1,609   | 0.59 ms | 0.77 ms | 0.99 ms | 3.2 ms  | 11 ms    |
| 1           | Topcoat        | 1,750   | 0.53 ms | 0.75 ms | 1.2 ms  | 3.1 ms  | 27 ms    |
| 10          | actix-web      | 5,629   | 1.1 ms  | 4.0 ms  | 11 ms   | 50 ms   | 495 ms   |
| 10          | axum           | 8,241   | 1.0 ms  | 2.0 ms  | 3.5 ms  | 8.1 ms  | 226 ms   |
| 10          | ntex           | 8,004   | 1.0 ms  | 2.0 ms  | 4.2 ms  | 16 ms   | 212 ms   |
| 10          | Rocket         | 6,248   | 1.1 ms  | 4.3 ms  | 10 ms   | 27 ms   | 270 ms   |
| 10          | Salvo          | 5,095   | 1.3 ms  | 4.1 ms  | 13 ms   | 68 ms   | 337 ms   |
| 10          | Topcoat        | 7,851   | 1.0 ms  | 1.6 ms  | 7.7 ms  | 18 ms   | 369 ms   |
| 50          | actix-web      | 20,024  | 2.3 ms  | 3.8 ms  | 5.9 ms  | 15 ms   | 207 ms   |
| 50          | axum           | 18,297  | 2.6 ms  | 4.1 ms  | 5.1 ms  | 12 ms   | 216 ms   |
| 50          | ntex           | 17,382  | 2.4 ms  | 4.6 ms  | 11 ms   | 58 ms   | 203 ms   |
| 50          | Rocket         | 18,404  | 2.4 ms  | 3.9 ms  | 6.2 ms  | 35 ms   | 407 ms   |
| 50          | Salvo          | 11,940  | 2.8 ms  | 7.4 ms  | 36 ms   | 175 ms  | 276 ms   |
| 50          | Topcoat        | 19,648  | 2.4 ms  | 3.7 ms  | 4.7 ms  | 13 ms   | 207 ms   |
| 200         | actix-web      | 14,876  | 10 ms   | 40 ms   | 51 ms   | 191 ms  | 243 ms   |
| 200         | axum           | 16,418  | 11 ms   | 18 ms   | 45 ms   | 224 ms  | 245 ms   |
| 200         | ntex           | 11,282  | 11 ms   | 47 ms   | 97 ms   | 337 ms  | 1,021 ms |
| 200         | Rocket         | 16,021  | 11 ms   | 16 ms   | 23 ms   | 568 ms  | 3,016 ms |
| 200         | Salvo          | 16,169  | 11 ms   | 17 ms   | 49 ms   | 241 ms  | 1,027 ms |
| 200         | Topcoat        | 17,920  | 10 ms   | 15 ms   | 22 ms   | 209 ms  | 309 ms   |

## Notes

- **Variance:** single runs vary, and one step of a run can slow down for one
  app without a cause that could be found, as Salvo's 50-connection step did
  here. Compare implementations within one run, and repeat a run before
  reading anything into one bad number.
- **Slowest request:** the single slowest request of a step is an isolated
  stall, whose cause was not measured, and does not move the percentiles.
  Compare p99 and p99.9 instead.
- **Topcoat** answers 200 with an empty message where the backends answer 201
  with the created todo, so its responses are a little smaller.
- The throwaway stacks, their volumes and their images were removed after the
  run. oha's full JSON output is in
  `~/Proyek/todo-bench-results/2026-11-06-rocket/`, one folder per
  implementation, with the run's log.

# Benchmark

Adding a todo on PostgreSQL, measured on every implementation on 2026-11-06,
after the actix-web backend was added, with each app given a pool of 50
database connections. The runs of 2026-10-18 are kept under
[Earlier runs](#earlier-runs); they were made with more CPUs for Docker, so
compare the implementations within one run, not across runs.

- **actix-web** is level with axum and Salvo at 50 connections (about 20,700
  requests/sec, within 1%), and third at 200 (18,100, about 8% behind
  Salvo), with a p99.9 of 350 ms there. At 10 connections it was the slowest,
  4,900 against 5,600 to 8,400, with a p95 of 6.8 ms (see [Variance](#notes)).
- **Salvo** holds up best under load: the most requests at 200 connections
  (19,800) with the lowest p99 and p99.9, and the fastest on a single
  connection. Like actix, it had a weak step at 10 connections.
- **axum** is level at 50 connections and drops at 200 (15,300, p99.9 357 ms),
  as in the second and third runs of 2026-10-18.
- **ntex** is about 6% behind at 50 connections, close to Salvo at 200, and
  the fastest at 10 this time.
- **Topcoat** is about 10% behind at 50 connections and the slowest at 200
  (13,300). On 2026-10-18 it was level with Salvo.
- Every request succeeded: 201 from the backends, 200 from Topcoat.

With the apps' own pool of 10, they all stop at about 11,000 requests/sec,
because the pool, not the framework, is the limit: see
[With a pool of 10](#with-a-pool-of-10) (measured on 2026-10-18, without
Salvo and actix-web).

## How it was measured

```sh
./bench.sh all                    # pool of 50, the default
BENCH_DB_POOL=10 ./bench.sh all   # the apps' own pool size
```

[`bench.sh`](bench.sh) builds each app from the current source and starts it
in a throwaway copy of the compose stack (project `todo-bench`) with its own
PostgreSQL 17, then runs [oha](https://github.com/hatoo/oha) against it: a 5 s
warm-up at 50 connections, then 15 s at each of 1, 10, 50 and 200
connections, without Caddy in front. Before measuring, it checks that the
rows land in PostgreSQL, and after the warm-up it counts the open database
connections (44 to 50 of the 50 in this run).

| Implementation       | Request                                                        | Runtime                                  |
|----------------------|----------------------------------------------------------------|------------------------------------------|
| `backend/actix/`     | `POST /api/v1/todos` with `{"title":"Benchmark todo"}`         | tokio, one runtime per worker (actix-rt); sqlx on tokio |
| `backend/axum/`      | the same                                                       | tokio; sqlx on tokio                     |
| `backend/ntex/`      | the same                                                       | compio (epoll in Docker); sqlx on smol (async-io), no tokio |
| `backend/salvo/`     | the same                                                       | tokio; sqlx on tokio                     |
| `fullstack/topcoat/` | `POST /_topcoat/runtime/procedures/add_todo` with `["Benchmark todo"]`, what its page sends | tokio; sqlx on tokio |

Machine: Apple M3 Pro (11 cores), Docker in OrbStack with 8 CPUs and 4.9 GiB.
oha ran on the same machine, so it competed with the apps for CPU. The
numbers compare the implementations with each other.

## Results: pool of 50

| Connections | Implementation | Req/sec | p50     | p95     | p99     | p99.9   | Slowest  |
|-------------|----------------|---------|---------|---------|---------|---------|----------|
| 1           | actix-web      | 1,661   | 0.53 ms | 0.74 ms | 1.5 ms  | 7.2 ms  | 174 ms   |
| 1           | axum           | 1,456   | 0.56 ms | 0.97 ms | 3.1 ms  | 7.5 ms  | 166 ms   |
| 1           | ntex           | 1,533   | 0.62 ms | 0.76 ms | 1.0 ms  | 5.1 ms  | 6.8 ms   |
| 1           | Salvo          | 1,915   | 0.49 ms | 0.69 ms | 0.94 ms | 1.8 ms  | 28 ms    |
| 1           | Topcoat        | 1,618   | 0.54 ms | 1.3 ms  | 1.7 ms  | 3.2 ms  | 41 ms    |
| 10          | actix-web      | 4,930   | 1.0 ms  | 6.8 ms  | 13 ms   | 142 ms  | 426 ms   |
| 10          | axum           | 7,320   | 1.0 ms  | 2.9 ms  | 7.2 ms  | 15 ms   | 189 ms   |
| 10          | ntex           | 8,365   | 0.98 ms | 2.1 ms  | 3.9 ms  | 20 ms   | 134 ms   |
| 10          | Salvo          | 5,616   | 1.0 ms  | 5.2 ms  | 9.4 ms  | 32 ms   | 658 ms   |
| 10          | Topcoat        | 6,906   | 1.0 ms  | 4.0 ms  | 7.7 ms  | 28 ms   | 153 ms   |
| 50          | actix-web      | 20,737  | 2.1 ms  | 3.3 ms  | 6.4 ms  | 18 ms   | 532 ms   |
| 50          | axum           | 20,774  | 2.3 ms  | 3.3 ms  | 4.2 ms  | 8.9 ms  | 203 ms   |
| 50          | ntex           | 19,478  | 2.3 ms  | 3.6 ms  | 7.2 ms  | 28 ms   | 204 ms   |
| 50          | Salvo          | 20,533  | 2.3 ms  | 3.4 ms  | 4.3 ms  | 19 ms   | 204 ms   |
| 50          | Topcoat        | 18,745  | 2.3 ms  | 4.0 ms  | 8.2 ms  | 64 ms   | 207 ms   |
| 200         | actix-web      | 18,128  | 9.4 ms  | 14 ms   | 35 ms   | 347 ms  | 1,024 ms |
| 200         | axum           | 15,311  | 9.9 ms  | 27 ms   | 55 ms   | 357 ms  | 1,032 ms |
| 200         | ntex           | 18,237  | 10 ms   | 13 ms   | 17 ms   | 116 ms  | 1,017 ms |
| 200         | Salvo          | 19,808  | 9.8 ms  | 12 ms   | 17 ms   | 45 ms   | 215 ms   |
| 200         | Topcoat        | 13,347  | 11 ms   | 29 ms   | 63 ms   | 227 ms  | 289 ms   |

## Earlier runs

### Third run, 2026-10-18 (without actix-web)

The latest run before actix-web was added, on a Docker with 10 CPUs. Salvo
was in front from 10 connections on, Topcoat within 2% of it, ntex about 6%
and axum about 7% behind at 50 connections; axum fell to 15,500 at 200.

| Connections | Implementation | Req/sec | p50     | p95     | p99     | p99.9   | Slowest  |
|-------------|----------------|---------|---------|---------|---------|---------|----------|
| 1           | axum           | 1,267   | 0.53 ms | 1.7 ms  | 4.7 ms  | 35 ms   | 188 ms   |
| 1           | ntex           | 1,414   | 0.58 ms | 1.2 ms  | 3.1 ms  | 7.0 ms  | 64 ms    |
| 1           | Salvo          | 1,697   | 0.57 ms | 0.71 ms | 0.84 ms | 1.8 ms  | 30 ms    |
| 1           | Topcoat        | 1,745   | 0.55 ms | 0.74 ms | 1.0 ms  | 1.9 ms  | 25 ms    |
| 10          | axum           | 7,329   | 1.0 ms  | 2.0 ms  | 9.0 ms  | 32 ms   | 369 ms   |
| 10          | ntex           | 6,470   | 1.0 ms  | 4.3 ms  | 8.0 ms  | 46 ms   | 434 ms   |
| 10          | Salvo          | 9,202   | 0.97 ms | 1.5 ms  | 3.7 ms  | 8.8 ms  | 84 ms    |
| 10          | Topcoat        | 9,091   | 0.94 ms | 1.6 ms  | 4.4 ms  | 10 ms   | 125 ms   |
| 50          | axum           | 19,066  | 2.3 ms  | 3.8 ms  | 7.3 ms  | 44 ms   | 239 ms   |
| 50          | ntex           | 19,382  | 2.4 ms  | 3.6 ms  | 5.9 ms  | 38 ms   | 1,003 ms |
| 50          | Salvo          | 20,599  | 2.3 ms  | 3.4 ms  | 4.1 ms  | 14 ms   | 206 ms   |
| 50          | Topcoat        | 20,515  | 2.3 ms  | 3.4 ms  | 4.3 ms  | 14 ms   | 204 ms   |
| 200         | axum           | 15,543  | 9.8 ms  | 27 ms   | 55 ms   | 401 ms  | 1,017 ms |
| 200         | ntex           | 18,422  | 10 ms   | 13 ms   | 17 ms   | 76 ms   | 222 ms   |
| 200         | Salvo          | 19,711  | 9.7 ms  | 12 ms   | 15 ms   | 74 ms   | 1,018 ms |
| 200         | Topcoat        | 19,296  | 10 ms   | 13 ms   | 16 ms   | 57 ms   | 211 ms   |

### Second run, 2026-10-18

The same command, earlier on 2026-10-18, after Salvo was added. Its numbers
are 25 to 35% lower than the other two runs for every implementation. Salvo
was in front from 50 connections on, axum was level with it at 50 but fell to
11,800 at 200, and ntex and Topcoat were 10 to 13% behind Salvo at 50.

| Connections | Implementation | Req/sec | p50     | p95     | p99     | p99.9   | Slowest  |
|-------------|----------------|---------|---------|---------|---------|---------|----------|
| 1           | axum           | 1,263   | 0.76 ms | 1.0 ms  | 1.3 ms  | 2.9 ms  | 19 ms    |
| 1           | ntex           | 1,030   | 0.85 ms | 1.6 ms  | 2.6 ms  | 5.6 ms  | 37 ms    |
| 1           | Salvo          | 1,376   | 0.69 ms | 0.95 ms | 1.2 ms  | 2.5 ms  | 13 ms    |
| 1           | Topcoat        | 1,359   | 0.69 ms | 0.88 ms | 1.7 ms  | 3.1 ms  | 33 ms    |
| 10          | axum           | 6,516   | 1.3 ms  | 2.3 ms  | 7.6 ms  | 16 ms   | 77 ms    |
| 10          | ntex           | 7,338   | 1.3 ms  | 1.9 ms  | 2.4 ms  | 4.5 ms  | 12 ms    |
| 10          | Salvo          | 6,924   | 1.3 ms  | 2.0 ms  | 4.4 ms  | 6.2 ms  | 189 ms   |
| 10          | Topcoat        | 7,366   | 1.2 ms  | 1.9 ms  | 3.2 ms  | 7.9 ms  | 118 ms   |
| 50          | axum           | 16,236  | 2.9 ms  | 4.7 ms  | 6.4 ms  | 17 ms   | 207 ms   |
| 50          | ntex           | 15,205  | 3.0 ms  | 5.0 ms  | 6.9 ms  | 15 ms   | 204 ms   |
| 50          | Salvo          | 16,930  | 2.9 ms  | 4.1 ms  | 4.9 ms  | 13 ms   | 205 ms   |
| 50          | Topcoat        | 14,726  | 3.1 ms  | 5.2 ms  | 9.3 ms  | 23 ms   | 226 ms   |
| 200         | axum           | 11,788  | 13 ms   | 37 ms   | 59 ms   | 640 ms  | 1,039 ms |
| 200         | ntex           | 14,550  | 13 ms   | 17 ms   | 24 ms   | 44 ms   | 224 ms   |
| 200         | Salvo          | 15,864  | 12 ms   | 15 ms   | 19 ms   | 46 ms   | 223 ms   |
| 200         | Topcoat        | 14,981  | 13 ms   | 17 ms   | 24 ms   | 130 ms  | 1,030 ms |

### First run, 2026-10-18 (without Salvo)

The same command, earlier on 2026-10-18, before Salvo was added.

| Connections | Implementation | Req/sec | p50     | p95     | p99     | p99.9   | Slowest  |
|-------------|----------------|---------|---------|---------|---------|---------|----------|
| 1           | axum           | 2,007   | 0.47 ms | 0.63 ms | 0.72 ms | 1.4 ms  | 12 ms    |
| 1           | ntex           | 1,789   | 0.52 ms | 0.70 ms | 0.85 ms | 5.2 ms  | 12 ms    |
| 1           | Topcoat        | 2,055   | 0.46 ms | 0.61 ms | 0.68 ms | 1.3 ms  | 10 ms    |
| 10          | axum           | 8,370   | 0.91 ms | 2.0 ms  | 6.3 ms  | 10 ms   | 174 ms   |
| 10          | ntex           | 8,184   | 0.95 ms | 2.4 ms  | 5.9 ms  | 10 ms   | 113 ms   |
| 10          | Topcoat        | 10,462  | 0.89 ms | 1.3 ms  | 1.9 ms  | 5.2 ms  | 133 ms   |
| 50          | axum           | 21,798  | 2.2 ms  | 3.3 ms  | 4.1 ms  | 7.8 ms  | 204 ms   |
| 50          | ntex           | 17,635  | 2.6 ms  | 4.5 ms  | 5.8 ms  | 16 ms   | 206 ms   |
| 50          | Topcoat        | 20,746  | 2.3 ms  | 3.5 ms  | 4.4 ms  | 8.6 ms  | 204 ms   |
| 200         | axum           | 18,257  | 9.6 ms  | 18 ms   | 24 ms   | 135 ms  | 1,022 ms |
| 200         | ntex           | 12,854  | 13 ms   | 24 ms   | 111 ms  | 189 ms  | 618 ms   |
| 200         | Topcoat        | 15,885  | 9.8 ms  | 20 ms   | 27 ms   | 263 ms  | 1,017 ms |

#### Repeated

Single runs vary: the last step of a run has slowed down before without a
cause that could be found (see below). ntex and axum were run again at 50 and
200 connections only, on fresh stacks:

| Connections | Implementation | Req/sec | p50    | p95    | p99    | p99.9  | Slowest |
|-------------|----------------|---------|--------|--------|--------|--------|---------|
| 50          | axum           | 20,973  | 2.3 ms | 3.5 ms | 4.4 ms | 7.5 ms | 204 ms  |
| 50          | ntex           | 19,705  | 2.5 ms | 3.5 ms | 4.3 ms | 7.2 ms | 203 ms  |
| 200         | axum           | 19,002  | 9.6 ms | 17 ms  | 21 ms  | 137 ms | 236 ms  |
| 200         | ntex           | 18,022  | 11 ms  | 14 ms  | 19 ms  | 27 ms  | 216 ms  |

So the gap is about 5% when both are measured fresh, not the 20 to 30% of the
first run at 200 connections. The first table is kept as it was measured.

## With a pool of 10

The same code, `BENCH_DB_POOL=10`, in the first run of 2026-10-18 (Salvo and
actix-web not measured):

| Connections | axum    | ntex    | Topcoat |
|-------------|---------|---------|---------|
| 1           | 2,018   | 1,849   | 2,069   |
| 10          | 7,751   | 7,929   | 7,445   |
| 50          | 11,262  | 10,745  | 11,159  |
| 200         | 9,470   | 9,093   | 9,460   |

Requests/sec. From 50 connections on, every app waits for one of its 10
database connections, and they all measure the same.

## Notes

- **Why ntex is slower:** its database round trips take longer, not its HTTP
  handling. Measured in the first run, with both backends side by side in
  one stack, each on its own port, pool of 50, 50 connections:
  - Without the database (`GET /health/live`), ntex is as fast as axum or
    faster: 60,500 against 57,700 requests/sec.
  - Adding todos, ntex's 10 workers sit at 9 to 15% CPU each, axum's at 17 to
    18%, and PostgreSQL is less busy under ntex (125% against 153% CPU). ntex
    is waiting, not working.
  - All of its database sockets are watched by one thread, smol's executor
    (`smol-1`, 24% CPU, so not saturated), which wakes the ntex worker for
    each reply. On tokio, the thread that receives a reply carries on with
    the request. The extra hand-off is about 0.05 ms per request on one
    connection, and about 0.45 ms at 50 connections, where requests queue
    for that thread.
  - More requests in flight hide it: at 200 connections the gap is about 5%.
- **Variance:** the last step of a run sometimes slows down for one app: ntex
  at 200 connections in the first run, axum at 200 in the second and third,
  and on 2026-11-06 actix-web and Salvo had a weak step at 10 connections
  (Salvo was first at 10 on 2026-10-18).
  Absolute numbers also move between runs (the second run is 25 to 35% lower
  than the other two). Compare implementations within one run, and repeat a run
  before reading anything into one bad number.
- **Slowest request:** the single slowest request of a run (up to about
  1 second) is an isolated stall, whose cause was not measured, and does not
  move the percentiles. Compare p99 and p99.9 instead.
- **Topcoat** answers 200 with an empty message where the backends answer 201
  with the created todo, so its responses are a little smaller.
- The throwaway stacks, their volumes and their images were removed after
  each run. oha's full JSON output of the first run was kept outside the
  repository; that of the second run was not kept, and that of the third was
  left in temporary folders outside the repository. Those of 2026-11-06 are
  in `~/Proyek/todo-bench-results/2026-11-06/`, one folder per
  implementation, with the run's log.

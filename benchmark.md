# Benchmark

Adding a todo on PostgreSQL, measured on every implementation on 2026-10-18,
with each app given a pool of 50 database connections. This is the third run
of the day; the two before it are kept under [Earlier runs](#earlier-runs).

- **Salvo** is in front from 10 connections on: about 20,600 requests/sec at
  50 connections and 19,700 at 200, with the lowest p99 at both, as in the
  second run.
- **Topcoat** is level with Salvo, within 2% at every step, and the fastest
  on a single connection.
- **ntex** is about 6% behind Salvo at 50 and 200 connections, and the
  slowest at 10.
- **axum** is about 7% behind Salvo at 50 connections and fell to 15,500 at
  200, with a p99.9 of 400 ms. It dropped at 200 in the second run too, but
  not in the first run or its repeat (see [Variance](#notes)).
- Every request succeeded: 201 from the backends, 200 from Topcoat.

This run's numbers are close to the first run's. The second run was 25 to 35%
lower on the same machine, so compare the implementations within one run, not
across runs.

With the apps' own pool of 10, they all stop at about 11,000 requests/sec,
because the pool, not the framework, is the limit: see
[With a pool of 10](#with-a-pool-of-10).

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
connections (45 to 50 of the 50 in every run).

| Implementation       | Request                                                        | Runtime                                  |
|----------------------|----------------------------------------------------------------|------------------------------------------|
| `backend/axum/`      | `POST /api/v1/todos` with `{"title":"Benchmark todo"}`         | tokio; sqlx on tokio                     |
| `backend/ntex/`      | the same                                                       | compio (epoll in Docker); sqlx on smol (async-io), no tokio |
| `backend/salvo/`     | the same                                                       | tokio; sqlx on tokio                     |
| `fullstack/topcoat/` | `POST /_topcoat/runtime/procedures/add_todo` with `["Benchmark todo"]`, what its page sends | tokio; sqlx on tokio |

Machine: Apple M5 (10 cores), Docker in OrbStack with 10 CPUs and 4.9 GiB.
oha ran on the same machine, so it competed with the apps for CPU. The
numbers compare the implementations with each other.

## Results: pool of 50

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

## Earlier runs

### Second run

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

### First run (without Salvo)

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

The same code, `BENCH_DB_POOL=10`, in the first run (Salvo not measured):

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
  at 200 connections in the first run, axum at 200 in the second and third.
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
  left in temporary folders outside the repository.

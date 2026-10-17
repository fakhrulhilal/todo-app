`POST /api/v1/todos` on Postgres peaks at about 8,900 requests/sec with sub-millisecond median latency, and every request in the final runs returned 201. One thing needs your attention first: your real `todo-dx` stack is no longer running.

## Your stack is down

The `todo-dx` containers and network disappeared at some point during the session; they were last seen running shortly before the rebuild. Every compose command I ran was scoped to `-p todo-bench`, and the Docker event log doesn't reach back far enough to show what removed them. So I can't tell you the cause — possibly you took it down yourself.

- **Data:** the `todo-dx_db-data` volume (your 2 todos) still exists, though I couldn't re-count the rows with the database down.
- **Restarting:** I left it down. Use `docker compose up -d --build`, not a plain `up -d`. The existing `todo-dx-api:latest` image still reads `TODO_DATABASE__URL`, while `compose.yaml` now sets `DATABASE__URL`. Without a rebuild the API silently ignores Postgres and writes to embedded SQLite inside the container.

## Results

Each run was 15 s after a 5 s warm-up, with the body `{"title":"Benchmark todo"}`, on an M3 Pro (11 cores) under OrbStack. The API was built from current source and backed by Postgres 17, with the default pool of 10 connections.

| Connections | Path      | Req/sec | p50     | p95     | p99    | p99.9   | Slowest |
| ----------- | --------- | ------- | ------- | ------- | ------ | ------- | ------- |
| 1           | direct    | 1,548   | 0.58 ms | 0.78 ms | 1.4 ms | 9.4 ms  | 61 ms   |
| 10          | direct    | 8,860   | 0.96 ms | 1.8 ms  | 3.8 ms | 10.7 ms | 118 ms  |
| 50          | direct    | 6,773   | 5.2 ms  | 15.8 ms | 41 ms  | 124 ms  | 329 ms  |
| 200         | direct    | 7,931   | 21 ms   | 45 ms   | 65 ms  | 107 ms  | 227 ms  |
| 50          | via Caddy | 6,821   | 5.8 ms  | 16.9 ms | 22 ms  | 77 ms   | 92 ms   |

- **Saturation:** throughput tops out at 10 connections, which matches the pool size. Beyond that, extra connections only queue, so latency rises while throughput stays around 7–8k.
- **Caddy:** going through the proxy costs nothing measurable at 50 connections.
- **Caveats:** `oha` ran on the same machine as the server, so they competed for CPU. The table grew to about 528k rows over the runs with no visible slowdown.

## SQLite fallback numbers

My first round accidentally measured the stale image on embedded SQLite, because of the env-var mismatch above. It managed 930 req/sec at 1 connection and 590–690 at 10–200. p99 was 1.9–3.1 s at 50–200 connections, and there were 7 HTTP 500s. I didn't capture the cause of the 500s before removing that stack. These numbers come from the old binary, so treat them as a rough indication only.

## What I ran against

I benchmarked a throwaway copy of the stack (project `todo-bench`, ports 18080/18081) so the load wouldn't write into your database. That stack, its volume and the `todo-bench-api` image are removed. The raw `oha` output is in the session scratchpad if you want the full histograms.

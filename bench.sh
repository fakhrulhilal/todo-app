#!/usr/bin/env bash
# Benchmarks adding a todo with oha.
#
#   ./bench.sh <name>   one implementation: a folder under backend/ or fullstack/
#   ./bench.sh all      every one of them, in turn
#
# A backend is measured on POST /api/v1/todos. A fullstack app has no JSON API,
# so it is measured on the request its page sends to add a todo (for Topcoat,
# the add_todo procedure).
#
# Runs against a throwaway copy of the stack (compose project "todo-bench":
# the chosen app built from the current source plus its own PostgreSQL), so
# nothing is written to the real database. The copy is removed when the
# script ends.
#
# Settings (environment):
#   BENCH_CONNECTIONS  concurrency levels to run, default "1 10 50 200"
#   BENCH_DURATION     length of each run, default 15s
#   BENCH_WARMUP       length of the unmeasured warm-up, default 5s
#   BENCH_PORT         host port the app is published on, default 18080
#   BENCH_DB_POOL      the app's PostgreSQL pool size (database.max_connections),
#                      default 50. With the app's own default of 10 the pool is
#                      the limit and every app measures the same; keep it under
#                      PostgreSQL's max_connections (100)
#   BENCH_OUT          directory for oha's full JSON results, default a temp dir
#                      (with `all`, one subdirectory per implementation)
#   BENCH_KEEP         set to 1 to leave the stack running afterwards
#
# Needs docker, oha and jq.

set -euo pipefail
cd "$(dirname "$0")"

name=${1:?usage: ./bench.sh <name>|all}

if [ "$name" = all ]; then
  for dir in backend/*/ fullstack/*/; do
    [ -f "$dir/Dockerfile" ] || continue
    app=$(basename "$dir")
    printf '\nbench: === %s\n' "${dir%/}"
    BENCH_OUT=${BENCH_OUT:+$BENCH_OUT/$app} "$0" "$app"
  done
  exit
fi

# The compose service, and the request that adds one todo.
if [ -f "backend/$name/Dockerfile" ]; then
  service=backend-$name
  path=/api/v1/todos
  body='{"title":"Benchmark todo"}'
elif [ -f "fullstack/$name/Dockerfile" ]; then
  service=fullstack-$name
  case $name in
    topcoat)
      path=/_topcoat/runtime/procedures/add_todo
      body='["Benchmark todo"]'
      ;;
    *)
      echo "bench: no request to add a todo is known for fullstack/$name" >&2
      exit 1
      ;;
  esac
else
  echo "bench: there is no backend/$name or fullstack/$name" >&2
  exit 1
fi

connections=${BENCH_CONNECTIONS:-"1 10 50 200"}
duration=${BENCH_DURATION:-15s}
warmup=${BENCH_WARMUP:-5s}
port=${BENCH_PORT:-18080}
pool=${BENCH_DB_POOL:-50}
out=${BENCH_OUT:-$(mktemp -d)}
url="http://127.0.0.1:$port$path"

for tool in docker oha jq; do
  command -v "$tool" >/dev/null || { echo "bench: $tool is not installed" >&2; exit 1; }
done
mkdir -p "$out"

# compose.yaml plus what the benchmark changes: an image tag of its own, so
# the real one is not replaced, and the app published without Caddy in front.
compose() {
  docker compose --progress quiet -p todo-bench -f compose.yaml -f - "$@" <<EOF
services:
  $service:
    # Names of their own, so the real stack can keep running.
    container_name: !reset null
    image: todo-bench-$name:latest
    labels: !reset {}
    ports: !override
      - "$port:8080"
    environment:
      DATABASE__MAX_CONNECTIONS: "$pool"
  db:
    container_name: !reset null
  db-plan-init:
    container_name: !reset null
  db-init:
    container_name: !reset null
EOF
}

cleanup() {
  if [ "${BENCH_KEEP:-0}" = 1 ]; then
    echo "bench: stack left running, remove it with: docker compose -p todo-bench down -v"
  else
    compose down -v --rmi local >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

echo "bench: building and starting the stack with $service, database pool of $pool"
compose up -d --build --wait "$service" db

load() { # connections duration output
  oha --no-tui -c "$1" -z "$2" -m POST \
    -H 'content-type: application/json' -d "$body" \
    --output-format json -o "$3" "$url"
}

# The store must be PostgreSQL: one request, then look for its row.
load 1 1s "$out/check.json"
rows=$(compose exec -T db psql -U todo -d todo -Atc 'select count(*) from todos')
[ "$rows" -gt 0 ] || { echo "bench: $service is not writing to PostgreSQL" >&2; exit 1; }

echo "bench: warming up for $warmup"
load 50 "$warmup" "$out/warmup.json"
# The pool really is that large: the app's connections, now open (psql's own excluded).
open=$(compose exec -T db psql -U todo -d todo -Atc \
  "select count(*) from pg_stat_activity where datname = 'todo' and pid <> pg_backend_pid()")
echo "bench: $open database connections open after the warm-up (pool of $pool)"

printf '\n%11s %9s %9s %9s %9s %9s %9s  %s\n' \
  connections req/sec p50 p95 p99 p99.9 slowest statuses
for c in $connections; do
  load "$c" "$duration" "$out/c$c.json"
  jq -r --arg c "$c" '
    def ms: . * 100000 | round / 100;
    [ $c,
      (.summary.requestsPerSec | round),
      (.latencyPercentiles | .p50, .p95, .p99, .["p99.9"] | ms),
      (.summary.slowest | ms),
      (.statusCodeDistribution | to_entries | map("\(.key)x\(.value)") | join(" "))
    ] | @tsv' "$out/c$c.json" |
    awk -F'\t' '{ printf "%11s %9s %7sms %7sms %7sms %7sms %7sms  %s\n", $1, $2, $3, $4, $5, $6, $7, $8 }'
done

printf '\nbench: full results in %s\n' "$out"

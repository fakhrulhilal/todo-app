#!/usr/bin/env bash
# Benchmarks POST /api/v1/todos with oha.
#
#   ./bench.sh
#
# Runs against a throwaway copy of the stack (compose project "todo-bench":
# the API built from the current source plus its own PostgreSQL), so nothing
# is written to the real database. The copy is removed when the script ends.
#
# Settings (environment):
#   BENCH_CONNECTIONS  concurrency levels to run, default "1 10 50 200"
#   BENCH_DURATION     length of each run, default 15s
#   BENCH_WARMUP       length of the unmeasured warm-up, default 5s
#   BENCH_PORT         host port the API is published on, default 18080
#   BENCH_OUT          directory for oha's full JSON results, default a temp dir
#   BENCH_KEEP         set to 1 to leave the stack running afterwards
#
# Needs docker, oha and jq.

set -euo pipefail
cd "$(dirname "$0")"

connections=${BENCH_CONNECTIONS:-"1 10 50 200"}
duration=${BENCH_DURATION:-15s}
warmup=${BENCH_WARMUP:-5s}
port=${BENCH_PORT:-18080}
out=${BENCH_OUT:-$(mktemp -d)}
url="http://127.0.0.1:$port/api/v1/todos"

for tool in docker oha jq; do
  command -v "$tool" >/dev/null || { echo "bench: $tool is not installed" >&2; exit 1; }
done
mkdir -p "$out"

# compose.yaml plus what the benchmark changes: an image tag of its own, so
# the real one is not replaced, and the API published without Caddy in front.
compose() {
  docker compose --progress quiet -p todo-bench -f compose.yaml -f - "$@" <<EOF
services:
  api:
    image: todo-bench-api:latest
    labels: !reset {}
    ports:
      - "$port:8080"
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

echo "bench: building and starting the stack"
compose up -d --build --wait api db

load() { # connections duration output
  oha --no-tui -c "$1" -z "$2" -m POST \
    -H 'content-type: application/json' -d '{"title":"Benchmark todo"}' \
    --output-format json -o "$3" "$url"
}

# The store must be PostgreSQL: one request, then look for its row.
load 1 1s "$out/check.json"
rows=$(compose exec -T db psql -U todo -d todo -Atc 'select count(*) from todos')
[ "$rows" -gt 0 ] || { echo "bench: the API is not writing to PostgreSQL" >&2; exit 1; }

echo "bench: warming up for $warmup"
load 50 "$warmup" "$out/warmup.json"

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

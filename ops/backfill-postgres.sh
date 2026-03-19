#!/usr/bin/env bash
set -euo pipefail
unset LC_ALL

usage() {
  cat >&2 <<'EOF'
usage: backfill-postgres.sh <env-file> <source> [options]

sources:
  binance   run postgres_kline_backfill
  fx        run postgres_fx_kline_backfill
  kr        run postgres_kr_exchange_kline_backfill
  all       run all three in sequence

examples:
  bash ops/backfill-postgres.sh ops/grafana/.env binance
  bash ops/backfill-postgres.sh ops/grafana/.env fx
  bash ops/backfill-postgres.sh ops/grafana/.env kr
  bash ops/backfill-postgres.sh ops/grafana/.env all
EOF
}

if [ "$#" -lt 2 ]; then
  usage
  exit 64
fi

env_file=$1
source_name=$2
shift 2

if [ ! -f "$env_file" ]; then
  echo "env file not found: $env_file" >&2
  exit 66
fi

set -a
# shellcheck disable=SC1090
. "$env_file"
set +a

export SANDBOX_QUANT_POSTGRES_URL="postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@127.0.0.1:${POSTGRES_PORT:-5432}/${POSTGRES_DB}"

run_bin() {
  local bin_name=$1
  shift
  cargo run --bin "$bin_name" -- "$@"
}

case "$source_name" in
  binance)
    run_bin postgres_kline_backfill "$@"
    ;;
  fx)
    run_bin postgres_fx_kline_backfill "$@"
    ;;
  kr)
    run_bin postgres_kr_exchange_kline_backfill "$@"
    ;;
  all)
    run_bin postgres_kline_backfill "$@"
    run_bin postgres_fx_kline_backfill "$@"
    run_bin postgres_kr_exchange_kline_backfill "$@"
    ;;
  *)
    echo "unsupported source: $source_name" >&2
    usage
    exit 64
    ;;
esac

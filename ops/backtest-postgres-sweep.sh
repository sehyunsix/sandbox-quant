#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GRAFANA_ENV_FILE="${GRAFANA_ENV_FILE:-$ROOT_DIR/ops/grafana/.env.example}"
BACKTEST_BIN="${BACKTEST_BIN:-$ROOT_DIR/target/debug/sandbox-quant-backtest}"
MODE="${MODE:-demo}"
BASE_DIR="${BASE_DIR:-var}"
INSTRUMENTS_CSV="${INSTRUMENTS_CSV:-BTCUSDT,ETHUSDT}"
TEMPLATES_CSV="${TEMPLATES_CSV:-price-sma-cross-long,price-sma-cross-short,price-sma-cross-long-fast,price-sma-cross-short-fast}"
DATE_WINDOWS_CSV="${DATE_WINDOWS_CSV:-2026-03-01:2026-03-15,2026-03-16:2026-03-18}"
OUTPUT_DIR="${OUTPUT_DIR:-$ROOT_DIR/var/backtest-sweeps}"

load_env_file() {
  local file="$1"
  local line name value
  [[ -f "$file" ]] || return 0
  while IFS= read -r line || [[ -n "$line" ]]; do
    [[ -n "$line" ]] || continue
    [[ "$line" =~ ^[[:space:]]*# ]] && continue
    [[ "$line" == *=* ]] || continue
    name="${line%%=*}"
    value="${line#*=}"
    name="${name#"${name%%[![:space:]]*}"}"
    name="${name%"${name##*[![:space:]]}"}"
    if [[ "$name" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]]; then
      export "$name=$value"
    fi
  done < "$file"
}

ensure_postgres_url() {
  if [[ -n "${SANDBOX_QUANT_POSTGRES_URL:-}" ]]; then
    return 0
  fi
  load_env_file "$GRAFANA_ENV_FILE"
  if [[ -z "${POSTGRES_USER:-}" || -z "${POSTGRES_PASSWORD:-}" || -z "${POSTGRES_DB:-}" ]]; then
    echo "missing PostgreSQL credentials; set SANDBOX_QUANT_POSTGRES_URL or prepare ops/grafana/.env.example" >&2
    exit 2
  fi
  export SANDBOX_QUANT_POSTGRES_URL="postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@localhost:${POSTGRES_PORT:-5432}/${POSTGRES_DB}"
}

ensure_backtest_bin() {
  if [[ -x "$BACKTEST_BIN" ]]; then
    return 0
  fi
  (cd "$ROOT_DIR" && cargo build -q --bin sandbox-quant-backtest)
}

ensure_postgres_url
ensure_backtest_bin

export SANDBOX_QUANT_BACKTEST_SOURCE="${SANDBOX_QUANT_BACKTEST_SOURCE:-postgres}"
export SANDBOX_QUANT_BACKTEST_EXPORT_POSTGRES=1

exec "$BACKTEST_BIN" sweep \
  --templates "$TEMPLATES_CSV" \
  --instruments "$INSTRUMENTS_CSV" \
  --windows "$DATE_WINDOWS_CSV" \
  --output-dir "$OUTPUT_DIR" \
  --mode "$MODE" \
  --base-dir "$BASE_DIR"

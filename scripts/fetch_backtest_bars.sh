#!/usr/bin/env bash
set -euo pipefail

if ! command -v curl >/dev/null 2>&1; then
  echo "curl is required but not installed" >&2
  exit 1
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required but not installed" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

SYMBOL="BTCUSDT"
INTERVAL="1m"
OUTPUT="$ROOT_DIR/data/backtest_${SYMBOL,,}_${INTERVAL}.csv"
LIMIT_BARS=10000
DAYS_BACK=14
INTERVAL_MS=60000
BASE_URL="https://api.binance.com"
REQUEST_TIMEOUT=20
MAX_RETRIES=3
SLEEP_SECONDS=0.25
VERBOSE=0

usage() {
  cat <<'USAGE'
Usage:
  scripts/fetch_backtest_bars.sh [--symbol BTCUSDT] [--interval 1m] [--days-back 14]
                              [--bars 10000] [--out data/backtest_BTCUSDT_1m.csv]
                              [--start-ms unix_ms] [--end-ms unix_ms]
                              [--timeout-seconds 20] [--max-retries 3]
                              [--sleep-seconds 0.25] [--verbose]

Arguments:
  --symbol     Symbol (default: BTCUSDT)
  --interval   Binance interval e.g. 1m, 5m, 1h, 1d (default: 1m)
  --days-back  Relative lookback from end time (default: 14)
  --bars       Maximum bars to fetch (default: 10000)
  --out        Output CSV path
  --start-ms   Start timestamp in ms (overrides --days-back)
  --end-ms     End timestamp in ms (default: now)
  --timeout-seconds   curl timeout seconds (default: 20)
  --max-retries       request retry count (default: 3)
  --sleep-seconds     sleep between batches (default: 0.25)
  --verbose           print fetch progress
USAGE
}

logf() {
  if (( VERBOSE == 1 )); then
    echo "[$(date -u +'%Y-%m-%dT%H:%M:%SZ')] $*"
  fi
}

parse_non_negative_int() {
  if [[ ! "$1" =~ ^[0-9]+$ ]]; then
    return 1
  fi
}

parse_non_negative_number() {
  if [[ ! "$1" =~ ^[0-9]+([.][0-9]+)?$ ]]; then
    return 1
  fi
}

while (($# > 0)); do
  case "$1" in
    --symbol)
      SYMBOL="$2"
      OUTPUT="$ROOT_DIR/data/backtest_${SYMBOL,,}_${INTERVAL}.csv"
      shift 2
      ;;
    --interval)
      INTERVAL="$2"
      OUTPUT="$ROOT_DIR/data/backtest_${SYMBOL,,}_${INTERVAL}.csv"
      shift 2
      ;;
    --bars)
      parse_non_negative_int "$2" || { echo "--bars requires integer >= 0: $2" >&2; exit 1; }
      LIMIT_BARS="$2"
      shift 2
      ;;
    --days-back)
      parse_non_negative_int "$2" || { echo "--days-back requires integer >= 0: $2" >&2; exit 1; }
      DAYS_BACK="$2"
      shift 2
      ;;
    --out)
      OUTPUT="$2"
      shift 2
      ;;
    --start-ms)
      parse_non_negative_int "$2" || { echo "--start-ms requires unix ms integer: $2" >&2; exit 1; }
      START_MS_OVERRIDE="$2"
      shift 2
      ;;
    --end-ms)
      parse_non_negative_int "$2" || { echo "--end-ms requires unix ms integer: $2" >&2; exit 1; }
      END_MS_OVERRIDE="$2"
      shift 2
      ;;
    --timeout-seconds)
      parse_non_negative_number "$2" || { echo "--timeout-seconds requires number >= 0: $2" >&2; exit 1; }
      REQUEST_TIMEOUT="$2"
      shift 2
      ;;
    --max-retries)
      parse_non_negative_int "$2" || { echo "--max-retries requires integer >= 0: $2" >&2; exit 1; }
      MAX_RETRIES="$2"
      shift 2
      ;;
    --sleep-seconds)
      parse_non_negative_number "$2" || { echo "--sleep-seconds requires number >= 0: $2" >&2; exit 1; }
      SLEEP_SECONDS="$2"
      shift 2
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    --verbose)
      VERBOSE=1
      shift
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage
      exit 1
      ;;
  esac
done

case "$INTERVAL" in
  1s) INTERVAL_MS=1000 ;;
  1m) INTERVAL_MS=60000 ;;
  3m) INTERVAL_MS=180000 ;;
  5m) INTERVAL_MS=300000 ;;
  15m) INTERVAL_MS=900000 ;;
  30m) INTERVAL_MS=1800000 ;;
  1h) INTERVAL_MS=3600000 ;;
  2h) INTERVAL_MS=7200000 ;;
  4h) INTERVAL_MS=14400000 ;;
  6h) INTERVAL_MS=21600000 ;;
  8h) INTERVAL_MS=28800000 ;;
  12h) INTERVAL_MS=43200000 ;;
  1d) INTERVAL_MS=86400000 ;;
  3d) INTERVAL_MS=259200000 ;;
  1w) INTERVAL_MS=604800000 ;;
  1M) INTERVAL_MS=2592000000 ;;
  *)
    echo "unsupported interval: $INTERVAL" >&2
    exit 1
    ;;
esac

if [[ ! "${SYMBOL}" =~ ^[A-Z0-9]{3,12}$ ]]; then
  echo "symbol must be 3-12 uppercase alphanum characters: $SYMBOL" >&2
  exit 1
fi

if [[ "${LIMIT_BARS}" -eq 0 ]]; then
  echo "nothing to fetch (--bars 0)"
  exit 0
fi

if [[ -z "${START_MS_OVERRIDE:-}" ]]; then
  if [[ -z "${END_MS_OVERRIDE:-}" ]]; then
    END_MS="$(date +%s)000"
  else
    END_MS="$END_MS_OVERRIDE"
  fi
  START_MS=$((END_MS - DAYS_BACK * 24 * 3600 * 1000))
else
  START_MS="$START_MS_OVERRIDE"
  if [[ -z "${END_MS_OVERRIDE:-}" ]]; then
    END_MS="$(date +%s)000"
  else
    END_MS="$END_MS_OVERRIDE"
  fi
fi

if [[ "$END_MS" -le "$START_MS" ]]; then
  echo "end-ms must be greater than start-ms" >&2
  exit 1
fi

TMP_FILE="$(mktemp)"
trap 'rm -f "$TMP_FILE"' EXIT
touch "$TMP_FILE"
echo "open_time,open,high,low,close" > "$TMP_FILE"

fetch_with_retries() {
  local request_url="$1"
  local attempt=1
  local http_code=""
  local response_file
  local response_body

  while (( attempt <= MAX_RETRIES )); do
    response_file="$(mktemp)"
    http_code="$(curl --max-time "$REQUEST_TIMEOUT" -fsSL -w "%{http_code}" -o "$response_file" "$request_url" || true)"
    response_body="$(cat "$response_file")"
    rm -f "$response_file"

    if [[ "$http_code" == "200" ]]; then
      if jq -e 'type == "array"' <<<"$response_body" >/dev/null 2>&1; then
        echo "$response_body"
        return 0
      fi

      if jq -e 'type == "object" and has("msg")' <<<"$response_body" >/dev/null 2>&1; then
        echo "request failed with structured api error: $response_body" >&2
        return 1
      fi

      logf "request failed with unexpected response: ${response_body:0:200}"
    else
      logf "request failed: attempt=$attempt status=$http_code"
    fi

    if (( attempt >= MAX_RETRIES )); then
      break
    fi

    ((attempt += 1))
    sleep 1
  done

  return 1
}

fetched=0
cursor="$START_MS"
while true; do
  remaining=$((LIMIT_BARS - fetched))
  [[ $remaining -le 0 ]] && break
  batch_limit=1000
  if (( remaining < batch_limit )); then
    batch_limit=$remaining
  fi

  url="${BASE_URL}/api/v3/klines?symbol=${SYMBOL}&interval=${INTERVAL}&startTime=${cursor}&endTime=${END_MS}&limit=${batch_limit}"
  if ! payload="$(fetch_with_retries "$url")"; then
    echo "failed to fetch after retries: $url" >&2
    exit 1
  fi

  row_count="$(jq 'length' <<<"$payload" 2>/dev/null || true)"
  if [[ -z "$row_count" || ! "$row_count" =~ ^[0-9]+$ ]]; then
    echo "invalid row count in response from: $url" >&2
    exit 1
  fi
  if [[ "$row_count" -eq 0 ]]; then
    break
  fi

  tail_time=""
  while read -r row; do
    open_time="$(jq -r '.[0]' <<<"$row")"
    open="$(jq -r '.[1]' <<<"$row")"
    high="$(jq -r '.[2]' <<<"$row")"
    low="$(jq -r '.[3]' <<<"$row")"
    close="$(jq -r '.[4]' <<<"$row")"
    echo "${open_time},${open},${high},${low},${close}" >> "$TMP_FILE"
    fetched=$((fetched + 1))
    tail_time=$open_time
  done < <(jq -c '.[]' <<<"$payload")
  logf "fetched ${fetched}/${LIMIT_BARS} bars (batch=${batch_limit}, rows=${row_count}) current_cursor=${cursor}"

  if [[ "$row_count" -lt "$batch_limit" ]]; then
    break
  fi
  if [[ -z "$tail_time" ]]; then
    break
  fi
  cursor=$((tail_time + INTERVAL_MS))
  sleep "$SLEEP_SECONDS"
done

if [[ "$fetched" -eq 0 ]]; then
  echo "no bars fetched; check symbol/interval/end-time/network" >&2
  exit 1
fi

mv "$TMP_FILE" "$OUTPUT"
trap - EXIT
echo "wrote $fetched bars to $OUTPUT"
echo "columns: open_time,open,high,low,close"

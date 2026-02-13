#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BACKLOG_FILE="$ROOT_DIR/data/demo_market_backlog.csv"
REGISTRY_FILE="$ROOT_DIR/data/demo_market_registry.csv"
EXPECTED_HEADER="exchange,environment,asset_class,product,api_base_url,notes"

for file in "$BACKLOG_FILE" "$REGISTRY_FILE"; do
  if [[ ! -f "$file" ]]; then
    echo "[ERROR] missing file: $file" >&2
    exit 1
  fi

  header="$(head -n1 "$file")"
  if [[ "$header" != "$EXPECTED_HEADER" ]]; then
    echo "[ERROR] invalid header in $file" >&2
    echo "        expected: $EXPECTED_HEADER" >&2
    echo "        actual:   $header" >&2
    exit 1
  fi

  line_no=1
  tail -n +2 "$file" | while IFS=, read -r exchange env asset product base notes extra; do
    line_no=$((line_no + 1))

    if [[ -n "${extra:-}" ]]; then
      echo "[ERROR] $file:$line_no has more than 6 columns" >&2
      exit 1
    fi

    for value in "$exchange" "$env" "$asset" "$product" "$base" "$notes"; do
      if [[ -z "$value" ]]; then
        echo "[ERROR] $file:$line_no has empty required fields" >&2
        exit 1
      fi
    done

    if [[ ! "$base" =~ ^https?:// ]]; then
      if [[ "$base" != "https://localhost" && "$base" != "http://localhost" && "$base" != "localhost" ]]; then
        echo "[ERROR] $file:$line_no has invalid api_base_url: $base" >&2
        exit 1
      fi
    fi
  done

done

# Duplicate full-line rows are not allowed in each file.
if [[ -n "$(tail -n +2 "$BACKLOG_FILE" | sort | uniq -d)" ]]; then
  echo "[ERROR] duplicate rows found in backlog" >&2
  exit 1
fi

if [[ -n "$(tail -n +2 "$REGISTRY_FILE" | sort | uniq -d)" ]]; then
  echo "[ERROR] duplicate rows found in registry" >&2
  exit 1
fi

# Same (exchange, environment, asset_class, product) key should not appear twice in backlog.
if [[ -n "$(tail -n +2 "$BACKLOG_FILE" | awk -F, '{print $1","$2","$3","$4}' | sort | uniq -d)" ]]; then
  echo "[ERROR] duplicate logical product keys found in backlog" >&2
  exit 1
fi

echo "catalog validation passed"

#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REGISTRY_FILE="$ROOT_DIR/data/demo_market_registry.csv"
BACKLOG_FILE="$ROOT_DIR/data/demo_market_backlog.csv"
DOC_FILE="$ROOT_DIR/docs/hourly-market-catalog.md"

if [[ ! -f "$REGISTRY_FILE" ]]; then
  echo "exchange,environment,asset_class,product,api_base_url,notes" > "$REGISTRY_FILE"
fi

if [[ ! -f "$BACKLOG_FILE" ]]; then
  echo "missing backlog file: $BACKLOG_FILE" >&2
  exit 1
fi

next_line=""
while IFS= read -r line; do
  [[ -z "$line" ]] && continue
  if ! grep -Fqx "$line" "$REGISTRY_FILE"; then
    next_line="$line"
    break
  fi
done < <(tail -n +2 "$BACKLOG_FILE")

added="false"
if [[ -n "$next_line" ]]; then
  echo "$next_line" >> "$REGISTRY_FILE"
  added="true"
fi

if [[ "$added" == "true" || ! -f "$DOC_FILE" ]]; then
  {
    echo "# Hourly Market Catalog"
    echo
    printf '%s\n' 'Auto-generated from `data/demo_market_registry.csv`.'
    echo
    echo "| Exchange | Environment | Asset Class | Product | API Base URL | Notes |"
    echo "|---|---|---|---|---|---|"
    tail -n +2 "$REGISTRY_FILE" | while IFS=, read -r exchange env asset product base notes; do
      printf '| %s | %s | %s | %s | %s | %s |\n' "$exchange" "$env" "$asset" "$product" "$base" "$notes"
    done
  } > "$DOC_FILE"
fi

if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
  {
    echo "added=$added"
    if [[ -n "$next_line" ]]; then
      echo "entry=$next_line"
    fi
  } >> "$GITHUB_OUTPUT"
fi

echo "added=$added"
if [[ -n "$next_line" ]]; then
  echo "entry=$next_line"
fi

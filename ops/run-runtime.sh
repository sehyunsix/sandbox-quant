#!/usr/bin/env bash
set -euo pipefail
unset LC_ALL

usage() {
  cat >&2 <<'EOF'
usage: run-runtime.sh <env-file> <service>

services are defined in ops/runtime.yaml

examples:
  bash ops/run-runtime.sh ops/grafana/.env recorder
  bash ops/run-runtime.sh ops/grafana/.env trading_engine
  bash ops/run-runtime.sh ops/grafana/.env backfill_binance
EOF
}

if [ "$#" -ne 2 ]; then
  usage
  exit 64
fi

env_file=$1
service_name=$2
config_path="$(dirname "$0")/runtime.yaml"

if [ ! -f "$env_file" ]; then
  echo "env file not found: $env_file" >&2
  exit 66
fi

if [ ! -f "$config_path" ]; then
  echo "runtime config not found: $config_path" >&2
  exit 66
fi

set -a
# shellcheck disable=SC1090
. "$env_file"
set +a

export SANDBOX_QUANT_POSTGRES_URL="postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@127.0.0.1:${POSTGRES_PORT:-5432}/${POSTGRES_DB}"

eval_output=$(
  ruby -e '
    require "yaml"
    config = YAML.load_file(ARGV[0])
    service = config.fetch("services").fetch(ARGV[1]) rescue nil
    abort("unknown service: #{ARGV[1]}") if service.nil?
    env = service["env"] || {}
    env.each do |key, value|
      puts "export #{key}=#{value.to_s.dump}"
    end
    cmd = service["command"] || []
    abort("service command missing: #{ARGV[1]}") if cmd.empty?
    puts "exec " + cmd.map(&:to_s).map(&:dump).join(" ")
  ' "$config_path" "$service_name"
)

eval "$eval_output"

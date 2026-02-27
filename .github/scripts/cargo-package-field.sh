#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo "Usage: $0 <Cargo.toml path> <field>" >&2
  exit 2
fi

manifest_path="$1"
field="$2"

if [ ! -f "$manifest_path" ]; then
  echo "Cargo manifest not found: $manifest_path" >&2
  exit 1
fi

value="$(
  awk -v requested_field="$field" '
    /^\[package\][[:space:]]*$/ { in_package = 1; next }
    /^\[[^]]+\][[:space:]]*$/ {
      if (in_package) exit
    }
    in_package && $0 ~ ("^[[:space:]]*" requested_field "[[:space:]]*=") {
      if (match($0, /"[^"]+"/)) {
        print substr($0, RSTART + 1, RLENGTH - 2)
        exit
      }
    }
  ' "$manifest_path"
)"

if [ -z "$value" ]; then
  echo "Could not read package.$field from $manifest_path" >&2
  exit 1
fi

printf '%s\n' "$value"

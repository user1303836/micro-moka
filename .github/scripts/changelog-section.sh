#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo "Usage: $0 <changelog path> <version>" >&2
  exit 2
fi

changelog_path="$1"
version="$2"

if [ ! -f "$changelog_path" ]; then
  echo "Changelog not found: $changelog_path" >&2
  exit 1
fi

awk -v requested_version="$version" '
  $0 ~ ("^## \\[" requested_version "\\]") { in_section = 1; next }
  in_section && /^## \[/ { exit }
  in_section { print }
' "$changelog_path"

#!/usr/bin/env bash
# Regenerate golden fixtures from the upstream TypeScript engine.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
upstream="$root/upstream"

if [[ ! -d "$upstream" ]]; then
  echo "error: upstream reference repo not found at $upstream" >&2
  echo "  git clone https://github.com/Shemarhn/Github_Ranked.git \"$upstream\"" >&2
  exit 1
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# Node's type-stripping needs explicit file extensions on relative imports.
cp "$upstream/lib/ranking/engine.ts" "$upstream/lib/ranking/constants.ts" "$work/"
cp "$root/tools/oracle/generate.ts" "$work/"
sed -i "s|from './constants'|from './constants.ts'|" "$work/engine.ts"

mkdir -p "$root/fixtures"
node --experimental-strip-types "$work/generate.ts" "$root/fixtures"

echo "fixtures written to $root/fixtures"

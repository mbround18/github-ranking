#!/usr/bin/env bash
# Build the wasm bundle into the frontend's source tree.
#
# The frontend imports this like any other module; Vite handles the .wasm asset.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
out="$root/web/src/wasm"

cd "$root"
wasm-pack build crates/wasm \
  --target web \
  --out-dir "$out" \
  --out-name github_ranked \
  --release \
  --no-pack

# wasm-pack drops a .gitignore that would exclude the build from the repo; the
# frontend build needs it present when Docker copies the context.
rm -f "$out/.gitignore"

echo "wasm built into $out"
ls -la "$out"

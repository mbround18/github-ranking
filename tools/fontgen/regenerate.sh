#!/usr/bin/env bash
# Regenerate the embedded glyph tables.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

# Noto Sans (SIL OFL 1.1) — permissively licensed for embedding. Swap these
# paths to change the card's typeface; nothing else needs to change.
REGULAR="${FONT_REGULAR:-/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf}"
BOLD="${FONT_BOLD:-/usr/share/fonts/truetype/noto/NotoSans-Bold.ttf}"

for font in "$REGULAR" "$BOLD"; do
  [[ -f "$font" ]] || { echo "error: font not found: $font" >&2; exit 1; }
done

cargo run --quiet -p fontgen -- "$REGULAR" "$BOLD" "$root/crates/core/src/render/glyphs.rs"

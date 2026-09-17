#!/usr/bin/env bash
# Fail when crates/ui/src paints text_color with stacked alpha on a theme
# paper (text / text_muted / text_faint) without `// a11y-ok: <reason>` on
# the same line or the line immediately above. Hsla::opacity multiplies.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC="$ROOT/crates/ui/src"
PATTERN='text_color\([^)]*theme\.text(_muted|_faint)?\.opacity\('
MARKER='//[[:space:]]*a11y-ok:[[:space:]]*[^[:space:]]'

has_marker() {
  printf '%s\n' "$1" | grep -qE "$MARKER"
}

violations=0
while IFS= read -r match; do
  [ -z "$match" ] && continue
  file="${match%%:*}"
  rest="${match#*:}"
  lineno="${rest%%:*}"
  snippet="${rest#*:}"
  rel="${file#"$ROOT/"}"

  if has_marker "$snippet"; then
    continue
  fi
  if [ "$lineno" -gt 1 ]; then
    prev="$(sed -n "$((lineno - 1))p" "$file")"
    if has_marker "$prev"; then
      continue
    fi
  fi

  printf '%s:%s:%s\n' "$rel" "$lineno" "$snippet"
  violations=$((violations + 1))
done < <(grep -R -n --include='*.rs' -E "$PATTERN" "$SRC" || true)

if [ "$violations" -ne 0 ]; then
  printf 'lint-text-alpha: %s site(s) stack opacity on theme text paper without // a11y-ok:\n' "$violations" >&2
  exit 1
fi

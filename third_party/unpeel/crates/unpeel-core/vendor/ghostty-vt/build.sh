#!/bin/bash
# Rebuild the vendored libghostty-vt.a from the ghostty checkout used for
# the app's GhosttyKit build. See README.md for requirements.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$HERE/../../../.." && pwd)"
GHOSTTY_SRC="$REPO_ROOT/apps/native/vendor/libghostty-spm/References/ghostty-upstream"

if [ ! -d "$GHOSTTY_SRC" ]; then
    echo "[-] ghostty source not found: $GHOSTTY_SRC" >&2
    exit 1
fi

ZIG="${ZIG:-/opt/homebrew/opt/zig@0.15/bin/zig}"
if ! "$ZIG" version >/dev/null 2>&1; then
    echo "[-] zig not found at $ZIG (brew install zig@0.15)" >&2
    exit 1
fi

echo "[*] zig: $("$ZIG" version) building lib-vt from $GHOSTTY_SRC"

# macOS universal (fat xcframework slice, needs xcodebuild).
(cd "$GHOSTTY_SRC" && PATH="$(dirname "$ZIG"):$PATH" zig build -Demit-lib-vt -Doptimize=ReleaseFast)
SRC_A="$GHOSTTY_SRC/zig-out/lib/ghostty-vt.xcframework/macos-arm64_x86_64/libghostty-vt.a"
mkdir -p "$HERE/macos-universal"
cp "$SRC_A" "$HERE/macos-universal/libghostty-vt.a"
lipo -info "$HERE/macos-universal/libghostty-vt.a"
echo "[+] vendored: $HERE/macos-universal/libghostty-vt.a"

# Linux slices for headless hosts (cross-compiled from macOS by zig; the
# generic static path in build.zig emits zig-out/lib/libghostty-vt.a).
for pair in "aarch64-linux-gnu:linux-aarch64" "x86_64-linux-gnu:linux-x86_64"; do
    target="${pair%%:*}"
    slice="${pair##*:}"
    echo "[*] building $target"
    (cd "$GHOSTTY_SRC" && PATH="$(dirname "$ZIG"):$PATH" \
        zig build -Demit-lib-vt -Doptimize=ReleaseFast -Dtarget="$target")
    mkdir -p "$HERE/$slice"
    cp "$GHOSTTY_SRC/zig-out/lib/libghostty-vt.a" "$HERE/$slice/libghostty-vt.a"
    echo "[+] vendored: $HERE/$slice/libghostty-vt.a"
done
echo "[!] update the commit hash in README.md: $(git -C "$GHOSTTY_SRC" rev-parse HEAD)"

#!/bin/bash
#
# build-app.sh — assemble an installable Unpeel.app from release builds.
#
# Produces apps/native/dist/Unpeel.app containing:
#   - the release UnpeelNative binary (GhosttyKit is statically linked)
#   - unpeel-host + unpeel-attach release binaries (embedded helpers the app
#     spawns; LaunchConfig resolves them via Bundle.main auxiliary executables)
#   - Sparkle.framework for Cloudflare/R2 appcast updates
#   - the SwiftPM resource bundle (for Bundle.module: the dock icon)
#   - AppIcon.icns + Info.plist
# Then code-signs the bundle. Defaults to ad-hoc signing for local builds; pass
# CODESIGN_IDENTITY="Developer ID Application: …" for distribution builds.
# Developer ID builds are signed with hardened runtime + timestamp so they can
# be notarized before public distribution.
#
# Usage:
#   apps/native/build-app.sh
#   UNPEEL_VERSION=0.1.0-beta.3 UNPEEL_BUILD=5 apps/native/build-app.sh
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
NATIVE_DIR="$REPO_ROOT/apps/native"
SWIFT_DIR="$NATIVE_DIR/UnpeelNative"
DIST="$NATIVE_DIR/dist"
APP="$DIST/Unpeel.app"
VERSION="${UNPEEL_VERSION:-0.1.0-beta.3}"
BUILD="${UNPEEL_BUILD:-5}"
# Empty by default on purpose: a dev build must not be a live Sparkle client
# pointed at the production feed (it would background-check and could replace
# itself with the published release). release.sh injects the channel feed URL.
SPARKLE_FEED_URL="${SPARKLE_FEED_URL:-}"
SPARKLE_PUBLIC_ED_KEY="${SPARKLE_PUBLIC_ED_KEY:-HbKIMOuEVJPtWViS7sbWhWOPj2qFRAiRG3Y4RP52PHg=}"
CODESIGN_IDENTITY="${CODESIGN_IDENTITY:--}"
CODESIGN_ENTITLEMENTS="${CODESIGN_ENTITLEMENTS:-}"
UNPEEL_DEV_BUILD="${UNPEEL_DEV_BUILD:-0}"

# Keep checkout, Cargo registry, and toolchain source paths out of shipped
# Rust panic/location strings. Caller-supplied Rust flags are retained.
. "$REPO_ROOT/scripts/rust-release-env.sh"
unpeel_enable_rust_path_remapping "$REPO_ROOT"

# Remap both debug metadata and compile-time file literals (including
# #filePath) in every Swift release object. -Xswiftc appends to any caller
# environment flags SwiftPM already honors.
SWIFT_PATH_REMAP_FLAGS=(
  -Xswiftc -debug-prefix-map
  -Xswiftc "$REPO_ROOT=/unpeel/source"
  -Xswiftc -file-prefix-map
  -Xswiftc "$REPO_ROOT=/unpeel/source"
)

step() { echo "==> $*"; }
is_adhoc_signing() { [ "$CODESIGN_IDENTITY" = "-" ]; }

codesign_release() {
  local target="$1"
  shift

  local args=(--force --sign "$CODESIGN_IDENTITY")
  if ! is_adhoc_signing; then
    args+=(--timestamp --options runtime)
    if [ -n "$CODESIGN_ENTITLEMENTS" ]; then
      args+=(--entitlements "$CODESIGN_ENTITLEMENTS")
    fi
  fi

  codesign "${args[@]}" "$@" "$target"
}

verify_release_path_privacy() {
  local candidate forbidden_prefix

  # Source paths can survive stripping in panic metadata and Swift #filePath
  # literals. Treat leakage of either this checkout or the build operator's
  # home directory as a release packaging failure. Scan every Mach-O in the
  # finished bundle, including framework helpers and both universal slices.
  while IFS= read -r -d '' candidate; do
    file -b "$candidate" | grep -q 'Mach-O' || continue
    for forbidden_prefix in "$REPO_ROOT" "${HOME:?}/"; do
      if LC_ALL=C grep -aFq -- "$forbidden_prefix" "$candidate"; then
        echo "FAIL: release Mach-O embeds a private build path: $candidate" >&2
        echo "      matched forbidden prefix: $forbidden_prefix" >&2
        exit 1
      fi
    done
  done < <(find "$APP" -type f -print0)
}

verify_release_architectures() {
  local binary

  # The native app's declared support floor is Apple silicon. A release may
  # become universal later, but it must never silently inherit an Intel build
  # host's architecture and publish without an arm64 slice.
  for binary in UnpeelNative unpeel-host unpeel-attach; do
    if ! lipo "$APP/Contents/MacOS/$binary" -verify_arch arm64 >/dev/null 2>&1; then
      echo "FAIL: release binary does not contain the required arm64 slice: $binary" >&2
      lipo -info "$APP/Contents/MacOS/$binary" >&2 || true
      exit 1
    fi
  done

  # Browser MCP deliberately ships both native slices even though the current
  # app support floor is arm64, keeping that separately pinned payload intact.
  if ! lipo "$APP/Contents/MacOS/agent-browser" \
    -verify_arch arm64 x86_64 >/dev/null 2>&1
  then
    echo "FAIL: release agent-browser must contain arm64 and x86_64 slices" >&2
    lipo -info "$APP/Contents/MacOS/agent-browser" >&2 || true
    exit 1
  fi
}

# --- 1. Release builds ------------------------------------------------------

step "building native Rust bridge (release)"
"$NATIVE_DIR/build-rust-bridge.sh" release

step "building UnpeelNative (release)"
(cd "$SWIFT_DIR" && swift build -c release "${SWIFT_PATH_REMAP_FLAGS[@]}")

step "building unpeel-host (release)"
(cd "$REPO_ROOT/crates" && cargo build --release --locked --bin unpeel-host)

step "building unpeel-attach (release)"
(cd "$NATIVE_DIR/unpeel-attach" && cargo build --release --locked)

SWIFT_BIN_DIR="$(cd "$SWIFT_DIR" && swift build -c release --show-bin-path "${SWIFT_PATH_REMAP_FLAGS[@]}")"
APP_BIN="$SWIFT_BIN_DIR/UnpeelNative"
HOST_BIN="$REPO_ROOT/crates/target/release/unpeel-host"
ATTACH_BIN="$NATIVE_DIR/unpeel-attach/target/release/unpeel-attach"
RES_BUNDLE="$SWIFT_BIN_DIR/UnpeelNative_UnpeelNative.bundle"
SPARKLE_FRAMEWORK="$SWIFT_DIR/.build/artifacts/sparkle/Sparkle/Sparkle.xcframework/macos-arm64_x86_64/Sparkle.framework"

for f in "$APP_BIN" "$HOST_BIN" "$ATTACH_BIN"; do
  [ -x "$f" ] || { echo "FAIL: missing build product $f" >&2; exit 1; }
done
[ -d "$SPARKLE_FRAMEWORK" ] || {
  echo "FAIL: missing Sparkle framework $SPARKLE_FRAMEWORK" >&2
  echo "      run: cd $SWIFT_DIR && swift package resolve" >&2
  exit 1
}

# --- 2. App icon (Icon Composer .icon → asset catalog) ----------------------
#
# macOS 26 (Tahoe) gives the modern full-size Dock/Launchpad icon treatment only
# to icons shipped as an Icon Composer ".icon" compiled into an asset catalog
# (Assets.car, referenced by Info.plist's CFBundleIconName) — the same packaging
# Claude/Codex use. A bare .icns, or even a classic multi-size .appiconset, is
# treated as legacy and drawn smaller/inset, no matter how full-bleed the art
# is. We synthesize a single-layer .icon from the 1024px square source; actool
# compiles it into Assets.car (the Tahoe "iconstack") plus a fallback
# AppIcon.icns for macOS < 26. The solid fill matches the icon's dark base.

step "preparing AppIcon.icon"
SRC_PNG="$SWIFT_DIR/Sources/UnpeelNative/Resources/AppIcon.png"
ICON_DIR="$(mktemp -d)/AppIcon.icon"
mkdir -p "$ICON_DIR/Assets"
cp "$SRC_PNG" "$ICON_DIR/Assets/AppIcon.png"
# The icon art is transparent, so this fill is the visible background. Dev
# builds get a burnt-orange base so dist and /Applications are tellable apart
# in the Dock at a glance; UNPEEL_ICON_FILL overrides either.
# Release fill = the terminal surface color (#1A1A1F, Theme.swift) so the
# icon base matches the app's own terminal background.
ICON_FILL="srgb:0.102,0.102,0.122,1.0"
[ "$UNPEEL_DEV_BUILD" = "1" ] && ICON_FILL="srgb:0.55,0.25,0.02,1.0"
ICON_FILL="${UNPEEL_ICON_FILL:-$ICON_FILL}"
cat > "$ICON_DIR/icon.json" <<JSON
{
  "fill" : { "solid" : "$ICON_FILL" },
  "groups" : [
    { "layers" : [ { "image-name" : "AppIcon.png", "name" : "AppIcon" } ] }
  ],
  "supported-platforms" : { "circles" : [ ], "squares" : [ "macOS" ] }
}
JSON

# --- 3. Assemble the bundle -------------------------------------------------

step "assembling $APP"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$APP/Contents/Frameworks"

cp "$APP_BIN"    "$APP/Contents/MacOS/UnpeelNative"
cp "$HOST_BIN"   "$APP/Contents/MacOS/unpeel-host"
cp "$ATTACH_BIN" "$APP/Contents/MacOS/unpeel-attach"

# Swift's release linker retains local object/archive provenance (including
# absolute checkout and Cargo paths) even when source locations are prefix-
# mapped. Remove those debug symbols from the staged copy before the privacy
# gate and final code signing. Compile-time #filePath strings remain covered
# by SWIFT_PATH_REMAP_FLAGS above.
if [ "$UNPEEL_DEV_BUILD" != "1" ]; then
  step "stripping native release debug symbols"
  strip -S "$APP/Contents/MacOS/UnpeelNative"
fi

# Unpeel Browser MCP engine (agent-browser, Apache-2.0). Release builds use
# only the exact workspace dependency locked by Bun. Both Darwin slices, the
# package version, host-native executable-reported version, license, and
# package payload checksums are verified before producing one universal signed
# helper. Local managed/PATH discovery remains development-only.
if [ "$UNPEEL_DEV_BUILD" != "1" ]; then
  AGENT_BROWSER_EXPECTED_VERSION="0.31.1"
  AGENT_BROWSER_PACKAGE="$REPO_ROOT/node_modules/agent-browser"
  AGENT_BROWSER_PACKAGE_JSON="$AGENT_BROWSER_PACKAGE/package.json"
  AGENT_BROWSER_ARM64="$AGENT_BROWSER_PACKAGE/bin/agent-browser-darwin-arm64"
  AGENT_BROWSER_X64="$AGENT_BROWSER_PACKAGE/bin/agent-browser-darwin-x64"
  AGENT_BROWSER_LICENSE="$AGENT_BROWSER_PACKAGE/LICENSE"
  [ -s "$AGENT_BROWSER_PACKAGE_JSON" ] || {
    echo "FAIL: pinned agent-browser package is not installed" >&2
    echo "      run: bun install --frozen-lockfile" >&2
    exit 1
  }
  AGENT_BROWSER_PACKAGE_VERSION="$(sed -n 's/^[[:space:]]*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$AGENT_BROWSER_PACKAGE_JSON" | head -1)"
  [ "$AGENT_BROWSER_PACKAGE_VERSION" = "$AGENT_BROWSER_EXPECTED_VERSION" ] || {
    echo "FAIL: agent-browser package is $AGENT_BROWSER_PACKAGE_VERSION; expected $AGENT_BROWSER_EXPECTED_VERSION" >&2
    exit 1
  }
  for spec in \
    "$AGENT_BROWSER_ARM64|fd7acd17b3071ff7f75a03c1ecd30501959d9c2d063bdaa05adb6f77abf2a7bf|arm64" \
    "$AGENT_BROWSER_X64|05aa3e2ed3550e06fb3eb7423a1cef0d9d6031c4d6a8835b9dbe033baf83ef6d|x86_64" \
    "$AGENT_BROWSER_LICENSE|014bb31e83d5c2e76aea1cc6e82217346ab41362f32cb355ad0f5c10aa0aeaff|license"
  do
    IFS='|' read -r source_path expected_sha256 source_kind <<EOF
$spec
EOF
    [ -s "$source_path" ] || { echo "FAIL: missing pinned agent-browser $source_kind: $source_path" >&2; exit 1; }
    actual_sha256="$(shasum -a 256 "$source_path" | awk '{print $1}')"
    [ "$actual_sha256" = "$expected_sha256" ] || {
      echo "FAIL: pinned agent-browser $source_kind checksum mismatch" >&2
      echo "      expected $expected_sha256" >&2
      echo "      actual   $actual_sha256" >&2
      exit 1
    }
  done
  lipo "$AGENT_BROWSER_ARM64" -verify_arch arm64 >/dev/null || {
    echo "FAIL: pinned agent-browser arm64 slice has the wrong architecture" >&2; exit 1;
  }
  lipo "$AGENT_BROWSER_X64" -verify_arch x86_64 >/dev/null || {
    echo "FAIL: pinned agent-browser x86_64 slice has the wrong architecture" >&2; exit 1;
  }
  case "$(uname -m)" in
    arm64|aarch64) AGENT_BROWSER_HOST_SLICE="$AGENT_BROWSER_ARM64" ;;
    x86_64) AGENT_BROWSER_HOST_SLICE="$AGENT_BROWSER_X64" ;;
    *) echo "FAIL: unsupported agent-browser build architecture: $(uname -m)" >&2; exit 1 ;;
  esac
  AGENT_BROWSER_CHECK_DIR="$(mktemp -d)"
  cp "$AGENT_BROWSER_HOST_SLICE" "$AGENT_BROWSER_CHECK_DIR/agent-browser"
  chmod +x "$AGENT_BROWSER_CHECK_DIR/agent-browser"
  reported_version="$("$AGENT_BROWSER_CHECK_DIR/agent-browser" --version 2>&1)" || {
    echo "FAIL: could not execute the host-native pinned agent-browser slice" >&2; exit 1;
  }
  [ "$reported_version" = "agent-browser $AGENT_BROWSER_EXPECTED_VERSION" ] || {
    echo "FAIL: agent-browser slice reported '$reported_version'" >&2; exit 1;
  }
  rm -rf "$AGENT_BROWSER_CHECK_DIR"
  step "bundling pinned universal agent-browser $AGENT_BROWSER_EXPECTED_VERSION"
  lipo -create "$AGENT_BROWSER_ARM64" "$AGENT_BROWSER_X64" \
    -output "$APP/Contents/MacOS/agent-browser"
  chmod +x "$APP/Contents/MacOS/agent-browser"
  lipo "$APP/Contents/MacOS/agent-browser" -verify_arch arm64 x86_64 >/dev/null || {
    echo "FAIL: bundled agent-browser is not universal" >&2; exit 1;
  }
  cp "$AGENT_BROWSER_LICENSE" "$APP/Contents/Resources/agent-browser-LICENSE.txt"
else
  AGENT_BROWSER_SRC="${UNPEEL_AGENT_BROWSER_BIN:-}"
  if [ -z "$AGENT_BROWSER_SRC" ]; then
    for candidate in \
      "$HOME/.unpeel/browser/bin/agent-browser" \
      "$(command -v agent-browser 2>/dev/null || true)"; do
      [ -n "$candidate" ] && [ -e "$candidate" ] || continue
      resolved="$(readlink -f "$candidate" 2>/dev/null || echo "$candidate")"
      if head -c 2 "$resolved" 2>/dev/null | grep -q '#!'; then
        arch="$(uname -m | sed 's/x86_64/x64/')"
        resolved="$(dirname "$resolved")/agent-browser-darwin-$arch"
      fi
      if [ -f "$resolved" ]; then AGENT_BROWSER_SRC="$resolved"; break; fi
    done
  fi
  if [ -n "$AGENT_BROWSER_SRC" ] && [ -f "$AGENT_BROWSER_SRC" ]; then
    step "bundling development agent-browser engine ($AGENT_BROWSER_SRC)"
    cp -L "$AGENT_BROWSER_SRC" "$APP/Contents/MacOS/agent-browser"
    chmod +x "$APP/Contents/MacOS/agent-browser"
    AGENT_BROWSER_LICENSE="${UNPEEL_AGENT_BROWSER_LICENSE:-$(dirname "$AGENT_BROWSER_SRC")/../LICENSE}"
    if [ -s "$AGENT_BROWSER_LICENSE" ]; then
      cp "$AGENT_BROWSER_LICENSE" "$APP/Contents/Resources/agent-browser-LICENSE.txt"
    fi
  else
    echo "note: agent-browser engine not found — Browser MCP resolves it at runtime (dev fallback)"
  fi
fi

# Computer Use is development-build-only until hosted sessions have a kernel-
# enforced broker boundary. The embedded unrestricted daemon inherits the
# app's TCC grants, so shipping it in a release would let same-UID hosted code
# bypass Unpeel's cooperative approval UI by calling its raw socket directly.
if [ "$UNPEEL_DEV_BUILD" = "1" ]; then
  CUA_DRIVER_SRC="${UNPEEL_CUA_DRIVER_BIN:-}"
  if [ -z "$CUA_DRIVER_SRC" ]; then
    for candidate in \
      "$HOME/.unpeel/computer/bin/cua-driver" \
      "$(command -v cua-driver 2>/dev/null || true)" \
      "$HOME/.local/bin/cua-driver"; do
      [ -n "$candidate" ] && [ -e "$candidate" ] || continue
      resolved="$(readlink -f "$candidate" 2>/dev/null || echo "$candidate")"
      if [ -f "$resolved" ]; then CUA_DRIVER_SRC="$resolved"; break; fi
    done
  fi
  if [ -n "$CUA_DRIVER_SRC" ] && [ -f "$CUA_DRIVER_SRC" ]; then
    step "bundling development-only cua-driver engine ($CUA_DRIVER_SRC)"
    cp -L "$CUA_DRIVER_SRC" "$APP/Contents/MacOS/cua-driver"
    # MIT notice ships alongside when the source layout carries one.
    CUA_DRIVER_LICENSE="$(dirname "$CUA_DRIVER_SRC")/../LICENSE"
    if [ -f "$CUA_DRIVER_LICENSE" ]; then
      cp "$CUA_DRIVER_LICENSE" "$APP/Contents/Resources/cua-driver-LICENSE.txt"
    fi
  else
    echo "note: cua-driver engine not found — development Computer Use is unavailable"
  fi
else
  step "excluding cua-driver from release build (Computer Use is security-blocked)"
fi

# Source guard for the production containment above. Keep this independent of
# the branch that chooses the engine so a future refactor cannot accidentally
# place the TCC-bearing helper back into a customer bundle.
if [ "$UNPEEL_DEV_BUILD" != "1" ] && [ -e "$APP/Contents/MacOS/cua-driver" ]; then
  echo "FAIL: release bundle contains security-blocked cua-driver" >&2
  exit 1
fi

# License payloads are part of the signed app. Rust notices follow the exact
# locked dependency graphs for every embedded Rust component. Native-only
# Swift/framework notices stay separate so CLI archives do not inherit them.
step "collecting release licenses and third-party notices"
cp "$REPO_ROOT/LICENSE" "$APP/Contents/Resources/LICENSE.txt"
RUST_NOTICE_TARGET="$(rustc -vV | sed -n 's/^host: //p')"
[ -n "$RUST_NOTICE_TARGET" ] || { echo "FAIL: rustc did not report a host target" >&2; exit 1; }
cargo run --quiet --locked \
  --manifest-path "$REPO_ROOT/crates/Cargo.toml" \
  -p unpeel-license-notices -- \
  --manifest-path "$REPO_ROOT/crates/Cargo.toml" \
  --package unpeel-host \
  --package unpeel-native-bridge \
  --manifest-path "$NATIVE_DIR/unpeel-attach/Cargo.toml" \
  --package unpeel-attach \
  --target "$RUST_NOTICE_TARGET" \
  --output "$APP/Contents/Resources/THIRD_PARTY_NOTICES_RUST.txt"
sh "$NATIVE_DIR/collect-swift-notices.sh" \
  "$APP/Contents/Resources/THIRD_PARTY_NOTICES_SWIFT.txt"

chmod +x "$APP/Contents/MacOS/"*
if ! otool -l "$APP/Contents/MacOS/UnpeelNative" | grep -q "@loader_path/../Frameworks"; then
  install_name_tool -add_rpath "@loader_path/../Frameworks" "$APP/Contents/MacOS/UnpeelNative"
fi
# SwiftPM resource bundle: Contents/Resources keeps codesign treating it as a
# resource, not stray code in MacOS/. Code must resolve it via ModuleResources
# (Bundle.main.resourceURL) — NOT Bundle.module, whose executable-target
# accessor only checks the .app root and the build machine's .build path, then
# fatalErrors (the beta.6–25 Settings ▸ Mobile crash).
[ -d "$RES_BUNDLE" ] && cp -R "$RES_BUNDLE" "$APP/Contents/Resources/"
ditto "$SPARKLE_FRAMEWORK" "$APP/Contents/Frameworks/Sparkle.framework"

# Compile the .icon → Assets.car (Tahoe iconstack) + AppIcon.icns (legacy
# fallback), both into Contents/Resources.
xcrun actool "$ICON_DIR" \
  --compile "$APP/Contents/Resources" \
  --app-icon AppIcon \
  --platform macosx \
  --minimum-deployment-target 13.0 \
  --output-partial-info-plist "$(mktemp)" >/dev/null
[ -f "$APP/Contents/Resources/Assets.car" ] || {
  echo "FAIL: actool did not produce Assets.car" >&2; exit 1
}
# Force the Dock to use the Assets.car iconstack (the full-size Liquid Glass
# render on macOS 26), not the legacy .icns: if a loose AppIcon.icns sits next
# to it AND CFBundleIconFile names it, the system resolves "AppIcon" to that
# legacy file and draws the classic, smaller icon. Drop the loose .icns and the
# CFBundleIconFile key (below) so only CFBundleIconName → Assets.car remains.
rm -f "$APP/Contents/Resources/AppIcon.icns"

# Sparkle update keys only when a feed URL is set (release builds). Without
# them the app's updater never starts (sparkleCanStart requires a feed).
SPARKLE_PLIST_KEYS=""
if [ -n "$SPARKLE_FEED_URL" ]; then
  SPARKLE_PLIST_KEYS="    <key>SUFeedURL</key>               <string>$SPARKLE_FEED_URL</string>
    <key>SUPublicEDKey</key>           <string>$SPARKLE_PUBLIC_ED_KEY</string>
    <key>SUEnableAutomaticChecks</key> <true/>"
fi
DEV_BUILD_PLIST_KEYS=""
# Dev builds are named "Unpeel Dev" (menu bar, Dock tooltip, force-quit list)
# so they're tellable from the installed release app; the bundle id stays
# com.unpeel.native either way. Quit them with `osascript -e 'quit app
# "Unpeel Dev"'` — plain "Unpeel" targets the installed app.
APP_NAME="Unpeel"
if [ "$UNPEEL_DEV_BUILD" = "1" ]; then
  DEV_BUILD_PLIST_KEYS="    <key>UnpeelDevelopmentBuild</key> <true/>"
  APP_NAME="Unpeel Dev"
fi

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>            <string>$APP_NAME</string>
    <key>CFBundleDisplayName</key>     <string>$APP_NAME</string>
    <key>CFBundleExecutable</key>      <string>UnpeelNative</string>
    <key>CFBundleIdentifier</key>      <string>com.unpeel.native</string>
    <key>CFBundleIconName</key>        <string>AppIcon</string>
    <key>CFBundlePackageType</key>     <string>APPL</string>
    <key>CFBundleShortVersionString</key> <string>$VERSION</string>
    <key>CFBundleVersion</key>         <string>$BUILD</string>
    <key>NSHumanReadableCopyright</key> <string>© $(date +%Y) UX Themes AS</string>
    <key>LSMinimumSystemVersion</key>  <string>13.0</string>
    <key>NSHighResolutionCapable</key> <true/>
    <key>NSSupportsAutomaticGraphicsSwitching</key> <true/>
    <key>LSApplicationCategoryType</key> <string>public.app-category.developer-tools</string>
$DEV_BUILD_PLIST_KEYS
$SPARKLE_PLIST_KEYS
    <!-- Finder right-click ▸ Services ▸ "New Unpeel Session Here". Shows on
         folders (NSSendFileTypes = public.folder); AppKit routes the message
         to AppDelegate.newUnpeelSession. After first install macOS may need a
         Launch Services refresh: /System/Library/CoreServices/pbs -update -->
    <key>NSServices</key>
    <array>
        <dict>
            <key>NSMenuItem</key>
            <dict>
                <key>default</key>
                <string>New Unpeel Session Here</string>
            </dict>
            <key>NSMessage</key>      <string>newUnpeelSession</string>
            <key>NSPortName</key>     <string>Unpeel</string>
            <key>NSSendFileTypes</key>
            <array>
                <string>public.folder</string>
            </array>
        </dict>
    </array>
</dict>
</plist>
PLIST

if [ "$UNPEEL_DEV_BUILD" != "1" ]; then
  step "checking release binary architectures"
  verify_release_architectures
  step "checking release binaries for private build paths"
  verify_release_path_privacy
fi

# --- 4. Code sign -----------------------------------------------------------

# Sparkle's nested executables are signed individually (inside-out), never
# with --deep and never with the app's entitlements: --deep is deprecated and
# would stamp CODESIGN_ENTITLEMENTS onto Sparkle's XPC services, while
# --preserve-metadata keeps their own entitlements (e.g. Downloader.xpc's
# sandbox) intact — per Sparkle's own signing guidance.
codesign_sparkle() {
  local args=(--force --sign "$CODESIGN_IDENTITY" --preserve-metadata=entitlements)
  if ! is_adhoc_signing; then
    args+=(--timestamp --options runtime)
  fi
  codesign "${args[@]}" "$1"
}

step "code signing"
# Sign the embedded helpers first, then the app bundle (inside-out).
codesign_release "$APP/Contents/MacOS/unpeel-host"
codesign_release "$APP/Contents/MacOS/unpeel-attach"
if [ -f "$APP/Contents/MacOS/agent-browser" ]; then
  # Re-sign the third-party engines with our identity so notarization covers them.
  codesign_release "$APP/Contents/MacOS/agent-browser"
fi
if [ -f "$APP/Contents/MacOS/cua-driver" ]; then
  codesign_release "$APP/Contents/MacOS/cua-driver"
fi
SPARKLE_FW="$APP/Contents/Frameworks/Sparkle.framework"
codesign_sparkle "$SPARKLE_FW/Versions/B/XPCServices/Downloader.xpc"
codesign_sparkle "$SPARKLE_FW/Versions/B/XPCServices/Installer.xpc"
codesign_sparkle "$SPARKLE_FW/Versions/B/Autoupdate"
codesign_sparkle "$SPARKLE_FW/Versions/B/Updater.app"
codesign_sparkle "$SPARKLE_FW"
codesign_release "$APP"
codesign --verify --deep --strict "$APP" && echo "    signature OK"

echo
echo "Built: $APP"
du -sh "$APP" | awk '{print "Size:  "$1}'

#!/usr/bin/env bash
# dev-app.sh — build, stably sign, and launch Unpeel.app for local development.
#
# Why this exists: build-app.sh defaults to ad-hoc signing ("-") when no
# CODESIGN_IDENTITY is set. An ad-hoc signature's designated requirement is the
# binary's cdhash, which changes on every rebuild, so the macOS keychain ACL for
# com.unpeel.license never matches the new build and you get the
# "Unpeel wants to access key" password prompt after every rebuild — even after
# clicking "Always Allow", because that only trusts the old cdhash.
#
# Signing with a stable identity (your local "Apple Development" cert) anchors
# the designated requirement to the cert + Team ID, which is constant across
# rebuilds. After one final "Always Allow", the prompt stops for good.
#
# Override the identity with CODESIGN_IDENTITY=... if auto-detection picks wrong.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Pick a stable signing identity: explicit override, else the first local
# "Apple Development" cert. We deliberately avoid ad-hoc ("-") here.
if [ -z "${CODESIGN_IDENTITY:-}" ]; then
  CODESIGN_IDENTITY="$(security find-identity -v -p codesigning 2>/dev/null \
    | grep "Apple Development" \
    | head -n1 \
    | sed -E 's/^[[:space:]]*[0-9]+\)[[:space:]]+[0-9A-F]+[[:space:]]+"(.*)"$/\1/')"
fi

if [ -z "$CODESIGN_IDENTITY" ] || [ "$CODESIGN_IDENTITY" = "-" ]; then
  echo "error: no stable code-signing identity found." >&2
  echo "       Set CODESIGN_IDENTITY to a local cert, e.g.:" >&2
  echo "       security find-identity -v -p codesigning" >&2
  exit 1
fi

echo "==> dev build, signing with: $CODESIGN_IDENTITY"
UNPEEL_DEV_BUILD=1 CODESIGN_IDENTITY="$CODESIGN_IDENTITY" "$HERE/build-app.sh"

APP="$HERE/dist/Unpeel.app"
echo "==> launching $APP"
# Dev and release intentionally share a bundle id. Opening the bundle as a
# document lets Launch Services resolve that id back to /Applications. Pass the
# exact bundle as the application instead, and request a fresh instance.
open -n -a "$APP"

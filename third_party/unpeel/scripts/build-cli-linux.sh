#!/bin/sh
# Build the two-binary Linux CLI archive consumed by release-cli.mjs.
# Run on the architecture being packaged; this deliberately does not pretend
# a cross-compiled binary has passed the runtime proof for that architecture.
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$script_dir/.." && pwd)
manifest="$repo_root/crates/Cargo.toml"
output_dir=${UNPEEL_CLI_OUTPUT_DIR:-"$repo_root/dist/cli"}

if [ "$(uname -s)" != Linux ]; then
  echo "error: build-cli-linux.sh must run on Linux" >&2
  exit 1
fi

case "$(uname -m)" in
  x86_64|amd64) target=linux-x86_64 ;;
  arm64|aarch64) target=linux-aarch64 ;;
  *) echo "error: unsupported Linux architecture: $(uname -m)" >&2; exit 1 ;;
esac

command -v cargo >/dev/null 2>&1 || { echo "error: cargo is required" >&2; exit 1; }
command -v git >/dev/null 2>&1 || { echo "error: git is required" >&2; exit 1; }
command -v objdump >/dev/null 2>&1 || { echo "error: objdump is required" >&2; exit 1; }
command -v tar >/dev/null 2>&1 || { echo "error: tar is required" >&2; exit 1; }
command -v sha256sum >/dev/null 2>&1 || { echo "error: sha256sum is required" >&2; exit 1; }

. "$repo_root/scripts/rust-release-env.sh"
. "$repo_root/scripts/cli-glibc.sh"
unpeel_enable_rust_path_remapping "$repo_root"

version=$(sed -n 's/^version[[:space:]]*=[[:space:]]*"\([^"]*\)"/\1/p' "$manifest" | head -1)
if [ -z "$version" ]; then
  echo "error: workspace version not found in $manifest" >&2
  exit 1
fi

cargo build --release --locked --manifest-path "$manifest" -p unpeel-tui -p unpeel-host

target_dir=${CARGO_TARGET_DIR:-"$repo_root/crates/target"}
stage=$(mktemp -d "${TMPDIR:-/tmp}/unpeel-cli-linux.XXXXXX")
trap 'rm -rf "$stage"' EXIT INT TERM
glibc_ceiling=2.31
for bin in unpeel unpeel-host; do
  source_path="$target_dir/release/$bin"
  if [ ! -x "$source_path" ]; then
    echo "error: expected executable missing: $source_path" >&2
    exit 1
  fi

  # Official Linux archives support Ubuntu 20.04 / Debian 11 and newer.
  # Building directly on a newer distro can silently raise the dynamic GLIBC
  # floor (ubuntu-latest produced GLIBC_2.39 once), so fail before packaging.
  objdump_symbols=$(objdump -T "$source_path") || {
    echo "error: could not inspect GLIBC symbols in $source_path" >&2
    exit 1
  }
  required_glibc=$(printf '%s\n' "$objdump_symbols" | unpeel_highest_glibc_version)
  if [ -n "$required_glibc" ]; then
    if ! unpeel_glibc_version_at_most "$required_glibc" "$glibc_ceiling"; then
      echo "error: $bin requires GLIBC_$required_glibc (release ceiling is GLIBC_$glibc_ceiling)" >&2
      echo "       build the release archive in the pinned Bullseye container" >&2
      exit 1
    fi
    echo "$bin requires at most GLIBC_$required_glibc (ceiling GLIBC_$glibc_ceiling)"
  fi
  install -m 755 "$source_path" "$stage/$bin"
done

install -m 644 "$repo_root/LICENSE" "$stage/LICENSE"
rust_notice_target=$(rustc -vV | sed -n 's/^host: //p')
if [ -z "$rust_notice_target" ]; then
  echo "error: rustc did not report a host target" >&2
  exit 1
fi
cargo run --quiet --locked --manifest-path "$manifest" -p unpeel-license-notices -- \
  --manifest-path "$manifest" \
  --package unpeel-tui \
  --package unpeel-host \
  --target "$rust_notice_target" \
  --output "$stage/THIRD_PARTY_NOTICES.txt"

source_commit=$(git -C "$repo_root" rev-parse --verify HEAD)
source_dirty=false
if [ -n "$(git -C "$repo_root" status --porcelain=v1 --untracked-files=all)" ]; then
  source_dirty=true
fi
printf '{\n  "schema": 1,\n  "version": "%s",\n  "target": "%s",\n  "source_commit": "%s",\n  "source_dirty": %s\n}\n' \
  "$version" "$target" "$source_commit" "$source_dirty" \
  > "$stage/BUILD_PROVENANCE.json"

mkdir -p "$output_dir"
archive="$output_dir/unpeel-$version-$target.tar.gz"
tar -czf "$archive" -C "$stage" \
  unpeel unpeel-host LICENSE THIRD_PARTY_NOTICES.txt BUILD_PROVENANCE.json
digest=$(sha256sum "$archive" | awk '{print $1}')
printf '%s  %s\n' "$digest" "$(basename "$archive")" > "$archive.sha256"

echo "Built $archive"
echo "Attach with: bun run release:cli -- --channel <channel> --$target $archive"
"$stage/unpeel" --version

# Vendored libghostty-vt

Three slices, one per host platform — `macos-universal/` (fat arm64 +
x86_64), `linux-aarch64/`, and `linux-x86_64/`. `build.rs` picks the slice
from the target triple, so a headless Linux host parses PTY output with the
exact same VT engine as the Mac app and the phone. The Linux slices are
cross-compiled from macOS by zig (`build.sh` builds all three); they come
from the generic static path in ghostty's `build.zig`, not the xcframework,
and link against `libstdc++` rather than `libc++`.

`macos-universal/libghostty-vt.a` is the standalone terminal-emulation C
library from ghostty (https://libghostty.tip.ghostty.org), used by
`src/terminal_viewport.rs` so the host parses PTY output with the exact same
VT engine that renders it on desktop and phone (GhosttyKit). The archive is
a fat arm64 + x86_64 static lib with the vendored SIMD deps (simdutf,
highway) combined in, so it links with no extra dependencies beyond libc++.

Built from the same ghostty checkout the app's GhosttyKit.xcframework comes
from (`apps/native/vendor/libghostty-spm/References/ghostty-upstream`),
commit `2da015cd6ac06cedc89e09756e895d2c1715205d` (tip, 2026-07-06 — the
VT-throughput optimization batch). Rebuild with:

```sh
./build.sh            # from this directory
```

Requirements (same as the GhosttyKit tip build — see
`apps/native/vendor/libghostty-spm/UNPEEL-PATCHES.md`):

- zig 0.15.2 exactly: Homebrew `zig@0.15` (the ziglang.org 0.15.2 tarball
  cannot link on macOS 26).
- Xcode Metal Toolchain is NOT needed for lib-vt (no renderer).

The C API is declared by hand in `src/ghostty_vt.rs` (no bindgen). If you
bump the vendored archive, diff `include/ghostty/vt/*.h` in the ghostty
checkout against the declarations there — the `layout_matches_type_json`
test cross-checks struct sizes against `ghostty_type_json()` at runtime.

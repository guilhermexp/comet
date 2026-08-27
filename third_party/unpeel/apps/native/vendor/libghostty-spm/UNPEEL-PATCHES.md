# Unpeel local patches

Vendored from https://github.com/Lakr233/libghostty-spm at tag `1.2.4`
(`.git` and `Example/` removed). UnpeelNative consumes this copy via a
path dependency because these changes are not upstream yet:

> **libghostty core upgraded to ghostty tip (2026-07-09).** The binary
> XCFramework is no longer Lakr233's `storage.1.2.4` release — it is built
> from ghostty tip (commit `2da015cd6`, which includes the ~1.5–6x VT
> throughput optimizations) via `./build.sh --skip-tests`, and
> `Package.swift` now uses the local path target
> (`BinaryTarget/GhosttyKit.xcframework`, gitignored — every fresh checkout
> must run the build once). Build prerequisites on this machine:
> - **zig 0.15.2 exactly** (ghostty pins it). Use Homebrew's `zig@0.15`
>   keg (`PATH="/opt/homebrew/opt/zig@0.15/bin:$PATH" ./build.sh …`). The
>   official ziglang.org 0.15.2 tarball cannot link anything on macOS 26
>   (undefined libSystem symbols) — do not use it.
> - **Xcode Metal Toolchain** (Xcode 26 ships it as a separate download):
>   `xcodebuild -downloadComponent MetalToolchain`.
>
> The tip source lives in `References/ghostty-upstream` (gitignored) with
> the full Unpeel patch set applied to its working tree; that state is
> snapshotted in `UNPEEL-tip-rebase.diff` (checked in — re-apply it to a
> fresh tip checkout to reproduce). `Patches/ghostty/0002-host-managed-io`
> and `0003-prebuilt-framedata` moved to `Patches/superseded-1.2.4/`; their
> content is folded into the rebase diff (tip now has
> `src/termio/HostManaged.zig`). The remaining `Patches/ghostty/*.sh`
> scripts are idempotent and still run.
>
> Distribution TODO: host `build/GhosttyKit.xcframework.zip` (~50MB) on R2
> and switch `Package.swift` back to a url+checksum binary target so fresh
> clones and CI don't need zig.

1. **Scrollbar metrics exposed** — `GHOSTTY_ACTION_SCROLLBAR` was falling
   into the `default:` (log-only) case. Added:
   - `TerminalScrollbarMetrics` + `TerminalSurfaceScrollbarDelegate` in
     `Sources/GhosttyTerminal/Surface/TerminalSurfaceViewDelegate.swift`
   - dispatch case in
     `Sources/GhosttyTerminal/InMemory/TerminalCallbackBridge.swift`
   Used by Unpeel's terminal scroll-to-bottom button.

2. **`TerminalSurface.performBindingAction` made `public`** in
   `Sources/GhosttyTerminal/Surface/TerminalSurface.swift` (was internal)
   so the host can run `scroll_to_bottom`.

3. **Synchronous render exposed** — `AppTerminalView.renderImmediately()`
   (new `open func`) calling `TerminalSurfaceCoordinator.renderImmediately()`
   (was `private`, now internal). The default wakeup path defers the first
   draw after a view re-attaches to the next runloop turn via
   `DispatchQueue.main.async`, so swapping a retained terminal view into
   the hierarchy presents the stale pre-detach drawable for a frame or
   two. Unpeel calls this right after adopting + focusing a pane on
   session switch so the swap transaction already contains a fresh,
   focused-cursor frame.

If you upgrade the vendored copy, re-apply these (or upstream them first —
both are small and generally useful).

4. **Forced refit exposed** — `AppTerminalView.forceRefit()` (new `public
   func`) calling `TerminalSurfaceCoordinator.forceRefit()` (new). A
   `fitToSize()` after re-attaching a retained view sends ghostty the
   same pixel size it already has — a no-op — so layout drift picked up
   while the view was detached/paused survives until the user manually
   resizes the window. `forceRefit` nudges the surface one pixel
   narrower, then synchronizes back to the true size, so ghostty runs
   its real resize path; both writes land before the next draw. Unpeel
   calls this after every session switch.

5. **Ghostty resource teardown moved off the main actor** —
   `TerminalSurfaceCoordinator` now detaches a raw surface on the main
   actor, then frees it on a serial utility queue. `TerminalController`
   uses the same queue for `ghostty_app_free` / `ghostty_config_free`, so
   app teardown stays ordered after surface teardown. This avoids macOS
   hangs where AppKit autorelease cleanup deallocated `AppTerminalView`
   on the main thread and `ghostty_surface_free` blocked waiting for
   termio backend threads.

6. **Immediate macOS scrollback repaint** — `AppTerminalView.scrollWheel`
   calls `renderImmediately()` after forwarding a scroll event when the
   terminal is not mouse-captured by a full-screen TUI. This removes the
   wrapper's main-queue render hop for ordinary scrollback while leaving
   mouse-reporting TUIs on Ghostty's normal app-repaint path.

7. *(cherry-pick, 2026-07-09)* **Upstream "Fix AppKit selection copy leak"
   (Lakr233/libghostty-spm#23, commit `65051461`)** applied — landed upstream
   after our 1.2.4 vendor point, so it is already present in upstream tags
   ≥1.2.6 (drop this note on the next vendor sync). Copy now routes through
   ghostty's `copy_to_clipboard` binding + the wired clipboard callbacks
   instead of a manual `readSelection()` pasteboard write, and
   `supports_selection_clipboard` is enabled.

8. **Synthetic key press exposed** — `TerminalSurface.sendKeyPress(keycode:
   mods:)` + `sendControlKeyPress(keycode:)` (new `public` funcs wrapping the
   internal `sendKeyEvent`) synthesize a press+release through ghostty's key
   encoder, so the bytes honor whatever keyboard protocol the running
   program negotiated (legacy vs kitty). Used by Unpeel's jump-to-bottom
   button to fake ctrl+End — sending raw `ESC [1;5F` via `sendText` was
   mangled into literal text for kitty-protocol TUIs like Claude Code.

9. **Activity-window render pump** (2026-07-09, added for the ghostty-tip
   core) — `TerminalSurfaceCoordinator` now runs a real display link
   (`MSDisplayLink`) for ~1s after every render request, wakeup, or scroll
   input, pumping `ghostty_app_tick` + `refresh` + `draw` every frame.
   The tip core coalesces wakeups/render actions under IO load, so the old
   purely push-driven embedder sat on stale frames: Claude's virtual-scroll
   repaints showed a long delay before scrolling started, and jump-to-bottom
   redraws left a blank screen until a window resize forced a frame. The
   pump *pulls* whatever the core queued instead of waiting to be pushed;
   `TerminalActivityLinkRelay`'s lock gate keeps closed-window frames nearly
   free. `AppTerminalView.scrollWheel` opens the window for mouse-captured
   TUIs (the uncaptured path keeps patch "Immediate macOS scrollback
   repaint" below).

10. **Callback userdata lifetime hardened** — app wakeup callbacks now use a
   retained `TerminalControllerCallbackContext` that weakly references the
   Swift controller and survives until `ghostty_app_free` completes. Surface
   callbacks now retain their `TerminalCallbackBridge` until
   `ghostty_surface_free` completes. Late callbacks during async teardown
   become no-ops instead of rehydrating freed Swift objects.

11. **UIKit committed text is not paste** — software-keyboard and committed
    IME text now bypasses `ghostty_surface_text` for host-managed terminals and
    writes UTF-8 bytes directly to `InMemoryTerminalSession`.
    `ghostty_surface_text` intentionally applies clipboard semantics and wraps
    every call in bracketed-paste markers when mode 2004 is active; OpenCode
    interprets a bracketed whitespace-only input as a request to paste the
    clipboard, so pressing Space pasted the clipboard contents. The explicit
    Paste accessory still uses `surface.sendText` and retains bracketed-paste
    behavior.

12. **Resize presentation: no stretch, same-turn draw** (2026-08-05, macOS) —
    live window resizes showed the previous drawable STRETCHED to the new
    bounds for a frame before the fresh draw landed ("shaking" text).
    Three changes in `Platform/AppKit`:
    - `AppTerminalView.commonInit` sets `metal.contentsGravity = .topLeft`
      so a stale drawable is never scaled to the new bounds.
    - `setFrameSize`/`layout`/`viewDidChangeBackingProperties` call
      `core.renderImmediately()` instead of `core.requestImmediateTick()`,
      drawing in the same runloop turn as the frame change instead of one
      main-queue hop later.
    - `updateMetalLayerMetrics` guards `contentsScale`/`drawableSize`
      writes behind equality checks — `drawableSize` writes invalidate the
      drawable pool even for identical values, and the method wrote the
      same CAMetalLayer twice per resize frame (via `layer` and the cached
      `metalLayer` ivar).

13. **Point→cell mapping honors an explicit grid origin** (2026-08-05,
    macOS) — `AppTerminalView.gridOrigin` (nil = legacy centered-grid
    assumption). Unpeel switched its panes to
    `window-padding-balance = false` (balanced padding re-centers the grid
    every resize frame — the "text shaking" during window drags), so the
    grid's top-left corner is now the fixed padding, which the host passes
    in for `gridCell(atViewX:viewY:)` (cmd-click file detection).

14. **Search actions exposed** (2026-08-12) — the four search action tags
    were falling into the `default:` (log-only) case. Added
    `TerminalSurfaceSearchDelegate` in
    `Sources/GhosttyTerminal/Surface/TerminalSurfaceViewDelegate.swift`
    and dispatch cases for `GHOSTTY_ACTION_START_SEARCH` / `END_SEARCH` /
    `SEARCH_TOTAL` / `SEARCH_SELECTED` in
    `Sources/GhosttyTerminal/InMemory/TerminalCallbackBridge.swift`.
    libghostty runs the search itself (query via the `search:<text>` /
    `navigate_search:*` / `start_search` / `end_search` binding actions
    through the already-public `performBindingAction`); these callbacks
    give the host the "3 of 17" counters. Match totals are collected
    during drawing, so they only fire while the surface renders. Used by
    Unpeel's ⌘F find bar.

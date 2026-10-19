# Native acceptance handoff

These checks are pending. Linux/host-UniFFI evidence does not certify them.
No Mac, Simulator, device, CI dispatch, packaging, push or release was used by
this worker. Keep this change open until the native criteria are recorded.

## iOS

Use the same integrated source revision and an installed iOS runtime; record the
runtime, device, revision, command and xcresult path. The current `ui-tests.yml`
iOS job regenerates the Swift bindings and runs **only `ZeronTests`**. It does not
execute `ZeronUITests` and cannot alone approve U1/U2/U4/U11 interaction.

1. Build the core through `scripts/ios/build-core.sh iphonesimulator`, then verify
   `git diff --exit-code -- apps/ios/Zeron/Core/Generated`. Host-generated bindings
   still require this target-specific drift/link check.
2. Run the existing iOS unit suite and these additional UITests (replace the
   destination with an installed simulator, retain a fresh xcresult bundle):

   ```sh
   xcodebuild test -project apps/ios/Zeron.xcodeproj -scheme Zeron \
     -destination 'platform=iOS Simulator,id=<installed-udid>' \
     -resultBundlePath '<fresh-path>/upstream-ui.xcresult' \
     -only-testing:ZeronUITests/VisualFlowTests \
     -only-testing:ZeronUITests/IPadLayoutTests \
     -only-testing:ZeronUITests/SessionFlowTests/testMathSettings \
     -only-testing:ZeronUITests/SessionFlowTests/testCancelledSwipeBackKeepsStreaming \
     CODE_SIGNING_ALLOWED=NO
   ```
3. Exercise compact tabs ↔ regular split while a Chat streams and while a draft,
   selection, scroll position and search/navigation state are active. Fold/unfold
   repeatedly, rotate, show/hide the keyboard and use uneven safe areas; verify
   transcript readable width and composer alignment. Existing UITests are useful
   regressions, not complete coverage of those transitions.
4. Drag Chats between sections, regular list and pins in both layouts; reorder
   section headers, reconnect a second device and verify neighbor anchors/order.
5. Exercise dashboard controls, 2D/3D/shaders/assets, font/size changes for all six
   math faces, fallback/source selection/copy and navigation teardown. Include a
   real device GPU; Linux software Vulkan cannot certify Metal/UIKit rendering.

## macOS

Use the existing macOS CI/native verification lanes for the exact revision and
record which jobs actually ran. Follow the root Cargo wrapper/space rules; no
parallel Rust gates. `native_diff_font_geometry` needs native font metrics;
`native_preview_focus` requires a logged-in graphical AppKit session and its
explicit opt-in, so Linux compilation is not evidence for it.

With a private demo profile and `ZERON_AUTO_UPDATE=0`, inspect native Luau and
math rendering/selection at narrow/wide widths, Details/lanes ownership, rename
reveal, hover/drag dismissal, Recent Files and Archived confirmation/persistence.
Exercise the fork's native tool/file-link/Mermaid paths and actual steering,
Workers/mailbox and device presence. Check direct Cargo launch and dev bundle
LaunchServices both retain update opt-out; keep the fork feed and installed-app
update behavior. Do not apply a release to the development bundle to test this.

## Explicit GPU limit

Bounded source, VM memory/time, decoded assets, tree/geometry and target reuse
have separate evidence. Those checks do not establish a wall-clock bound for
arbitrary hostile GLSL on a GPU. No intentionally nonterminating shader was run
on the shared host; acceptance must not claim that stronger property.

## Review continuation R4

On the delivered revision, run `ZeronTests/SessionDragTests.swift` in the iOS
Simulator. In Group by Project, drag an actual Chat from a project group to
Pinned and a custom section; with project grouping disabled also exercise
Recent. Ensure project groups remain rejected destinations and search/Archived/header drags remain blocked. The UIKit policy
has behavioral unit coverage added, but no Swift compiler/Simulator is available
on agents-us. Linux source review does not close this acceptance item.

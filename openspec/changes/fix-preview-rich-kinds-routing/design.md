# Design

## Context

See proposal.md for motivation. `Shell::add_file_surface_at` creates the Files editor; `open_preview_surface` registers/deduplicates FilePreview by context/path. The loader resolves explicit local paths across project boundaries. HTML/PDF/video already own isolated native documents; media is loaded by URL rather than copied to memory.

## Goals / Non-Goals

**Goals:** one shared navigation decision, unchanged editor interactions, URL-streamed audio, exact local external targets.

**Non-Goals:** engine/RPC changes, remote binary reads, codec libraries, new dependencies, Host A rendering changes, provider bootstrap/isolation changes.

## Decisions

- **D1:** Before editor registration in `add_file_surface_at`, classify the path and route Html/Pdf/Video/Data/Audio to `open_preview_surface` only if the owner has a known checkout on this device. Use the owner's preview context, not whichever Chat is active. Keep Markdown/Code/Image/Unsupported and remote/unknown owners on the existing path. Existing preview registration owns dedupe.
- **D2:** Extend PreviewKind → LoadedPreview → NativeDocumentAction → DocumentKind with Audio and `open_audio`, mirroring video's `loadFileURL:allowingReadAccessToURL:`. Share video's legacy-JavaScript media policy (content scripts disabled). Skip in-memory byte limits for Video/Audio. Reuse non-macOS textual fallback and native hide/drop/focus behavior. Existing file-link classification accepts everything except Unsupported.
- **D3:** Preserve the Chat context/root; the loader already accepts an absolute relative_path via Path::join, matching transcript preview semantics. External absolute links reach the same shared routing decision, so no second resolution convention or mutation permission is needed.

## Risks / Trade-offs

- WebKit codec support is platform-dependent; recognizing an extension is not proof of decoding every codec.
- macOS native controls require the legacy media JavaScript policy, not document script permission.
- The full app QA recipe is currently forbidden by scripts/AGENTS.md because provider configuration isolation is unproven. Run focused tests and safe isolated native checks, explicitly record full app screenshots as notProven and leave that acceptance task open.
- LSP reports no server available for Rust in this session; use Trail's indexed references and exact source spans.
- Cargo verification runs once in an isolated wrapper; tests are authored before behavior changes, and cheap standalone red probes are preferred before the final grouped Cargo run.

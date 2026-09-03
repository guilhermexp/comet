## Context

See `proposal.md` for motivation. Rust 1.98 checks cfg values emitted inside dependency macros. GPUI asks the app `AssetSource` for two fixed font paths. Cargo reports future-incompatible code in transitive `block` and `proc-macro-error2` versions.

## Goals / Non-Goals

**Goals:** remove only the known diagnostics and preserve detection of new ones; reuse embedded assets; keep dependency compatibility local and reviewable.

**Non-Goals:** update Comet, the updater, the pinned GPUI revision, or broadly silence compiler warnings.

## Decisions

- **D1:** Add `cfg(feature, values("cargo-clippy"))` to the UI crate's lint check-cfg declaration while leaving `unexpected_cfgs` at warning level. A crate-wide allow was rejected.
- **D2:** Map GPUI's two virtual font paths to Comet's existing embedded Geist sans and Geist Mono bytes and include them in `AssetSource::list`. Adding IBM Plex/Lilex binaries was rejected because it adds unnecessary assets and licensing surface.
- **D3:** First attempt compatible Cargo resolution to fixed transitive releases. If constraints prevent it, vendor minimal source-equivalent compatibility patches under `third_party` with provenance, license, and dedicated `[patch.crates-io]` entries. Updating GPUI or suppressing the future-incompat report was rejected.

## Risks / Trade-offs

- [Virtual font names differ from font metadata] → verify actual SVG text rendering in the headed smoke test.
- [Dependency patch drifts] → keep patches minimal, documented, and removable once constraints admit fixed releases.
- [Concurrent asset edits overlap] → patch only the `AssetSource` match/list and its focused tests after re-reading the file.

## Migration Plan

No runtime data migration. Cargo lock changes are reversible; local patches can be removed when compatible fixed releases become resolvable.

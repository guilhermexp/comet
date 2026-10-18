# Conflict decisions (working integration)

Ancestor 75b72602; fork e750f810; real merge target c526d6f7. Planning parent d7df75f8. The following lists every initially conflicted path; compile repairs and the final additions audit are recorded separately.

| Path | Resolution |
| --- | --- |
| Cargo.toml | 1.0.7, vendored gpui patches and fork profiles retained; add visual/GPU/zatex and optimized visual profiles; dependency versions centralized. |
| Cargo.lock | Regenerate from fork lock against merged manifests, preserving existing locked packages; no upstream version replacement. |
| THIRD_PARTY_NOTICES.md | Union Blobatar attribution and all six math font notices/licenses. |
| apps/ios/Zeron/Core/Generated/zeron_core.swift | Regenerated with UniFFI from the built merged Linux core; normalized using the iOS script rule. The hand candidate had identical nonblank content; no hand-generated file is treated as native proof. iOS target regeneration/link remains pending. |
| crates/doc/src/parts.rs | Add Visual beside Text; keep fork reasoning fields, completion/duration, file preview and subagent lifecycle. |
| crates/doc/src/schema.rs | Visual mapping plus fork reasoning/answers semantics; retain both opening-tail and new visual round-trip tests. |
| crates/engine/src/repos.rs | New NUL-safe all-checkout lookup for workspace authorization, with the fork timeout; deletion keeps its separate bounded ownership/branch semantics. |
| crates/harness/src/opencode/mod.rs | Remove adapter InputResolved emission; engine remains sole lifecycle owner. Retain v2 form routes and all v1 handling. |
| crates/markdown/src/mend.rs | New indexed delimiter matcher plus fork maximum-open-delimiter bound and nested-fence preservation. |
| crates/markdown/src/parser.rs | Math blocks/budgets/streaming, plus fork file paths and recursive fence protection; retain both helpers. |
| crates/mobile/src/layout/markdown.rs | Final upstream formula/table-area/source fallback and baseline adjustments; preserve fork attachment handling. |
| crates/mobile/src/layout/mod.rs | Shared static font faces, math inputs/generation and visual slot/version measurements. |
| crates/mobile/src/layout/rows.rs | Visual rows and formula-aware heap accounting, preserve fork input fields. |
| crates/mobile/src/layout/tests.rs | Keep new visual Worker constructor and all existing/new tests. |
| crates/mobile/src/layout/tools.rs | Math source in thought/code detail path. |
| crates/rpc/src/lib.rs | Reject duplicate upstream method table; PublishVisual registered in method.rs only, local-only. |
| crates/ui/Cargo.toml | zatex bundled alongside root-owned html5ever dependency. |
| crates/ui/src/change_requests.rs | Keep SidebarIcon plus SessionRow surface; suppress duplicate tooltip on session rows, preserve fork device routing. |
| crates/ui/src/changes.rs | Retain process-wide native text backend. New math test helper uses that same backend. |
| crates/ui/src/files/mod.rs | Retain fork Changes/Create/search state; upstream removal applies only to obsolete explorer lanes, already absent here. |
| crates/ui/src/lib.rs | Keep url_chips and add visuals module. |
| crates/ui/src/markdown/render.rs | Formula slots/offsets/bounds/glyph overlay and source copy combined with fork prefix invalidation, nested IDs, file links, rounded code backgrounds, hard breaks, table geometry/type and source preview. No old fixed-width thought presenter restored. |
| crates/ui/src/pull_request_detail.rs | Add math style to fork settled options builder; retain media/copy. |
| crates/ui/src/pull_request_media.rs | Keep settled-options test setup with default math. |
| crates/ui/src/settings.rs | Union computer-use/scope and collapse settings with math settings; preserve central writer and add Archived ownership. |
| crates/ui/src/shell.rs | Union search warming/hover/section drag state; side-chat rename reveals lanes, removing obsolete tab editor; fork menu/actions/panels retained. |
| crates/ui/src/shell/panel_interaction_tests.rs | Union all fork geometry/watch lifecycle tests and upstream lane rename/scroll tests. |
| crates/ui/src/shell/session_info.rs | Fork 16px gutters and Details metadata ownership; add lanes and scoped totals watch, release while hidden. Starter actions live in lanes and existing Details header. |
| crates/ui/src/shell/spaces.rs | Add selectors/new controls while retaining fork row projection, mentions, pin transfer and grouping. |
| crates/ui/src/shell/tabs.rs | Keep fork titlebar geometry/content-sized controls; adapt new running side-chat + subagent count without introducing a second panel host. |
| crates/ui/src/transcript.rs | Add Visual rows/renderer and math style invalidation to fork renderer. Reject resurrected upstream compact/thought/user rendering that replaced fork behavior. Retain fork tool tree, wizard, lifecycle lines, mentions, file cards and native selection. Release departed visual frames. |
| scripts/dev-demo.sh | Fanout fixture + fork Worker fixture, scalar Bash 3.2 variables, both port/profile override conventions, temporary demo project folders. |

Compile reconciliation: keep parent tool-use IDs when opening subagent surfaces, use fork ProjectedRow source spans, use composite-owner row remeasurement on visual layout changes, export Visual reference metadata, keep Visual outside activity folds, adapt fork Window argument in the takeover/rename regression and extend FlatText fixtures with math slots. Full failures remain under logs/integration-compile-{1,2,3}.log.

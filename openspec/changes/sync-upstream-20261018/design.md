## Integration decisions

Real no-ff merge of c526d6f7 after formatting our side. Resolve every conflict against ancestor/fork/upstream; no wholesale side selection. PR #24 is stale diagnostic evidence at 6ac88599 and remains untouched. Version-only commits remain in ancestry without changing 1.0.7. Incoming additions audit compares only 75b72602..c526d6f7, including final-upstream fixes.

### U1: ADAPT

Native sandboxed Luau visuals, controls/charts/tables, 2D/3D/shaders/assets with bounded validation, off-main GPU, reuse and teardown; root-only MCP grants. Source 8a0fded8; exact incoming files are recorded by git diff and per-commit manifests in evidence.

### U2: ADAPT

zatex inline/display math, six fonts and persisted settings, bounded parser/layout and streaming fallback; retain file links/Mermaid/selection. Source c02739da; exact incoming files are recorded by git diff and per-commit manifests in evidence.

### U3: ADAPT

Side-chat/subagent lanes, folded finished rows, bounded scroll/counts/rename/harness marks and checkout totals; reconcile Details Workers ownership and existing new/fork buttons without duplicate activity. Source 59182488; exact incoming files are recorded by git diff and per-commit manifests in evidence.

### U4: TAKE

Convergent section header drag order, iOS cross-section/pin drag, neighbor anchors and advertised capability. Source 26c8e840; exact incoming files are recorded by git diff and per-commit manifests in evidence.

### U5: ADAPT

Dwell/warm hover card, dismiss on scroll/drag/menu, once-per-frame lifecycle and metadata; retain mentions and fork rows. Source 840fda74; exact incoming files are recorded by git diff and per-commit manifests in evidence.

### U6: ADAPT

OpenCode v2 forms normalize to field-keyed input panels and form route; v1 works and engine owns lifecycle. Source 9609f994; exact incoming files are recorded by git diff and per-commit manifests in evidence.

### U7: ADAPT

Cursor Plus catalog/defaults on fresh and switched runs, warm catalog; protect dev bundles with fork updater/feed. Source 3b2f7802; exact incoming files are recorded by git diff and per-commit manifests in evidence.

### U8: ADAPT

Recent Files reduces authorization Git round trips with concurrent checkout/identity lookup and NUL-safe worktree parsing; fork unified search/index ownership remains. Source 2a209cc6; exact incoming files are recorded by git diff and per-commit manifests in evidence.

### U9: ADAPT

Persist sidebar Archived visibility setting. Source 81e04e1c; exact incoming files are recorded by git diff and per-commit manifests in evidence.

### U10: ADAPT

Bulk archived Chat deletion with explicit confirmation and device/sync routing. Source 4141c761; exact incoming files are recorded by git diff and per-commit manifests in evidence.

### U11: TAKE

Adaptive iOS split/tab shell, fold state, uneven safe areas, transcript/composer alignment; native acceptance pending. Source c526d6f7; exact incoming files are recorded by git diff and per-commit manifests in evidence.

## Verification plan

Use direct Cargo with one shared task target and limited jobs as explicitly required on agents-us. Retain baseline failures and exact revision/exit logs. Run upstream regressions plus fork grants/Workers/steer/search/markdown regressions, workspace/all-target integration, actual isolated app under Xvfb/software Vulkan, synthetic bulk deletion, selection/copy and hostile math/visual budgets. Read actual CI definitions but do not dispatch. Linux cannot certify AppKit/UIKit; record precise handoff and keep acceptance tasks open. No archival before native criteria are verified.

## Review correction boundaries R1–R5

Native Luau-to-Rust parsing must bound visits independently from constructed
nodes and check a native deadline. Text and captured logs reserve cumulative
bytes before copying; collection entries (including empty values) consume work.
Model import preflights input, buffer/image counts and aggregate decoded/encoded
bytes before import/copy/decode; identical image views share existing Arc backing.
Cursor cache metadata must be committed with its accepted credential-scoped model
snapshot, not mutated by speculative discovery. iOS drag source eligibility is
independent from destination eligibility, retaining search/archive restrictions.
Reproductions use scaled inputs under CPU/address-space/time limits; no full
exponential DAG/OOM payload or infinite GPU shader. Native acceptance stays open.

R2 closure: retained text admission includes axes labels/defaults, implicit
tabs/link copies, numeric root children and canvas text. Reuse existing
pre-copy byte charging; projection copies derive from already admitted data.

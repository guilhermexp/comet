# Tasks

## Fasing

| Fase | U-IDs | Seções | Depends on | Audit state | Audited commit | Entrega | UAT mode |
|---|---|---|---|---|---|---|---|
| F1 | C1-C4 | §1 | — | passed | fce64b7d | Pure renderer: three formats + artifact index + filename | none — unit |
| F2 | C5-C7 | §2 | F1 | human_needed | — | Six actions live in the chat menu, end to end | human-driven |

## 1. The renderer (pure, no gpui)

**must_haves:** the three formats are functions of one intermediate and cannot disagree; no raw tool input or verbose output appears in any format; the artifact index is built only from fields the transcript already carries; filenames are safe on every platform.

- [x] C1 New `crates/ui/src/chat_export.rs` with `ExportDoc` — chat metadata (id, title, branch, cwd, exported-at) plus messages plus `Vec<Artifact>` — built in ONE pass over `&[SessionMessageEntry]`. Artifacts are file writes (`diff_stats`/`file_preview`), heavy outputs (`output_bytes` over the reference's 16 KiB budget) and subagents (`subagent_ref`), each addressed by ordinal `(message_ix, part_ix)`. files: `crates/ui/src/chat_export.rs`, `crates/ui/src/lib.rs`. verify: `cargo test -p zeron-ui chat_export`.
- [x] C2 Markdown renderer: title heading, `**Exported:** / **Project:** / **Branch:**` lines, `---`, `## Artifacts` (or `_None._`), then `### **You**` / `### **Assistant**` per message. Tool parts render one line each — Bash as a fenced `bash` block with the command, Write/Edit as `> Modified: \`path\``, Read as `> Read: \`path\``, anything else as `> *Used X tool*`. Reasoning parts and verbose tool output never appear. files: `crates/ui/src/chat_export.rs`. verify: `cargo test -p zeron-ui chat_export`.
- [x] C3 Text and JSON renderers over the SAME `ExportDoc`: text mirrors the markdown spine without markup (`You:` / `Assistant:`, `[used X tool]`, `ARTIFACTS:` block); JSON emits `{exportedAt, chat, artifactIndex, messages}` with the doc's parts intact, indented 2. Add a test asserting all three name the same artifact set and the same message count — the D-04 invariant. files: `crates/ui/src/chat_export.rs`. verify: `cargo test -p zeron-ui chat_export`.
- [x] C4 Filename builder: sanitize the chat title (invalid chars and whitespace → `_`, collapse runs, trim, cap 100, fall back to `chat`), append `-` + the first 8 chars of the chat id + the format's extension. Unit-test the empty title, a title of only invalid chars, and a title over the cap. files: `crates/ui/src/chat_export.rs`. verify: `cargo test -p zeron-ui chat_export`.

## 2. The six actions

**must_haves:** all six work from any row in the list, not just the selected one; every outcome — success and failure — is visible; nothing is written outside the Downloads directory.

- [x] C5 Resolve a Chat Transcript by chat id: the selected chat reads `AppState::transcript`; any other opens a transient `WatchDocMessages`, takes the first reset frame and drops the subscription. No new RPC method. files: `crates/ui/src/shell.rs`, `crates/ui/src/state.rs`. verify: `cargo test -p zeron-ui shell` + `cargo test -p zeron-ui state`.
- [x] C6 Wire download and copy: download writes the rendered bytes into the user's Downloads directory under the C4 filename; copy writes the same string via `cx.write_to_clipboard`. Both report through `Shell::sidebar_notice` — success names the file or says it was copied, failure names the reason. `notify::post` is NOT used (D-07). files: `crates/ui/src/shell.rs`. verify: `cargo test -p zeron-ui shell` over the pure outcome-message decision + visual check.
- [x] C7 Add the flat `EXPORT` section to `chat_menu` — `menu_separator`, `menu_heading("Export")`, then the six rows — between Archive and Delete, and widen the menu card for the longest label. Then the DOX pass: `crates/ui/AGENTS.md` gains the export invariants, and the root `AGENTS.md` glossary pointer gains the Chat-vs-Session line. files: `crates/ui/src/shell.rs`, `crates/ui/AGENTS.md`, `AGENTS.md`. verify: `cargo test -p zeron-ui` + visual check of the menu.

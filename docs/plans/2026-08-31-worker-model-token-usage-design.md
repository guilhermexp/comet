# Worker Model and Token Usage Design

**Status:** Approved on 2026-08-31.

## Goal

Show the effective OMP model and the tokens processed by that model inside each
CLI Worker row, using the same compact disclosure pattern already used by OMP
subagents.

## Decisions

- **D1 — Provider-owned evidence:** OMP session JSONL is the only authority for
  model and token usage. The terminal screen, launch preset, and global OMP
  config are not telemetry sources.
- **D2 — Exact Worker binding:** the bundled OMP lifecycle extension publishes
  `session_id` and `provider_transcript_path` from OMP's `sessionManager` on its
  existing lifecycle hook. The Worker Session remains identified by the hook
  URL; the payload identifies the provider conversation owned by that Worker.
- **D3 — Effective model identity:** model identity is
  `provider/model:thinking`, derived in JSONL order from `model_change`,
  `thinking_level_change`, and assistant `message` records. Missing thinking
  level omits the suffix.
- **D4 — Per-model accounting:** each assistant message contributes its
  `usage.totalTokens` exactly once to the effective model that produced it.
  Session total is the saturating sum of the per-model totals. Reasoning tokens
  are already contained in provider usage and are never added a second time.
- **D5 — Durable local projection:** normalized telemetry is written atomically
  inside the Worker's private Session directory after the OMP `Stop`
  (`agent_end`) hook. The Details sidebar reads this small projection; it does
  not scan provider JSONL during render or every bootstrap refresh.
- **D6 — Additive wire:** the Host bootstrap adds optional `modelUsage` and
  `totalTokens` fields. Older Sessions and non-OMP runtimes continue decoding
  with no telemetry.
- **D7 — Existing interaction language:** a Worker with telemetry becomes
  independently expandable using the widget's existing stable expansion map.
  Its collapsed metadata shows total tokens and the current model. Expansion
  lists every model identity with its own token total, current model first.
- **D8 — Honest degradation:** missing, malformed, untrusted, or temporarily
  incomplete telemetry leaves the current `command` subtitle intact. It never
  blocks Worker lifecycle, terminal access, bootstrap, or the rest of the
  widget.

## Data Flow

```text
OMP session JSONL
  model_change + thinking_level_change + assistant message.usage.totalTokens
         │
         ▼
bundled OMP lifecycle extension ── session id/path ──► Comet hook ingress
                                                            │
                                                            ▼
                                            runtime telemetry normalizer
                                                            │
                                                            ▼
                                  private Worker telemetry marker (atomic)
                                                            │
                                                            ▼
                       Host bootstrap → WorkersSession → ChatWorkerRow → gpui
```

## UI

Collapsed Worker:

```text
⌄  WTK a1: audit release gates                         ◯
   216.6k tokens        google-antigravity/gemini-3.7-flash:medium
```

Expanded Worker with a model switch:

```text
   Models
   ● google-antigravity/gemini-3.7-flash:medium   216.6k tokens
   ● openai-codex/gpt-5.6-sol:high                 42.1k tokens
```

The chevron toggles telemetry only. Clicking the rest of the Worker row keeps
opening the Worker terminal. Model identity truncates before token text; token
text remains visible.

## Failure and Compatibility Rules

- Canonicalize a hook-reported transcript path and accept it only beneath the
  OMP session root. Reject symlink escape and non-JSONL files.
- Ignore malformed JSONL lines and messages without a valid non-negative
  `totalTokens`; keep all valid records before and after them.
- Parse on lifecycle refresh, outside gpui render. A parse failure preserves
  the last valid marker.
- New wire fields are optional and default to empty/`None` throughout the Rust
  frontier.
- The telemetry marker is device-local Worker state. It never enters Loro,
  Chat Transcript, edge sync, Managed Provider Usage, or API billing UI.
- Cost fields are deliberately ignored: subscription-backed providers do not
  make the JSONL estimate equivalent to an invoice or quota debit.

## Non-goals

- Adding model selection to `launch_worker`.
- Estimating tokens from text or terminal output.
- Changing Managed Provider Usage.
- Exporting Worker telemetry in Chat Transcript Export.
- Adding telemetry for runtimes whose provider-owned session format has not
  been specified and tested.

## Verification

- Unit fixtures prove OMP model/thinking transitions, per-model sums,
  saturation, malformed-line tolerance, and trusted-path enforcement.
- Hook ingress integration proves provider session metadata is persisted for
  the URL-addressed Worker without confusing the two session identities.
- Frontier tests prove optional wire decode and projection ordering.
- UI unit tests prove formatting, expansion identity, fallback, and
  multi-model ordering.
- Native gpui QA proves the collapsed/expanded layout, truncation, status
  alignment, and that the row still opens the correct terminal.

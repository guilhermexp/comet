# Idle Session Recap — Design Document

## 1. Executive Summary & Parity Goal

This design documents the implementation of the **Idle Session Recap** in Comet, achieving exact functional and visual parity with the implementation in `orchestrator.dev` (commit `c54828fb`).

When a chat sits idle without active streaming, context compaction, or composer draft text for a configured delay (default 240 seconds, clamped between 60s and 600s), the system derives a concise, one-line "where things stand" recap from the stored transcript tail and displays it in the **Workspace** widget of the **Details Sidebar**, directly beneath the *"Projects worked"* section:

$$\text{※ recap: <summary text>}\quad\text{HH:MM}$$

### Core Invariants
1. **Zero Session Overhead:** The recap is derived text generated via an isolated one-shot model execution. It never touches the chat's live session, adds no turns, appends no events to the session CRDT document, and consumes no chat history context.
2. **Epoch-Based Staleness Guard:** Staleness is gated by message count (`epoch = transcript.len()`), not elapsed time alone. A recap covers an exact epoch; any new turn or message immediately invalidates and clears stale recaps so obsolete context is never shown.
3. **Durable Persistence Across Restarts:** Recaps survive application restart within `DetailsSidebarPreferences` in `ui-settings.json`. The user returning to any chat across projects immediately sees where work stopped without triggering redundant model calls on unchanged transcripts.
4. **Hydration-Safe Branch Evaluation:** The idle evaluation policy checks epoch matching *before* draft or empty transcript checks, preventing hydration races (where the UI mounts before messages load from storage) from deleting valid persisted recaps.
5. **Bounded Retention:** Recaps are capped at 50 entries, expire after 24 hours (`IDLE_RECAP_MAX_AGE_MS`), and future-dated timestamps from clock skew are discarded.

---

## 2. Architecture & Seams

```
┌────────────────────────────────────────────────────────────────────────┐
│ UI Layer (crates/ui)                                                   │
│                                                                        │
│  Active Chat / Composer       evaluate_idle_recap                      │
│   - is_streaming          ─────────────────────────► IdleRecapAction   │
│   - has_draft                                          │               │
│   - message_count (epoch)                              │               │
│                                                        ▼               │
│  Idle Timer (cx.spawn) ──[after delay]──► RPC: GenerateChatRecap       │
│                                                 │                      │
│  Workspace Widget (DetailsSidebar)             │                      │
│   - IdleRecapRow ◄── DetailsSidebarPreferences ◄┘                      │
└─────────────────────────────────────────────────┼──────────────────────┘
                                                  │ Typed RPC
┌─────────────────────────────────────────────────┼──────────────────────┐
│ Engine Layer (crates/engine + crates/rpc)       ▼                      │
│                                                                        │
│  GenerateChatRecap handler                                             │
│   1. Read SessionDoc transcript tail (select_recap_transcript)         │
│   2. Construct prompt with language pin (build_recap_prompt)           │
│   3. One-shot execution via Harness with cheapest_model (Haiku/Flash)  │
│   4. Clean & sanitize response (validate_recap)                        │
└────────────────────────────────────────────────────────────────────────┘
```

### 2.1 RPC Protocol (`crates/rpc`)

Add a new typed RPC method in `crates/rpc/src/method.rs`:

```rust
GENERATE_CHAT_RECAP / GenerateChatRecap = "GenerateChatRecap" {
    params: GenerateChatRecapParams,
    reply: GenerateChatRecapReply,
    local_only: true,
    deadline_secs: 45,
}
```

* `GenerateChatRecapParams`: `{ chat_id: String }`
* `GenerateChatRecapReply`: `{ recap: Option<String> }`
* `local_only: true`: executed by the local engine host owning the chat; not forwarded over device relay.

### 2.2 Engine Recap Pipeline (`crates/engine/src/recap.rs`)

* **`select_recap_transcript`:**
  * Walks the stored `SessionDoc` messages in reverse chronological order.
  * Projects only `user` and `assistant` textual content. Tool calls, tool results, and system messages are excluded.
  * Limits: maximum 40 messages, total character budget of 6,000 characters, and up to 800 characters per individual message (truncated at a word boundary).
  * Returns the selected entries in chronological order.
* **`build_recap_prompt`:**
  * Embeds the chat's overall goal or title as an anchor when available.
  * Instructs the model:
    * Under 40 words, 1–2 plain sentences, no markdown.
    * Lead with the overall goal and current task, followed by the one next action.
    * Skip root-cause narrative, internals, secondary to-dos, and em-dash tangents.
    * Write in the **exact same language** used in the conversation.
* **`run_recap_model`:**
  * Resolves the chat's harness from `HarnessRegistry`.
  * Selects `cheapest_model` (Haiku, Flash, Mini, Nano) matching the existing pattern in `crates/engine/src/titles.rs`.
  * Runs a `RunRequest` with `SandboxLevel::ReadOnly`, `ReasoningLevel::Minimal`, `auto_approve: true`, and 45s timeout.
  * Retries on short backoff delays (250ms, 1000ms).
* **`validate_recap`:**
  * Strips code fences, markdown asterisks/underscores, bullet markers, and surrounding quotes.
  * Iteratively strips model preambles (*"Sure:"*, *"Here's a recap:"*, *"Summary -"*, etc.).
  * Truncates at a word boundary to a maximum of 280 characters (`RECAP_MAX_CHARS`). Returns `None` if the text is empty or invalid.

---

## 3. UI Layer & Sidebar Integration (`crates/ui`)

### 3.1 Pure Policy & Entry Types (`crates/ui/src/details_sidebar/idle_recap.rs`)

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdleRecapEntry {
    pub text: String,
    pub epoch: usize,
    pub generated_at: u64, // Unix timestamp in milliseconds
}

pub struct IdleRecapState<'a> {
    pub enabled: bool,
    pub can_generate: bool,
    pub is_streaming: bool,
    pub is_compacting: bool,
    pub has_draft: bool,
    pub message_count: usize,
    pub entry: Option<&'a IdleRecapEntry>,
    pub delay_seconds: u64,
}

pub enum IdleRecapAction {
    None,
    Clear,
    Keep,
    Arm { delay_ms: u64 },
}

pub fn evaluate_idle_recap(state: &IdleRecapState<'_>) -> IdleRecapAction { ... }
pub fn prune_idle_recaps(entries: &mut HashMap<String, IdleRecapEntry>, now: u64) { ... }
```

#### Evaluation Rules
1. If `!enabled` or `!can_generate`: return `Clear` if an entry exists, else `None`.
2. If `is_streaming` or `is_compacting`: return `Clear` if an entry exists, else `None`.
3. If `entry.epoch == message_count`: return `Keep` (do not regenerate).
4. If `message_count == 0`: return `Keep` if an entry exists (transient hydration safeguard), else `None`.
5. If `has_draft`: return `Clear` if an entry exists, else `None`.
6. Otherwise: return `Arm { delay_ms: clamped(delay_seconds, 60, 600) * 1000 }`.

### 3.2 State Persistence & Settings (`crates/ui/src/settings.rs` & `view.rs`)

* Add `idle_recaps: HashMap<String, IdleRecapEntry>` to `DetailsSidebarPreferences`.
* Add preferences:
  * `idle_recap_enabled: bool` (default `true`)
  * `idle_recap_delay_seconds: u64` (default `240`)
* Pruning on load/save: retains the 50 newest entries within a 24-hour window, discarding malformed or future timestamps.

### 3.3 Details Sidebar Rendering (`crates/ui/src/details_sidebar/view.rs`)

In `render_details_sidebar`, inside the `Workspace` widget card (`workspace-widget`), immediately beneath the `worked_section` (`Projects worked`):

```rust
if let Some(entry) = self.sidebar.idle_recap_for(&context.key) {
    let clock_text = format_recap_timestamp(entry.generated_at);
    let recap_row = div()
        .mt(px(4.0))
        .pt(px(6.0))
        .border_t_1()
        .border_color(theme.border.opacity(0.50))
        .flex()
        .items_end()
        .gap(px(8.0))
        .px(px(2.0))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(12.0))
                .italic()
                .text_color(theme.text_muted.opacity(0.85))
                .child(format!("※ recap: {}", entry.text)),
        )
        .child(
            div()
                .shrink_0()
                .text_size(px(10.0))
                .text_color(theme.text_muted.opacity(0.50))
                .child(clock_text),
        );
    workspace_body = workspace_body.child(recap_row);
}
```

---

## 4. Error Handling & Edge Cases

| Edge Case | Handling |
| :--- | :--- |
| **Model request timeout or error** | Silent failure. Memoize the `failed_epoch` in session memory so the app does not loop requesting the same epoch. |
| **User starts typing while timer is running** | Timer cancelled immediately via `has_draft` state change. |
| **User starts typing while RPC is in-flight** | Upon RPC completion, `is_streaming`, `has_draft`, and `epoch` are re-checked. If the user resumed work, the reply is silently discarded. |
| **Chat switch while timer is running** | Context change cancels the timer for the previous chat and arms the timer for the newly active chat. |
| **Workspace hydration race on app startup** | `message_count == 0` is treated as unhydrated transcript rather than empty; valid persisted recaps are preserved. |
| **Offline / no model credentials** | RPC returns `{ recap: None }` cleanly. No UI error banner; the widget simply omits the line. |

---

## 5. Testing Strategy

1. **Unit Tests in `crates/engine/src/recap.rs`:**
   * `select_recap_transcript` bounds: verifies 40 messages, 6000 character limit, 800 character per-entry clamp, word boundary slicing, exclusion of tool parts.
   * `build_recap_prompt`: language matching instructions, goal anchor presence/absence.
   * `validate_recap`: preamble stripping, markdown elimination, max length truncation.
2. **Unit Tests in `crates/ui/src/details_sidebar/idle_recap.rs`:**
   * `evaluate_idle_recap`: exhaustive state matrix (streaming, compacting, draft, matching epoch, unhydrated 0-count, armed delay clamping).
   * `prune_idle_recaps`: age expiration (>24h), capacity cap (50 entries), future clock skew dropping.
3. **Integration / E2E Verification:**
   * Verify via `scripts/dev-demo.sh`: chat in demo workspace, observe sidebar card display after simulated idle delay, check persistence across demo restarts.

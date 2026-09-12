# Design: Idle Session Recap in Comet

## Architecture Overview

O design espelha o contrato e a separação de responsabilidades do `orchestrator.dev`:

```
┌─────────────────────────────────────────────────────────────┐
│ UI (crates/ui)                                              │
│                                                             │
│  Composer / Active Chat       evaluate_idle_recap           │
│   - is_streaming          ─────────────────────► Action     │
│   - has_draft                                      │        │
│   - message_count (epoch)                          │        │
│                                                    ▼        │
│  Idle Timer (cx.spawn) ──[after delay]──► RPC Call          │
│                                              │              │
│  Workspace Widget (DetailsSidebar)          │              │
│   - IdleRecapRow ◄── DetailsSidebarPreferences ◄┘           │
└──────────────────────────────────────────────┼──────────────┘
                                               │
┌──────────────────────────────────────────────┼──────────────┐
│ Engine (crates/engine)                       ▼              │
│                                                             │
│  GenerateChatRecap handler                                  │
│   1. Read SessionDoc (select_recap_transcript)              │
│   2. Build Prompt with language pin (build_recap_prompt)    │
│   3. One-shot via Harness cheapest_model (run_recap_model)  │
│   4. Clean & sanitize response (validate_recap)             │
└─────────────────────────────────────────────────────────────┘
```

## Data Types

### RPC (`crates/rpc/src/method.rs`)
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateChatRecapParams {
    pub chat_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateChatRecapReply {
    pub recap: Option<String>,
}
```

### UI (`crates/ui/src/details_sidebar/idle_recap.rs`)
```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdleRecapEntry {
    pub text: String,
    pub epoch: usize,
    pub generated_at: u64,
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
```

## Evaluation Policy & Invariants

```rust
pub fn evaluate_idle_recap(state: &IdleRecapState<'_>) -> IdleRecapAction {
    let has_entry = state.entry.is_some();
    if !state.enabled || !state.can_generate {
        return if has_entry { IdleRecapAction::Clear } else { IdleRecapAction::None };
    }
    if state.is_streaming || state.is_compacting {
        return if has_entry { IdleRecapAction::Clear } else { IdleRecapAction::None };
    }
    if let Some(entry) = state.entry
        && entry.epoch == state.message_count
    {
        return IdleRecapAction::Keep;
    }
    if state.message_count == 0 {
        return if has_entry { IdleRecapAction::Keep } else { IdleRecapAction::None };
    }
    if state.has_draft {
        return if has_entry { IdleRecapAction::Clear } else { IdleRecapAction::None };
    }
    let clamped_seconds = state.delay_seconds.clamp(60, 600);
    IdleRecapAction::Arm { delay_ms: clamped_seconds * 1000 }
}
```

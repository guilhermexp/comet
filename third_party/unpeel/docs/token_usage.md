# Token Usage Counter

## Goal

Add a local token counter for Unpeel sessions. This should be a usage visibility
feature, not a billing dashboard.

The first version should answer:

- How many tokens did I use today, this week, and this month?
- Which providers, projects, and sessions used the most tokens?
- How much of that was input, output, cached input, or reasoning output where
  the provider exposes those fields?

## MVP

- Show total tokens by day, week, and month.
- Break down totals by provider.
- Break down totals by project and session.
- Show current-session token total in the session detail/sidebar when available.
- Show provider-specific token fields only when they are real:
  - Claude: input, output, cache creation, cache read.
  - Codex: input, cached input, output, reasoning output, total.
- Show "not available" for providers without a reliable local usage source.
- Keep Codex rate-limit percentage separate from token totals.

Avoid cost estimates in the first version. Token counts are defensible from
local files; cost depends on changing provider pricing, subscription plans,
cache pricing, and fields that some CLIs do not expose.

## Data Sources

Unpeel-owned session metadata already gives us the join keys:

- `~/.unpeel/app-sessions/<session-id>/manifest.json`
- `provider_session_id`
- `provider_transcript_path`
- session command, project id, cwd, created timestamp, and title

Provider transcript sources:

- Claude: `~/.claude/projects/<cwd-as-dashes>/<provider-session-id>.jsonl`
- Codex: `~/.codex/sessions/YYYY/MM/DD/*.jsonl`

Unpeel already captures provider metadata from hooks and writes it into the
session manifest. Codex usually provides a direct transcript path. Claude can
usually be resolved by provider session id or cwd plus session creation time.

## Provider Parsing

### Claude

Claude assistant rows include `message.usage`, with fields such as:

- `input_tokens`
- `output_tokens`
- `cache_creation_input_tokens`
- `cache_read_input_tokens`
- `server_tool_use`
- `service_tier`
- `message.model`
- `requestId`
- `message.id`
- `timestamp`

Important: Claude can repeat the same `message.usage` across multiple rows for
one assistant response, such as thinking, text, and tool-use blocks. Do not sum
every row blindly. Dedupe by `requestId` when present, with `message.id` as a
fallback.

### Codex

Codex JSONL rows include `event_msg` payloads where:

- `payload.type == "token_count"`
- `payload.info.total_token_usage`
- `payload.info.last_token_usage`
- `payload.info.model_context_window`
- `payload.rate_limits`

Important: Codex emits cumulative totals repeatedly. For a session total, use
the latest or max `total_token_usage` value, not the sum of all token-count
events. `last_token_usage` can be used for per-turn deltas, but repeated events
must still be deduped carefully.

## Storage

Create a normalized cache owned by Unpeel, for example:

- `~/.unpeel/usage.sqlite`

Suggested tables:

- `usage_sources`
  - provider
  - transcript path
  - modified timestamp
  - last parsed offset
  - parser version
- `usage_events`
  - provider
  - unpeel session id
  - provider session id
  - project id
  - cwd
  - timestamp
  - model
  - request id / turn id
  - input tokens
  - cached input tokens
  - cache creation tokens
  - cache read tokens
  - output tokens
  - reasoning output tokens
  - total tokens
- `usage_snapshots`
  - provider
  - unpeel session id
  - latest cumulative totals
  - latest rate-limit fields where available

The dashboard should read from the normalized cache, not directly from provider
files on every render.

## UI Notes

- Name the feature "Token Usage" or "Token Counter".
- Keep the first UI compact and utilitarian.
- Primary views:
  - Overview: total tokens over time.
  - Providers: Claude vs Codex.
  - Projects: project totals.
  - Sessions: session-level table.
- Do not imply these numbers are billable cost.
- Label missing providers clearly: "Token data unavailable".

## Privacy And Robustness

- Parse local files only.
- Do not upload transcript contents.
- Store token metadata, not message text.
- Treat provider file formats as unstable private contracts.
- Version parser behavior so we can reindex when field handling changes.
- Fail soft when a provider changes its JSONL shape.

## Later

- Add adapters for Grok, OpenCode, Gemini, Amp, Cursor Agent, and Copilot if
  they expose reliable local usage data.
- Add per-turn charts after session totals are solid.
- Add optional estimated cost only if pricing metadata is explicit, local, and
  clearly labeled as an estimate.

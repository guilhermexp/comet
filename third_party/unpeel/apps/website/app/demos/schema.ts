/**
 * Normalized demo-transcript format.
 *
 * The website's demo terminals can optionally replay a *real* CLI session
 * (trimmed JSONL from ~/.claude, ~/.codex, …) as a streaming animation, instead
 * of the hand-authored static mock. Provider JSONL is verbose and
 * provider-specific, so it is normalized at build time (see
 * `app/demos/normalize.ts` + `scripts/build-demo-transcripts.ts`) into this
 * compact, provider-agnostic shape. The committed artifact the UI imports is the
 * generated JSON (`app/demos/generated/<cli>.json`) — no JSONL parsing ships to
 * the client.
 *
 * Keep this JSON-serializable and presentation-agnostic: `StreamingTerminal`
 * renders it, and the SAME renderer is meant to back a future phone-preview
 * frame, so nothing here may assume the desktop window chrome.
 */

export type DemoCli = 'claude' | 'codex' | 'cline' | 'kimi' | 'kiro' | 'gemini' | 'cursor'

/** One line of a rendered diff. */
export type DemoDiffLine = { sign?: '+' | '-'; text: string }

/** A single conversation atom, in display order. */
export type DemoBlock =
  | { kind: 'user'; text: string }
  | { kind: 'assistant'; text: string }
  | { kind: 'reasoning'; text: string }
  | { kind: 'tool'; name: string; detail?: string; result?: string }
  | { kind: 'diff'; file?: string; lines: DemoDiffLine[] }

export type DemoTranscript = {
  cli: DemoCli
  /** Conversation atoms in display order (already trimmed + sanitized). */
  blocks: DemoBlock[]
}

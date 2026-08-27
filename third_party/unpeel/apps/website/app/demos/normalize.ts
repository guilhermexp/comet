/**
 * Pure provider JSONL → normalized `DemoTranscript` parsers.
 *
 * These mirror (a small subset of) the Rust transcript adapters in
 * `crates/unpeel-core/src/transcripts.rs` — just enough to replay a believable
 * streaming demo: user prompts, assistant text, reasoning, and tool calls with a
 * short result. No filesystem or DOM access, so this runs under `bun` (build +
 * test) and could run in the Worker too. See `app/demos/schema.ts`.
 */

import type { DemoBlock, DemoCli, DemoTranscript } from './schema'

// Strip the operator's home path from any absolute path so committed samples /
// the public site never leak a username.
const HOME_PATH = /\/Users\/[^/\s"']+/g
const sanitize = (s: string): string => s.replace(HOME_PATH, '~')

/** Collapse whitespace and clip to `n` chars with an ellipsis. */
const clip = (s: unknown, n = 220): string => {
  const t = String(s ?? '').replace(/\r/g, '').replace(/\s+/g, ' ').trim()
  return t.length > n ? t.slice(0, n).trimEnd() + ' …' : t
}

const clean = (s: unknown, n?: number): string => sanitize(clip(s, n))

/** Attach a tool result to the most recent tool block, if any. */
const attachResult = (blocks: DemoBlock[], result: string) => {
  const last = blocks[blocks.length - 1]
  if (last && last.kind === 'tool' && result) last.result = result
}

const eachLine = function* (jsonl: string): Generator<Record<string, unknown>> {
  for (const line of jsonl.split('\n')) {
    if (!line.trim()) continue
    try {
      yield JSON.parse(line)
    } catch {
      /* skip partial / non-JSON lines */
    }
  }
}

/** Claude Code — one message per line under `~/.claude/projects`. */
export function normalizeClaude(jsonl: string): DemoBlock[] {
  const blocks: DemoBlock[] = []
  for (const o of eachLine(jsonl)) {
    const type = o.type
    const msg = o.message as any
    if ((type !== 'user' && type !== 'assistant') || !msg) continue
    const content = msg.content
    const items = Array.isArray(content)
      ? content
      : typeof content === 'string'
        ? [{ type: 'text', text: content }]
        : []
    for (const b of items) {
      if (!b || typeof b !== 'object') continue
      if (b.type === 'text' && String(b.text).trim()) {
        blocks.push({ kind: msg.role === 'user' ? 'user' : 'assistant', text: clean(b.text, 260) })
      } else if (b.type === 'thinking' && String(b.thinking).trim()) {
        blocks.push({ kind: 'reasoning', text: clean(b.thinking, 180) })
      } else if (b.type === 'tool_use') {
        const input = (b.input ?? {}) as Record<string, unknown>
        const detail = input.file_path ?? input.path ?? input.command ?? input.pattern ?? Object.values(input)[0]
        blocks.push({ kind: 'tool', name: String(b.name ?? 'tool'), ...(detail ? { detail: clean(detail, 80) } : {}) })
      } else if (b.type === 'tool_result') {
        let c: unknown = b.content
        if (Array.isArray(c)) c = c.map((x: any) => x?.text ?? '').join(' ')
        attachResult(blocks, clean(c, 90))
      }
    }
  }
  return blocks
}

/** Strip Codex exec-output bookkeeping noise (chunk ids, wall time, exit code). */
const cleanCodexOutput = (out: string): string =>
  out
    .replace(/Chunk ID:[^\n]*/g, '')
    .replace(/Wall time:[^\n]*/g, '')
    .replace(/Process exited with code \d+/g, '')
    .replace(/Original token count[^\n]*/gi, '')

/** Codex — response_item / event_msg lines under `~/.codex/sessions`. */
export function normalizeCodex(jsonl: string): DemoBlock[] {
  const blocks: DemoBlock[] = []
  for (const o of eachLine(jsonl)) {
    const p = o.payload as any
    if (!p || typeof p !== 'object') continue
    switch (p.type) {
      case 'message': {
        // The assistant `message` duplicates `agent_message`; keep only the
        // user side here and let `agent_message` carry assistant text.
        if (p.role === 'assistant') break
        const c = p.content
        const txt = Array.isArray(c) ? c.map((x: any) => x?.text ?? '').join(' ') : ''
        if (txt.trim()) blocks.push({ kind: 'user', text: clean(txt, 260) })
        break
      }
      case 'agent_message':
        if (String(p.message).trim()) blocks.push({ kind: 'assistant', text: clean(p.message, 260) })
        break
      case 'reasoning': {
        const s = p.summary
        const txt = Array.isArray(s) ? s.map((x: any) => x?.text ?? '').join(' ') : ''
        if (txt.trim()) blocks.push({ kind: 'reasoning', text: clean(txt, 180) })
        break
      }
      case 'function_call': {
        let detail: unknown
        try {
          const a = JSON.parse(p.arguments)
          detail = a.cmd ?? a.command ?? a.query ?? a.path
        } catch {
          // arguments may be truncated (samples) or huge — pull the command
          // out of the raw string even when it isn't valid JSON.
          const m = String(p.arguments).match(/"(?:cmd|command|query|path)"\s*:\s*"((?:[^"\\]|\\.)*)/)
          if (m) detail = m[1].replace(/\\"/g, '"')
        }
        const name = p.name === 'exec_command' ? 'exec' : String(p.name ?? 'tool')
        blocks.push({ kind: 'tool', name, ...(detail ? { detail: clean(detail, 80) } : {}) })
        break
      }
      case 'function_call_output': {
        let out: unknown = p.output
        if (out && typeof out === 'object') out = (out as any).content
        attachResult(blocks, clean(cleanCodexOutput(String(out ?? '')), 90))
        break
      }
    }
  }
  return blocks
}

const NORMALIZERS: Partial<Record<DemoCli, (jsonl: string) => DemoBlock[]>> = {
  claude: normalizeClaude,
  codex: normalizeCodex
}

export function normalize(cli: DemoCli, jsonl: string): DemoTranscript {
  const fn = NORMALIZERS[cli]
  return { cli, blocks: fn ? fn(jsonl) : [] }
}

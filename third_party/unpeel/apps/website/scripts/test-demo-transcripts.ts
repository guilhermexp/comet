/**
 * Tests for the demo-transcript normalizers. Plain `bun` + `node:assert` (the
 * repo has no test runner; other one-offs run the same way). Run with
 * `bun run demos:test`.
 *
 * Covers both a tiny inline fixture (deterministic shape assertions) and the
 * real committed samples (so a bad sample or a normalizer regression is caught).
 */
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import { normalizeClaude, normalizeCodex, normalize } from '../app/demos/normalize'

const here = dirname(fileURLToPath(import.meta.url))
const sample = (cli: string) =>
  readFileSync(join(here, '..', 'app', 'demos', 'samples', `${cli}.jsonl`), 'utf8')

let passed = 0
const test = (name: string, fn: () => void) => {
  fn()
  passed++
  console.log(`  ✓ ${name}`)
}

// ---- Claude fixture --------------------------------------------------------
test('claude: text/tool/tool_result → blocks with folded result + ~ sanitize', () => {
  const jsonl = [
    JSON.stringify({ type: 'user', message: { role: 'user', content: 'add a route' } }),
    JSON.stringify({
      type: 'assistant',
      message: { role: 'assistant', content: [{ type: 'text', text: 'On it.' }] }
    }),
    JSON.stringify({
      type: 'assistant',
      message: {
        role: 'assistant',
        content: [{ type: 'tool_use', name: 'Edit', input: { file_path: '/Users/alice/app/router.ts' } }]
      }
    }),
    JSON.stringify({
      type: 'user',
      message: { role: 'user', content: [{ type: 'tool_result', content: 'The file was updated.' }] }
    })
  ].join('\n')
  const blocks = normalizeClaude(jsonl)
  assert.equal(blocks.length, 3)
  assert.deepEqual(blocks[0], { kind: 'user', text: 'add a route' })
  assert.deepEqual(blocks[1], { kind: 'assistant', text: 'On it.' })
  assert.equal(blocks[2].kind, 'tool')
  assert.equal((blocks[2] as any).name, 'Edit')
  // home path stripped to ~
  assert.equal((blocks[2] as any).detail, '~/app/router.ts')
  // tool_result folded onto the preceding tool block, not a separate block
  assert.equal((blocks[2] as any).result, 'The file was updated.')
})

// ---- Codex fixture ---------------------------------------------------------
test('codex: agent_message wins over duplicate assistant message; user kept', () => {
  const jsonl = [
    JSON.stringify({ type: 'response_item', payload: { type: 'message', role: 'user', content: [{ type: 'input_text', text: 'why is it slow?' }] } }),
    JSON.stringify({ type: 'response_item', payload: { type: 'function_call', name: 'exec_command', arguments: JSON.stringify({ cmd: 'nl -ba src/x.rs' }) } }),
    JSON.stringify({ type: 'response_item', payload: { type: 'function_call_output', output: 'ok\nWall time: 0.1s\nProcess exited with code 0' } }),
    JSON.stringify({ type: 'event_msg', payload: { type: 'agent_message', message: 'Because of X.' } }),
    JSON.stringify({ type: 'response_item', payload: { type: 'message', role: 'assistant', content: [{ type: 'output_text', text: 'Because of X.' }] } })
  ].join('\n')
  const blocks = normalizeCodex(jsonl)
  assert.deepEqual(blocks[0], { kind: 'user', text: 'why is it slow?' })
  assert.equal(blocks[1].kind, 'tool')
  assert.equal((blocks[1] as any).name, 'exec')
  assert.equal((blocks[1] as any).detail, 'nl -ba src/x.rs')
  // exec bookkeeping stripped from the folded result
  assert.ok(!/Wall time|Process exited/.test((blocks[1] as any).result ?? ''))
  // exactly one assistant block (no duplicate from the assistant `message`)
  const assistants = blocks.filter((b) => b.kind === 'assistant')
  assert.equal(assistants.length, 1)
  assert.equal((assistants[0] as any).text, 'Because of X.')
})

// ---- Real committed samples ------------------------------------------------
for (const cli of ['claude', 'codex'] as const) {
  test(`${cli}: real sample normalizes to non-empty blocks, no leaked home path`, () => {
    const t = normalize(cli, sample(cli))
    assert.equal(t.cli, cli)
    assert.ok(t.blocks.length > 0, 'expected at least one block')
    const dump = JSON.stringify(t)
    assert.ok(!dump.includes('/Users/'), 'home path must be sanitized to ~')
    for (const b of t.blocks) assert.ok(b.kind, 'every block has a kind')
  })
}

console.log(`\n${passed} passed`)

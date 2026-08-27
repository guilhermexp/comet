/**
 * Build step: read the trimmed real JSONL samples in `app/demos/samples/` and
 * write normalized, committed `app/demos/generated/<cli>.json` for the site to
 * import. Run with `bun run demos:build` after refreshing a sample.
 *
 * Keeping this a discrete generate-and-commit step (not runtime parsing) means
 * the Worker/client bundle never ships a JSONL parser or the raw samples.
 */
import { readFileSync, writeFileSync, existsSync } from 'node:fs'
import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import { normalize } from '../app/demos/normalize'
import type { DemoCli } from '../app/demos/schema'

const here = dirname(fileURLToPath(import.meta.url))
const samplesDir = join(here, '..', 'app', 'demos', 'samples')
const outDir = join(here, '..', 'app', 'demos', 'generated')

const CLIS: DemoCli[] = ['claude', 'codex']

for (const cli of CLIS) {
  const src = join(samplesDir, `${cli}.jsonl`)
  if (!existsSync(src)) {
    console.warn(`skip ${cli}: no sample at ${src}`)
    continue
  }
  const jsonl = readFileSync(src, 'utf8')
  const transcript = normalize(cli, jsonl)
  const out = join(outDir, `${cli}.json`)
  writeFileSync(out, JSON.stringify(transcript, null, 2) + '\n')
  console.log(`${cli}: ${transcript.blocks.length} blocks → ${out}`)
}

## 1. Seam permanente

- [x] 1.1 Expandir `lifecycle_extension_reports_provider_session_identity` para observar jsonl Start → continuação sem Stop → Stop terminal com identidade → Stop legado com identidade, e verificar que o harness Bun materializa o JS e o notify.sh

## 2. Emitter

- [x] 2.1 Em `lifecycle-extension.js`, silenciar `Stop` só quando `willContinue === true`; `agent_start` anuncia `Start`; terminal e legado anunciam `Stop` com metadata. Verificar com a sonda Bun (`failed: 0`)

## 3. Proveniência e DOX

- [x] 3.1 Recalcular `vendored_tree` em `third_party/unpeel-upstream.toml` no mesmo commit da correção vendorizada
- [x] 3.2 Registrar o contrato `willContinue` no `AGENTS.md` de `crates/workers-unpeel` e o seam na Test Coverage Matrix

## 4. Validação desta onda

- [x] 4.1 `openspec validate --change fix-worker-turn-lifecycle --strict` e `check-test-stamps.sh --root <worktree> fix-worker-turn-lifecycle` verdes

## Why

Um Worker `pi` nunca reporta lifecycle: o catálogo declara `source = "output"`,
o adapter não instala nada, e `derive_activity` cai no ramo hookless. Na prática
o Worker fica pintado como parado enquanto o agente trabalha, não dispara
notificação de conclusão e não é candidato seguro à hibernação. `pi` é o
upstream da família (`omp`, `prime-agent`), aceita o mesmo `-e/--extension` e
roda a mesma API de extensão (`agent_start`/`agent_end`) — medido em `pi`
0.84.4: a extensão da família emite `Start` e `Stop` com id de conversa e
caminho de transcript do provider.

O mesmo relato ("reinstalei o CLI limpo e os hooks não voltaram") expôs dois
defeitos no caminho de instalação: `write_file_atomic` não criava o diretório
pai, então um `~/.unpeel/hooks` apagado derrubava a instalação da extensão com
`No such file or directory` (`trace.log`, 2026-09-01: `Failed to install Comet
hooks for runtime sh.omp.cli (omp)`), e esse erro abortava o loop de instalação
inteiro — todo runtime atrás dele na ordem do catálogo ficava sem hooks.

## What Changes

- `pi` declara `lifecycle_hooks` e `notify_when_done` e passa a ter
  `source = "hooks"`, `authority = "partial"`, como `omp` e `prime-agent`.
- O adapter do `pi` instala a extensão de lifecycle da família e injeta
  `--extension <unpeel_home>/hooks/pi-family-lifecycle-extension.js` uma única
  vez, preservando o resto do comando; o append idempotente vira
  `setup::with_lifecycle_extension`, compartilhado com o adapter da família.
- A instalação de hooks gerenciados sobrevive a um root apagado: o writer
  atômico cria o diretório pai, e uma falha de runtime não pula os runtimes
  seguintes — as falhas são acumuladas e relatadas juntas.
- A detecção de hook gerenciado stale casa root temporário e nome de asset na
  **mesma linha** de comando, em vez de em qualquer lugar do arquivo.

## Capabilities

### New Capabilities

- `pi-worker-lifecycle-hooks`: um Worker `pi` reporta início e fim de turno por
  hook, com o mesmo contrato dos demais runtimes hook-owned do catálogo.

### Modified Capabilities

Nenhuma.

## Impact

- `third_party/unpeel/runtimes/pi/{runtime.toml,adapter/mod.rs}`,
  `runtimes/_shared/pi-family/adapter/{mod.rs,setup.rs}`,
  `crates/unpeel-core/src/hook_assets/mod.rs`,
  `apps/shared/.../GeneratedRuntimeCatalog.swift` (regenerado),
  `third_party/unpeel-upstream.toml` (proveniência do vendorizado).
- `crates/workers-unpeel/src/{hook_migration.rs,activity_bridge.rs,lib.rs}` —
  instalação resiliente, detecção stale por linha, e os testes que usavam `pi`
  como runtime hookless passam a usar `agy`.
- Nenhuma mudança de wire, CRDT, edge ou protocolo do Host: `lifecycle_hooks` e
  `notify_when_done` já trafegam em `WorkersSessionCapabilities`.
- Sessões `pi` já rodando não são migradas — só o próximo lançamento carrega a
  extensão.
- DOX: `crates/workers-unpeel/AGENTS.md`, `third_party/AGENTS.md`.

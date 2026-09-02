## Why

Um Worker CLI só deixa de existir por Stop, Archive ou Remove explícitos.
Depois que o agente encerra o turno, o CLI fica no prompt indefinidamente:
na máquina de desenvolvimento, 36 de 39 session hosts estavam há mais de
20 h em `Stop`, somando ~4,3 GB e ~5 min de CPU por dia cada. As settings
de recursos já persistem `hibernation_enabled`,
`hibernate_after_idle_minutes` e `max_live_idle_workers` desde
`feat(workers): persist resource settings`, mas nada as consome. Com o
resume da família pi resolvido (`pi-family-resume-adapter`), parar um Worker
ocioso deixa de custar o contexto e a hibernação vira segura para todo preset.

## What Changes

- O Comet passa a hibernar Workers ociosos: um Worker `running` em atividade
  `idle` há mais de `hibernate_after_idle_minutes` é parado e arquivado,
  mantendo diretório de sessão e conversa, de modo que Restart o traz de
  volta.
- Workers em `blocked` (esperando decisão humana), Workers `working`,
  Workers pinados, a sessão selecionada no painel, sessões de terminal e
  Workers com lançamento de runtime pendente nunca são hibernados.
- Quando o número de Workers ociosos vivos passa de `max_live_idle_workers`,
  os ociosos há mais tempo são hibernados antes do prazo, do mais antigo para
  o mais novo, até voltar ao teto.
- A seção Resources das settings de Workers ganha os controles já
  persistidos (toggle, minutos, teto), que hoje não têm UI.
- O controller MCP ganha `restart_worker`, e `send_text` / `send_keys` num
  Worker hibernado respondem com um erro que nomeia a ação de restart, para o
  Orquestrador continuar uma delegação sem adivinhar.
- Hibernação é desligada por default (`hibernation_enabled: false`
  permanece), então nada muda para quem não ligar.

## Capabilities

### New Capabilities

- `worker-hibernation`: parar e arquivar automaticamente Workers ociosos
  segundo as settings de recursos, preservando a conversa para retomada.

### Modified Capabilities

Nenhuma.

## Impact

- `crates/ui/src/workers/model.rs` (decisão e disparo no ciclo de refresh),
  `crates/ui/src/workers/settings.rs` (controles Resources),
  `crates/workers-unpeel/src/controller_mcp.rs` (`restart_worker`, erro em
  Worker hibernado), `crates/workers-unpeel/src/lib.rs` (política pura de
  hibernação, testável sem UI).
- Nenhuma mudança de wire, CRDT, edge ou protocolo do Host: usa
  `WorkersSessionCommand::Archive` e `SessionAction::Restart` já existentes.
- Depende de `pi-family-resume-adapter` para que `omp`/`prime-agent` voltem
  com contexto; em runtimes sem capability `restart`, o Worker não é
  hibernado.
- DOX: `crates/ui/AGENTS.md` (workers), `crates/workers-unpeel/AGENTS.md`
  (controller MCP), skill `delegate` do Orquestrador (fora do repo, apontar
  `restart_worker` antes de `send_text`).

## Why

A extensão de lifecycle da família pi ignora o payload de `agent_end` e anuncia `Stop` em toda chamada. Na API pública de extensão, `AgentEndEvent.willContinue === true` significa que uma continuação automática já está agendada, não um settle terminal. O painel marca o Worker ocioso no meio de um turno OMP que ainda vai continuar.

## What Changes

- `agent_start` continua anunciando `Start` com o metadata de identidade do provider.
- `agent_end` com `willContinue === true` NÃO anuncia `Stop`.
- `agent_end` terminal (`willContinue === false`) e o legado sem a flag continuam anunciando `Stop`, com o mesmo metadata que o consumidor já lê.
- A correção mora no emitter (`lifecycle-extension.js`). A máquina de estados de atividade, `wait_for_status`, normalize/RPC e `session_stop` ficam intactos.

## Capabilities

### New Capabilities

- (nenhuma)

### Modified Capabilities

- `pi-worker-lifecycle-hooks`: o fim de turno anunciado ao transporte de notificação passa a distinguir continuação já agendada (`willContinue === true`) de settle terminal / legado sem flag.

## Impact

- `third_party/unpeel/runtimes/_shared/pi-family/assets/lifecycle-extension.js` (fonte embutida via `include_str`).
- `third_party/unpeel/runtimes/_shared/pi-family/adapter/setup.rs` (seam permanente Bun/notify.sh).
- `third_party/unpeel-upstream.toml` (tree id do snapshot vendorizado).
- DOX em `crates/workers-unpeel/AGENTS.md`.
- Asset instalado em `~/.unpeel/hooks` só atualiza pela instalação oficial; processos vivos podem continuar com a cópia antiga até relançar o Worker.

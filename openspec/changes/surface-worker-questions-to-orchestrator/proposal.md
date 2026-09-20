## Why

Um Worker que abre um diálogo interativo para perguntar algo ao Orquestrador fica indistinguível de um Worker trabalhando: medido em 2026-09-20 na sessão `4003f898-4471-4ca3-a274-bfc60c98844d` (runtime `omp`, projeto meridian), com o diálogo `ask` aberto e bloqueado, `inspect_worker` devolveu `state: "running"`, `activity: "working"`, `unread: false`, nenhuma notificação foi emitida e `wait_for_status(exited, 180s)` expirou sem sinal. Um Orquestrador que confia na notificação encerra o turno e o Worker fica parado para sempre.

O mesmo episódio expôs que o único canal que atravessa a fronteira chega vazio: as duas notificações de conclusão vieram com `Output tail: none`, e `read_transcript` recusa o runtime `omp` com `502`. Hoje não existe transporte confiável de conteúdo saindo de um Worker.

## What Changes

- Um Worker da família pi que abre um prompt de permissão ou pergunta passa a reportar atenção por hook, e o painel/Orquestrador passam a vê-lo como `blocked` em vez de `working`.
- O runtime `omp` deixa de declarar `attention_reliable = false`: a atenção passa a ser observável por evento, não por heurística de viewport.
- Atingir `blocked` engatilha a notificação ao pai já modelada (`WorkerParentNotificationKind::WaitingForInput`), de modo que o Orquestrador é acordado quando o Worker pergunta — não só quando termina.
- A cauda de output da notificação para de ser apagada: uma linha terminada em retorno de carro (todo repaint de TUI) hoje vira string vazia e o prompt imprime `none`.
- `unread` deixa de ser um campo morto. Hoje ele depende de `activity-state.json`, que só o app nativo Swift do upstream escrevia e que o Comet nunca grava, então é permanentemente `false`. Passa a ser derivado do estado de notificação pendente que a crate já mantém.
- O runtime `omp` passa a declarar a capability `transcript` e ganha adaptador, tornando `read_transcript` um caminho estruturado de leitura em vez de `502`.

## Capabilities

### New Capabilities

Nenhuma.

### Modified Capabilities

- `pi-worker-lifecycle-hooks`: a extensão de lifecycle da família pi passa a reportar atenção quando o agente abre um prompt interativo, e o motor de estado passa a aceitar essa transição para prompts de pergunta ao usuário; hoje ela é descartada por construção.
- `workers-host-bridge`: a notificação ao pai passa a preservar o conteúdo da cauda de output, `unread` passa a ter fonte de verdade local, e o transcript do runtime `omp` passa a ser legível pelo controller MCP.

## Impact

- `third_party/unpeel/runtimes/_shared/pi-family/assets/lifecycle-extension.js` — hoje escuta apenas `agent_start` e `agent_end`.
- `third_party/unpeel/crates/unpeel-tui/src/activity.rs` — `is_latch_only` descarta `PermissionRequest` quando `tool_name == Some("AskUserQuestion")`; o comentário justifica a supressão com "a pergunta aparece no terminal", premissa que vale para um humano no pane e não para um Orquestrador.
- `third_party/unpeel/runtimes/omp/runtime.toml` — `capabilities` sem `transcript`; `[lifecycle] attention_reliable = false`.
- `third_party/unpeel/crates/unpeel-core/src/transcripts/` — tabela `TRANSCRIPT_ADAPTERS` gerada em build a partir das capabilities declaradas.
- `crates/workers-unpeel/src/parent_notifications.rs` — `safe_output_block`; `WorkerCompletionEvidence`; derivação de `unread`.
- `crates/workers-unpeel/src/activity_bridge.rs` — `derive_activity` e o mapeamento de `HookState` para o wire.
- `crates/ui/src/workers/model.rs` — coleta da cauda de output que alimenta a notificação.
- Consumidores: painel Workers do app headed e o controller MCP do Orquestrador primário. Ambos passam a distinguir Worker bloqueado de Worker trabalhando.

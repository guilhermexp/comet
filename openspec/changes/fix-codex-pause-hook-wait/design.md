# Design

## Context

A máquina de atividade é `third_party/unpeel/crates/unpeel-tui/src/activity.rs`, incluída por `#[path]` em `crates/workers-unpeel/src/activity_bridge.rs`. O host varre o viewport e grava `menu_prompt_active`. `viewport_has_menu_prompt` só aceita navegação+seleção ou confirmar+cancelar. `wait_until_matching` só retorna o status pedido, conclusão do episódio, cancelamento ou timeout.

Codex usa hooks e desconfia de `Stop` (`distrust_stops_while_output_grows`, graça de 5s e janela de 90s). Esc/Ctrl-C pinta `Conversation interrupted` e devolve o prompt sem necessariamente entregar `Stop`. A revisão de hooks pinta `t trust all · enter review · esc close` e também não é `PermissionRequest`.

## Goals / Non-Goals

**Goals:**

- Bloquear no footer interativo de revisão e soltar a espera de `completed`/`exited` com needs-input.
- Reportar `idle` na interrupção do turno atual sem `hook_confirmed_idle`.
- Manter graça de rearme do Codex, episódio, geração, cancelamento e deadline.
- Um único ajuste de estado reportado, usado pelo bridge do Comet e pelo `derive_status` do TUI.

**Non-Goals:**

- Aprovar hooks ou confiar em pastas automaticamente.
- Tratar silêncio, CLI vivo ou terminal de fundo como ociosidade ou conclusão.
- Heurística genérica de "qualquer falta de output".
- Feature nova no iOS do Comet. O espelho do detector fica no iOS vendorizado do Unpeel.

## Decisions

1. **Footer de revisão é o terceiro par do detector de menu.** Ação (`enter review` ou `trust all`) e fechar (`esc close` ou `escape to close`) na mesma janela de duas linhas já usada pelos outros pares. Aviso, prosa e lista sem o par continuam negativos. O iOS vendorizado ganha os mesmos marcadores.

2. **Interrupção é a row de status do Codex, não um substring.** `viewport_shows_settled_interrupt` é verdadeiro só quando a última row é `■ Conversation interrupted` com sufixo conhecido do CLI (nenhum, ou `- use /feedback if something went wrong`), o composer (`›`, `❯` ou `>`) abaixo dela está vazio ou num placeholder exato (`Ask Codex to do anything`, `Ask a follow-up question`), e nenhuma linha depois traz `esc to interrupt` / `escape to interrupt` nem texto digitado no composer. Prosa, blockquote, sufixo desconhecido, prompt só acima da row e um shell vivo sem composer embaixo não contam. Footer de trabalho e submissão digitada vencem o placeholder. O host grava `turn_settled_by_interrupt` no manifesto, no mesmo scan de borda do menu.

3. **A máquina não confirma Stop.** `report_state_for_settled_interrupt` só degrada `Busy` reportado para `Idle`. `stopped_at` e `hook_confirmed_idle` não mudam. Atenção explícita permanece. O menu, aplicado depois, ainda promove `Busy`/`Idle` a bloqueado. Footer de trabalho de novo limpa o flag e o `Busy` do latch reaparece; `Start`/`UserPromptSubmit` e a graça de rearme do Codex ficam intactos.

4. **Needs-input é anterior à conclusão e ao timeout.** Para `completed` e `exited`, atividade `blocked` retorna no mesmo tick: `matched: false`, `needs_input: true`, `blocked: true`, snapshot do worker e `next` para inspecionar o output e responder, sem aprovar nada. `wanted == blocked` continua casando antes. Cancelamento do host continua a vencer a espera. Outros status não ganham esse atalho.

## Risks / Trade-offs

- Frase de interrupção citada no prompt, sem footer de trabalho, conta como pausa. Aceito: é a evidência observável; um turno novo volta a pintar `esc to interrupt`.
- Campo novo no manifesto exige literais de teste. `#[serde(default)]` mantém manifestos antigos.
- O espelho Swift não roda no gate Rust. O contrato fica no detector Rust; o Swift só copia os marcadores.

## Migration Plan

Sem migração de dados. Manifestos antigos leem o flag como falso. Rollback é reverter o patch vendorizado e o bridge.

## Open Questions

Nenhuma. A prova visual headed fica com o Main depois do source estável.

## Context

See proposal.md for motivation. `lifecycle-extension.js` registra `agent_start`/`agent_end` e chama `notify.sh` com `Start`/`Stop`. O handler de `agent_end` ignora o primeiro argumento (`_event`) e sempre emite `Stop`. Fonte pública: `AgentEndEvent.willContinue` em https://raw.githubusercontent.com/can1357/oh-my-pi/HEAD/packages/coding-agent/src/extensibility/shared-events.ts. O RPC do harness usa outro nome (`isTerminal`); `classify_agent_end` já respeita isso e está fora deste corte.

## Goals / Non-Goals

**Goals:**
- No emitter, `willContinue === true` retorna sem `Stop`; o resto do contrato de notificação permanece.
- O teste permanente em `adapter/setup.rs` observa a sequência Start/Stop com identidade, não só forwarding.

**Non-Goals:**
- Máquina de estados, `wait_for_status`, `distrust_stops_while_output_grows`, debounce, `session_stop`, filtro de subagent, normalize/RPC.

## Decisions

1. **Guard estrito no emitter.** `event?.willContinue === true` silencia só esse `Stop`. Ausência da flag e `false` anunciam. Não reutilizar `isTerminal` do RPC.
2. **Não migrar para `session_stop`.** Esse evento é pré-settle e pode pedir continuação; não é a notificação final.
3. **Um seam só.** Expandir `lifecycle_extension_reports_provider_session_identity` para jsonl observado (continuação, terminal, legado). Não empilhar teste de identidade separado. Idle após Stop real permanece noutro teste.

## Risks / Trade-offs

- [Asset embutido] → `include_str` + `install_lifecycle_extension` no launch. Processo vivo pode ficar com JS antigo até relançar. Não editar `~/.unpeel/hooks` à mão.
- [Flag ausente em pi/prime] → tratado como terminal, compatível com o legado.

## Migration Plan

Corrigir o fonte vendorizado. Relançar um Worker da família pi para o instalador oficial reescrever o asset. Rollback = reverter o commit do emitter.

## Why

O Chat nativo do Comet descarta resultados textuais de comandos locais OMP, não apresenta a atividade de compactação e só oferece steering para mensagens enviadas durante trabalho. O usuário autorizou executar estas três correções de paridade, excluindo toda interface de extensões.

## What Changes

- G1: preservar command_output na ordem correta, fechar comandos locais sem depender de agent_end e manter os resultados no Chat Transcript.
- G7: projetar compactação automática/manual como atividade real da Session, com término/cancelamento/falha e integração com contexto/idle recap.
- G4: oferecer mensagem pós-turno como intent durável separado de steering, com FIFO, cancelamento e prioridade de Stop.
- Provar comportamento com testes de transporte/estado e uso nativo real. Não reiniciar a instância que hospeda o orquestrador.

## Capabilities

### New Capabilities

- `omp-local-command-output`: saída e terminalidade de comandos locais, sem invocação artificial do modelo.
- `omp-compaction-activity`: projeção transitória e fiel da compactação automática/manual no Chat nativo.
- `chat-after-turn-queue`: submissão e execução durável de mensagens pós-turno distintas de steering.

### Modified Capabilities

Nenhuma requirement existente é substituída. Context usage last-known, Chat Transcript Export, Live Voice e descoberta de extensions mantêm seus contratos.

## Decisions

- D-01: reutilizar TextDelta e o fold de texto existente para resultados slash; não usar toasts como único registro.
- D-02: consumir eventos desde a requisição inicial, preservando ordem e backpressure. Operações locais longas têm prazo próprio finito e cancelamento, não o prazo de ACK curto.
- D-03: representar FollowUp no ledger existente; somente o host executor inicia a próxima mensagem após assentamento real e fora de compactação/AwaitingInput.
- D-04: preservar Run e Steer existentes; não usar a fila volátil follow_up do OMP como segunda fonte de verdade.
- D-05: controles elegíveis ultrapassam FollowUp diferido; Stop/erro/crash não disparam a fila anterior nem repetem entrega incerta.
- D-06: compactação é estado opcional da Session; resumos internos não entram no transcript nem em streams públicos novos. Não fabricar progresso percentual.
- D-07: observar estado real de compactação manual, que pode não emitir eventos automáticos. Não detectar atividade pelo texto digitado nem duplicar o parser da CLI.
- D-08: extensões/Fusion/TUI, configurações de modelos, warming de subprocesso, dependências, Workers e publicação ficam fora.

## Impact

Harness OMP, tipos de atividade/commands, doc host/executor e controles nativos de composer/transcript. O fluxo de documentos continua sendo a única fonte durável de comandos e mensagens. Novos kinds são rejeitados por hosts antigos sem fallback inseguro; campos opcionais não quebram consumidores antigos.

Plano de origem: `docs/plans/2026-09-06-0238-feat-omp-chat-runtime-parity-plan.md`. Fases F1/U1 → F2/U3 → F3/U2, com auditoria entre elas. Código e gates usam a cadeia AGENTS.md do repo; requisitos e cenários desta change prevalecem sobre o insumo de planejamento.

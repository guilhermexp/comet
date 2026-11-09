# Design

## Context

Ver motivação em `proposal.md`. Auditoria em 2026-11-07: Comet `28aa8628cca0fc6c977a7eb1c89550466a06557e`; upstream oficial OMP `abc8f6eef3404f7cd91b3d52f150c97466c26ff6`. A branch local do checkout irmão contém extensões do fork e não é a referência oficial.

Fontes primárias:

- [RPC atual, pinado](https://github.com/can1357/oh-my-pi/blob/abc8f6eef3404f7cd91b3d52f150c97466c26ff6/docs/rpc.md), seções Startup, Request/Response Correlation, Prompt payload, Yield vs settled e Progress events.
- [Contrato já presente em v18.18.0](https://github.com/can1357/oh-my-pi/blob/v18.18.0/docs/rpc.md): piso conservador comprovado, sem afirmar que foi a primeira versão a implementá-lo.
- [Dispatcher](https://github.com/can1357/oh-my-pi/blob/abc8f6eef3404f7cd91b3d52f150c97466c26ff6/packages/coding-agent/src/modes/rpc/rpc-mode.ts), especialmente o encaminhamento de prompts e a inicialização de extensões.
- [Resultados de prompt](https://github.com/can1357/oh-my-pi/blob/abc8f6eef3404f7cd91b3d52f150c97466c26ff6/packages/coding-agent/src/modes/rpc/rpc-prompt-results.ts#L304): conclusão local assíncrona sem run é um caminho real do produtor.

Achados no consumidor:

| Achado | Evidência Comet | Consequência |
|---|---|---|
| F1: startup interativo bloqueia | `process.rs:282-289,344-355`, `mod.rs:365-367,536` | Negociação/configuração aguardam enquanto o handler de diálogo ainda não está ativo. |
| F2: resultado correlacionado ausente | `process.rs:362-450`, `mod.rs:1382-1407` | O ID é interno ao request e o loop conclui por eventos sem identidade do prompt; comando local assíncrono pode ficar aberto, e erro pode perder classificação. |
| F3: yield confundido com terminal | `mod.rs:1387`, `normalize.rs:158-178` | Fim não terminal é ignorado mesmo quando cedeu; assentamento sem próximo run não encerra a atividade pendente. |
| F4: compact depende de prosa | `mod.rs:1445-1463` | Resultado estruturado é ignorado e ausência de trabalho pode virar erro. |

Os paths da tabela são relativos a `crates/harness/src/omp/`. Testes de subprocesso já cobrem warm reuse, startup burst, correlação de respostas, steering, host tools, timing e compactação; serão estendidos.

## Goals / Non-Goals

**Goals:** cumprir os deltas das três capabilities com mudanças concentradas no driver, fixtures e testes; preservar as fronteiras de turno e os recursos vivos da Session.

**Non-Goals:** atualizar ou modificar OMP, restaurar drafts no Stop, implementar todas as RPCs opcionais, redesenhar UI, alterar schema persistente ou lançar o app instalado. Não desabilitar extensões globalmente.

## Decisions

### D1. Separar identidade, admissão e resultado

`OmpProcess` deve disponibilizar a identidade do request antes do envio, mantendo o mapa de respostas e seus limites. O loop registra o prompt pendente antes que um resultado possa chegar; ACK, resultado e eventos do run são tratados separadamente. Ampliar o seam existente de requests em vez de criar outro transporte.

No contrato moderno, `prompt_result.status` governa a conclusão e o erro. Resposta local síncrona continua sendo conclusão imediata; resposta de erro seguida de resultado não duplica a falha. Um pequeno estado por prompt/geração conserva ID, admissão e conclusão, descartando duplicatas e IDs alheios. `message_end` continua autoritativo para conteúdo e métricas: `agent_end.messages` não é um replay completo garantido. O watchdog moderno após o ACK mede inatividade; frames de atividade renovam o intervalo, mas resultados alheios não. Host tools e input humano conservam seus próprios prazos sem consumir esse orçamento: `wait_for_status` pode durar até quatro horas e `wait_for_turn` até uma hora. Compactação explícita mantém seu prazo absoluto cancelável.

O piso moderno é a versão verificada do mesmo executável, via `executable::binary_version`, `>=18.18.0`. V2 de framing não é prova de lifecycle. Versões anteriores/desconhecidas conservam fallback legado; sinais estruturados efetivamente recebidos também são reconhecidos, sem dupla conclusão. Fixtures cobrem o piso, versão antiga e desconhecida. Não inventar campos de capability no `ready` nem sondar suporte enviando prompt.

### D2. Usar o canal de background que a engine já possui

Reter `TurnControl`, hoje descartado como `_turn` em `mod.rs:846`. Marcar `set_background(1)` antes de `Done` se houver pendência nativa (`sessionSettled:false` ou `awaitingAsyncWork:true`), conservar durante wakes e limpar em `session_settled`, estado confirmado `isSettled:true` ou teardown. É uma indicação conservadora, não contagem inventada de tasks.

O `Done` do prompt sai no yield confirmado; `yielded:false` mantém o turno aberto. `agent_start` autônomo abre outro turno, sem reutilizar o ID já concluído; ele termina uma vez no yield apropriado. Assentamento isolado só limpa pendência, não emite outro `Done`. Preservar a resolução única de inputs e host tools em cada fronteira válida.

A engine já consulta esse canal em `sessions.rs:4944-4974,4988-5008`; `Done(Completed)` estaciona mantendo runtime em `5911-5926`. Provar o comportamento em `tests/omp_warm_runtime.rs` e/ou `tests/runtime_survival.rs`; não adicionar novo evento de wire nem polling. Um `get_state` já necessário pode reconciliar o estado, sem timer adicional.

### D3. Compactação usa sua operação ativa como escopo

Para compact explícito em runtime moderno, aguardar o par de progresso nativo e interpretar seu fim estruturado. Os eventos não têm ID de prompt: só vinculá-los à operação de compactação ativa naquela geração, sem confundir compactação espontânea ou evento atrasado. ACK local não substitui o término do trabalho. `result` representa commit; `skipped` é no-op; `aborted` e erro seguem os caminhos não bem-sucedidos existentes.

Após sucesso/no-op, conservar leitura de usage antes da conclusão. Preservar o fallback de `command_output` para contratos antigos, com deadline e cancelamento. Prosa contraditória não vence resultado estruturado; output repetido não vira mensagem duplicada. Reaproveitar a apresentação existente, sem novo fluxo de interface.

### D4. Tornar startup observável antes de comandos que podem esperar extensões

Separar child pronto de negociação/configuração concluída. No Chat, disponibilizar o stream de eventos e o handler de `extension_ui_request` enquanto a inicialização prossegue, reutilizando o mesmo mecanismo de input do run. Negociação, assinatura de subagentes, host tools e resume continuam ordenados, mas não impedem atender um diálogo do startup. Tempo em espera de input não conta como timeout de ACK; cancelamento e falha real permanecem limitados e encerram todos os recursos.

Discovery de modelos/comandos sem Chat usa `--no-ui` explícito, mantendo descoberta de extensões, overlay de skills e modos já exigidos. Não aplicar esse argumento aos Chats. O contrato `omp-extension-discovery` permanece válido. Preservar limites de buffer, backpressure e validação de chunks; rejeitar a alternativa de silenciar todas as extensões.

## Risks / Trade-offs

- Resultado pode chegar antes de o consumidor processar o ACK → registrar identidade antes do envio e testar ambas as ordens de consumo.
- Legado e extensões próprias podem divergir da versão → piso conservador, aceitação de sinais recebidos e fixtures com variantes; nunca inferir sucesso moderno pelo silêncio.
- Background não libera ou libera cedo → testar reaper/update, wake e assentamento sem run; limpar no teardown e reconciliar somente com estado nativo confirmado.
- Startup envolve criação das bridges antes de completar o run → caminho único de cleanup em falha/cancelamento, com teste que prova ausência de filho restante.
- Proposta é fundamentada em código e contratos, não em reprodução viva com provider → fixtures determinísticas são a prova exigida; smoke pago ou lançamento do app não faz parte deste pedido.

## Migration Plan

Sem migração de dados ou dependências. Implementar após aceite, com worker delimitado e testes RED/GREEN por grupo. Conferir e publicar apenas nos destinos autorizados na execução. Reversão pelo Git restaura o driver anterior; não altera contas, configuração ou executável do OMP.

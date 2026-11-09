# Design

## Context

Ver problema em `proposal.md`. Base da release: `v1.0.18`, commit `8c5898efc5438be60978d8dfd74468df95469e38`; o checkout local `main@28aa8628` diverge. A execução deve reconciliar os paths na linha destinada à próxima entrega, preservando o trabalho local.

### Diagnóstico de 2026-11-08

A imagem anexada mostra `Unsupported`, `Used / capacity — / —`, listas MCP/memory sem valores e quatro linhas de rodapé técnico. No próprio Chat Codex ativo desta conversa, o RPC read-only `GetChatContextBreakdown` reproduziu `availability:unsupported`, `source:codex`, `usedTokens:null`, `capacity:null`, listas unsupported e motivo `Native context inspection is unavailable for this runtime`.

Na release, esse motivo nasce em `crates/harness/src/context_inspection.rs:22-30`, quando o driver não anexa o mailbox. Só Claude e OMP fazem esse attach. Codex e Pi já publicam `AgentEvent::ContextUsage` (`codex/normalize.rs:296`; `pi/normalize.rs:96,154`), mas o caminho detalhado não aproveita esses agregados. A engine propaga o Unsupported e acrescenta identidade/origem/horário (`engine/src/context_breakdown.rs:195,213-224,250-260`).

O renderer `ui/src/details_sidebar/context_breakdown.rs:365-403,479-557` monta barra e seções mesmo sem dados, e adiciona a origem, observação, falta de horário nativo e motivo bruto como parágrafos. A spec anterior já previa que Codex exibiria seus totais reais; esse comportamento ficou incompleto.

| Runtime conferido | Fato disponível | Limite confirmado |
|---|---|---|
| Codex 0.167.0 | Uso agregado e capacidade nativos, via eventos já normalizados | Sem mailbox de inspeção no Comet. O schema do binário, inclusive experimental, não contém RPC de composição por categoria/arquivo/tool. |
| Pi 1.5.0 | Agregados nativos já normalizados | Sem mailbox detalhado no Comet; não tratá-lo automaticamente como detalhamento suportado. |
| OMP oficial 18.18.1 | Agregados de estado nativo | O consumidor espera `get_context_breakdown`, ausente nessa distribuição oficial. |
| Claude Code 2.1.319 | Controle summary já integrado | Categorias estimadas e memória redigida/indisponível em parte; não representa exposição integral de todo contexto interno. |

Codex foi conferido pela [referência oficial](https://learn.chatgpt.com/docs/app-server) e pelo comando estático `codex app-server generate-json-schema --experimental --out <temporário>`, sem iniciar thread: 278 métodos, com `thread/tokenUsage/updated`, `thread/read` e `mcpServerStatus/list`, sem método de composition breakdown. `ThreadTokenUsage` informa `last`, `total` e `modelContextWindow`; não reinterpretar o total cumulativo como ocupação atual.

No [OMP oficial 18.18.1](https://github.com/can1357/oh-my-pi/blob/v18.18.1/docs/rpc.md), `get_state.contextUsage` existe e `get_context_breakdown` não. O produtor da RPC detalhada está no checkout irmão `oh-my-pi`, branch `comet/context-inspection`, revisão observada `c13c5f0712`, e na change arquivada `2026-11-05-expose-session-context-breakdown`. Seus recibos registram integração local sem publicação ou substituição do executável instalado. `scripts/omp-dev` é opt-in, conforme `scripts/AGENTS.md:40`; não é uma correção instalada para o usuário.

## Goals / Non-Goals

**Goals:** tornar utilizáveis os fatos reais de contexto já disponíveis; corrigir estados vazios; remover os textos marcados pelo usuário; preservar leitura, origem e escopo por Chat. Expor todas as categorias/listas que um produtor compatível realmente reporta.

**Non-Goals:** inventar atribuição, extrair o prompt interno inteiro, incluir conteúdo de memória ou resultados de tools, transformar catálogo disponível em itens carregados, enviar `/context` como prompt, criar loop de polling, trocar/distribuir CLIs ou fazer override global de executável.

## Decisions

### D1. Agregado e composição têm disponibilidades independentes

Reaproveitar o caminho já autoritativo de `ContextUsage` e as regras de `context-usage-continuity`. O relatório detalhado deve combinar somente fatos cuja origem pertença ao Chat/host/runtime/modelo selecionados. Se a inspeção detalhada for unsupported, usar o agregado validado como relatório parcial; se a composição vier disponível, ela prevalece para seus próprios campos sem somar duas vezes o mesmo total.

A integração pode reter o último agregado no estado de runtime da engine e convertê-lo no seam de `context_breakdown`, ou responder pelo adapter se isso reutilizar sua normalização existente. Não criar um segundo parser de tokens nem inferir ocupação a partir de `totalTokens` acumulado. Manter timestamp/proveniência do evento, não o horário da consulta que apenas leu o cache.

Não copiar dados de outro modelo ou processo para simular relatório atual. Quando o dado anterior puder ser mostrado somente como histórico, manter stale explícito; quando não houver proveniência suficiente, o valor permanece desconhecido. Total zero válido continua zero e não vira falta de suporte.

### D2. OMP oficial tem fallback agregado, não promessa de update

Preservar a consulta detalhada e o contrato schema-v1 para builds que de fato o implementam. Na distribuição oficial sem RPC, aproveitar os agregados já recebidos e/ou o `get_state` read-only já suportado, sem reconstruir o request do provider. Cachear a falta da capability por runtime; não repetir comando desconhecido como se pudesse mudar por refresh.

Trocar o diagnóstico `Update OMP…` por capacidade ausente, com orientação contextual verdadeira. Atualizar a CLI oficial para outra versão sem essa RPC não satisfaz o contrato. Prova source-built deve identificar o produtor e permanecer distinta de suporte no executável instalado.

Para composição completa do OMP no ambiente de produção, será necessário disponibilizar um produtor compatível pelo fluxo de instalação/distribuição autorizado. Esta change não decide silenciosamente substituir a CLI do usuário por um fork. Até a dependência existir, o contrato aceito aqui é parcial e a limitação deve continuar registrada na fila. Não encerrar essa dependência como se a composição completa estivesse entregue.

### D3. Corpo compacto, metadados sob interação

Refinement do componente existente, seguindo `/impeccable` e `DESIGN.md`: manter o cabeçalho Context, controle de expansão e alinhamento das colunas. Mostrar total/capacidade/barra quando os campos existirem e estado curto de detalhe parcial quando necessário. Mostrar categorias e seções MCP/memory apenas com conteúdo ou estado medido útil; lista unsupported não vira `0` ou bloco vazio com traço.

Remover do corpo padrão as linhas `Native runtime`, `Observed…`, `Provider measurement time unknown`, motivos crus e notas técnicas repetitivas. Origem, horário, precisão e diagnóstico acionável ficam em tooltip/help acessível por teclado do estado/Refresh. Não substituir os quatro parágrafos por outra pilha de texto. Um relatório totalmente vazio tem um único estado curto; só oferecer ação que possa atualizar algum fato ou esclarecer uma limitação real.

As atribuições parciais continuam identificadas: estimativas não viram medidas exatas, categorias ausentes não viram zero e deferred tools não entram no orçamento ativo. Manter os limites de lista e privacidade da spec original.

### D4. Provar o cenário que chegou ao usuário

Incluir fixture correspondente ao Unsupported genérico com agregado conhecido, fixture totalmente sem dados, OMP oficial sem RPC e produtor compatível com categorias. Testar host remoto, troca de runtime/modelo, resposta tardia, refresh falho, zero real, memória redigida e categorias estimadas. A fixture de render deve provar ausência dos quatro parágrafos, barra/listas vazias e presença de totais parciais quando conhecidos.

O smoke read-only do produtor compatível não autoriza novo turno ou uso pago. Os testes devem registrar executável/build real, consumidor e limites observados; não declarar suporte da CLI instalada com base apenas na fixture ou no checkout fonte.

## Risks / Trade-offs

- Exibir um agregado antigo como atual → conservar proveniência/timestamp e os guards existentes de host/configuração/runtime antes e depois da inspeção.
- Confundir “todos os dados expostos” com acesso a todo contexto interno → reportar cobertura parcial por contrato, sem composição inventada ou promessa de dump completo.
- Remover rodapé e perder honestidade do estado → conservar estado curto e detalhes acionáveis acessíveis sob interação, sem parágrafos persistentes.
- Validar só o consumidor contra um produtor não distribuído → matriz por build e dependência de produção explícita; não chamar source-built de instalação corrigida.
- Conflitar com a primeira change da fila em `omp/mod.rs` → executar após ela e adaptar sobre a revisão integrada, repetindo apenas os testes afetados por essa integração.

## Migration Plan

Sem migração de perfis, credenciais ou histórico. Implementar sobre a linha correta do fork e verificar o protocolo existente entre versões. Se houver campo novo, ele é opcional; preferir a forma `ContextBreakdown` atual. O painel continua operando com capacidades parciais. Distribuição de produtor OMP, ampliação de APIs do Codex/Pi e publicação do app exigem entregas autorizadas próprias; nenhuma é presumida pela investigação.

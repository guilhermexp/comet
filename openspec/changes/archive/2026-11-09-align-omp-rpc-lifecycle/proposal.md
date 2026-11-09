# Proposal

## Why

A auditoria do Comet em `28aa8628` contra o OMP oficial em `abc8f6eef3404f7cd91b3d52f150c97466c26ff6` encontrou lacunas nos sinais de conclusão e no atendimento de extensões durante a inicialização. Elas podem deixar operações ativas sem necessidade, perder a classificação de uma falha e bloquear startup; os detalhes e as fontes estão em `design.md`.

## What Changes

- Correlacionar o resultado assíncrono de cada prompt com seu pedido e distinguir admissão, resultado do prompt e assentamento da Session.
- Tratar cedência, continuação automática e assentamento sem exigir que sempre venha outro turno; preservar mensagens, usage, timing e exatamente uma conclusão por turno visível.
- Usar o resultado estruturado da compactação quando disponível, distinguindo sucesso, ausência de trabalho, cancelamento e falha; preservar o fallback legado e publicar contexto atualizado antes da conclusão.
- Atender pedidos de extensão durante startup de Chats, inclusive antes do fim da negociação; discovery sem interlocutor deve executar com política headless explícita.
- Preservar runtimes antigos, runtime quente, ordem de resume e inspeção de contexto opcional. Não adicionar funcionalidades de produto nem alterar o OMP neste trabalho.

## Capabilities

### New Capabilities

Nenhuma.

### Modified Capabilities

- `omp-turn-continuity`: resultado correlacionado e continuidade após yield, inclusive quando não ocorre um novo run antes do assentamento.
- `omp-compaction-tokens`: resultados estruturados e ausência de trabalho sem falso erro ou conclusão prematura.
- `omp-startup`: extensões interativas não bloqueiam negociação/configuração; discovery possui política sem UI.

## Impact

Principalmente `crates/harness/src/omp/{process,mod,normalize}.rs`, fixtures de subprocesso e `crates/harness/tests/omp_rpc.rs`. A integração com estacionamento/aposentadoria na engine será conferida em `crates/engine/tests/omp_warm_runtime.rs`. Alterações de documentação ficam nos donos desses contratos.

Sem nova dependência, migração persistente, alteração de CRDT ou publicação nesta etapa. O pedido original autorizava a proposta; a autorização de implementação vigente está registrada abaixo.

## Execution authorization

Implementação autorizada em 2026-11-09 no worker Codex, branch `fix/blaze-omp-usage-context-20261109`, base `8c5898efc5438be60978d8dfd74468df95469e38`. Esta autorização supera a prosa histórica de proposta-apenas. Sem push, tag, release, deploy ou substituição de app/OMP. Aceite, marcação oficial das tasks e arquivamento pertencem ao orquestrador.

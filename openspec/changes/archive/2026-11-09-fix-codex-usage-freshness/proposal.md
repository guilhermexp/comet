# Proposal

## Why

As duas contas Codex do Usage exibem percentuais de leituras antigas como se fossem atuais. O diagnóstico de 2026-11-08 confirmou que a release instalada rejeita a resposta saudável por comparar identidades de formatos diferentes; a outra conta responde 401, e ambas preservam seus saldos antigos sem sinalização visível.

## What Changes

- Distinguir quota atual, última leitura conhecida e falha de atualização por conta, inclusive quando existem janelas de quota retidas.
- Validar a resposta contra a identidade de conta/workspace do provedor, mantendo a chave composta local apenas para isolamento do cache; rejeitar respostas realmente alheias sem inventar credencial ausente.
- Mostrar no resumo do Usage quando o saldo está desatualizado ou exige recuperação de autenticação; informar a data da última leitura válida e a ação aplicável no detalhe.
- Recuperar a leitura após credencial voltar a estar disponível ou após autenticação pelo fluxo nativo existente, respeitando backoff e sem trocar a conta ativa para consultar outra.
- Evitar que uma resposta RPC recente esconda uma medição de quota antiga e que falha/sucesso de uma conta contamine outra.
- Preservar o significado de saldo restante e a janela correta. O defeito reproduzido é retenção de dado antigo, não inversão aritmética.

## Capabilities

### New Capabilities

Nenhuma.

### Modified Capabilities

- `usage-widget-freshness-and-tone`: retenção de snapshot com estado de frescor e falha observáveis; recuperação isolada por conta e revalidação da origem da leitura.

## Impact

Principalmente `crates/engine/src/agent_accounts{.rs,/usage.rs}`, `crates/ui/src/details_sidebar/usage.rs` e seus testes. Os campos `AgentAccount.usage_fetched_at` e `usage_error` já existem; não se presume wire novo. Alterar tipos compartilhados somente se a prova demonstrar necessidade.

O plano usa os contratos de autenticação existentes e o backend de quota já utilizado pelo app. O checkout atual difere da release no trecho de validação; a execução deverá conferir a cobertura antes de alterar código ou concluir que basta publicar. Não requer reimplementar OAuth, consumir créditos, executar prompts, limpar perfis nem sincronizar Usage pelo CRDT. Esta etapa entrega diagnóstico, spec e posição na fila; a correção de produto e sua publicação ficam nas tarefas seguintes.

## Execution authorization

Implementação autorizada em 2026-11-09 no worker Codex, branch `fix/blaze-omp-usage-context-20261109`, base `8c5898efc5438be60978d8dfd74468df95469e38`. Esta autorização supera a prosa histórica de proposta-apenas. Sem push, tag, release, deploy ou substituição de app/OMP. Aceite, marcação oficial das tasks e arquivamento pertencem ao orquestrador.

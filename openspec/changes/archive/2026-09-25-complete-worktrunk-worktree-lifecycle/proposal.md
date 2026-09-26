## Why

A unificação entregue em `unify-worktree-lifecycle` deixou duas partes da integração ao Worktrunk que haviam sido aprovadas: cópia opt-in de arquivos ignorados ao criar um checkout e limpeza de branches já integradas ao removê-lo. Sem elas, checkouts novos começam sem caches escolhidos pelo projeto e branches concluídas acumulam mesmo após a remoção.

## What Changes

- Se `.worktreeinclude` existir, copiar do checkout principal apenas arquivos ignorados pelo Git que casem com seus padrões. Nunca copiar arquivos rastreados, metadados Git, worktrees aninhados ou sobrescrever um destino existente. Usar reflink quando disponível, com cópia comum como fallback. Falha deixa o checkout criado e impede o primeiro run, para que o preparo possa ser repetido no mesmo caminho.
- Após remoção segura de um checkout gerenciado, apagar sua branch local somente quando comprovadamente integrada à branch padrão. Preservar branches não integradas, remotas, padrão, em outro worktree ou alteradas durante a operação. Falha dessa limpeza vira aviso e não desfaz a remoção.
- Continuar usando o serviço comum para Chat e Workers; nenhum binário `wt` passa a ser exigido.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `worktree-lifecycle`: acrescenta a cópia opt-in e a limpeza conservadora de branch no ciclo comum.
- `worktree-deletion-safety`: a branch de um checkout removido pode sair quando já foi integrada, mantendo todas as guardas de remoção física.

## Impact

- `crates/workers-unpeel`: serviço de criação e remoção, leitura Git, cópia de arquivos e testes de integração.
- `crates/engine` e `crates/ui`: consumidores do serviço comum e diagnósticos de preparo/remoção; sem novo RPC obrigatório.
- Contratos DOX e verificação OpenSpec do comportamento entregue.

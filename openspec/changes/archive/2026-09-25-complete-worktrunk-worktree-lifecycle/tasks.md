# Tasks

## 1. Cópia opt-in no preparo comum

- [x] 1.1 Implementar seleção `.worktreeinclude` ∩ ignorados pelo Git a partir do checkout principal, com exclusões de metadados/worktrees aninhados e sem seguir links. Verify: testes em repositórios Git temporários.
- [x] 1.2 Implementar reflink por arquivo com fallback, sem sobrescrever destino e com retry idempotente. Verify: testes de destino existente, conteúdo e fallback; macOS verifica clonefile quando disponível.
- [x] 1.3 Executar cópia antes de setup e `pre-start` nas entradas Chat e Workers apenas quando o preparo estiver pendente. Falha mantém `Preparing` e impede run/launch. Verify: testes de integração das duas entradas e retry no mesmo path.

## 2. Branch integrada na remoção

- [x] 2.1 Classificar branch local contra a padrão por mesmo OID, ancestralidade, diff three-dot vazio, árvore igual ou merge simulado sem mudanças. Falha de prova conserva branch. Verify: testes de branches integrada, não integrada e squash-merged.
- [x] 2.2 Após a remoção física, revalidar ref e ocupação e tentar deleção com compare-and-swap; erro vira aviso sem reverter checkout/histórico. Verify: corrida, branch padrão, outro worktree e remoção pelos caminhos Chat/Workers.

## 3. Fechamento

- [x] 3.1 Atualizar DOX, specs e a evidência de verificação, corrigindo texto anterior que diz que branch sempre fica e que cópia não existe. Verify: leitura da cadeia DOX e `openspec validate --strict`.
- [x] 3.2 Rodar testes focados e suítes afetadas de Workers, engine e UI, além de `cargo fmt --all --check`. Verify: resultados registrados.
- [x] 3.3 Revisar fluxo e casos de perda de dados; arquivar a change quando código e testes estiverem concluídos. Verify: revisão e `openspec archive`.

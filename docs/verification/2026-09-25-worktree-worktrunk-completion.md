# Worktree: cópia opt-in e limpeza de branch — verificação

Esta continuação completa as duas partes do ciclo de vida aprovadas e deixadas
pendentes na [primeira verificação](2026-09-25-worktree-lifecycle.md). A execução
ocorreu na branch `worktree-foundation`, num worktree isolado; o perfil real do
usuário não foi migrado.

## Comportamento comprovado

- Chat e Workers usam o mesmo preparo: `.worktreeinclude` seleciona somente
  arquivos ignorados no checkout principal; a cópia termina antes dos comandos
  de setup. Sem arquivo de seleção, não há cópia. Um destino já existente não é
  sobrescrito. Erro no arquivo de seleção deixa o checkout pendente, e o retry
  usa o mesmo caminho.
- A cópia não segue links simbólicos, não entra em repositórios aninhados e não
  substitui arquivos rastreados. No macOS, o teste verifica o caminho
  `clonefile` quando o filesystem temporário o oferece; o fallback conserva
  conteúdo e permissões comuns.
- A remoção física continua recusando checkout sujo, inclusive cache ignorado.
  Um `pre-remove` aprovado pode limpar o cache antes da verificação final.
  Após remover o checkout, uma branch integrada sai; commit não integrado,
  branch padrão, ref movida ou branch ocupada permanecem. A transação Git
  verifica também a ref padrão usada como prova de integração.
- O caminho de remoção de Chat e o de Workers recebem a mesma regra; prune de
  checkout que já desapareceu não apaga branch.

## Limites

- Esta verificação usa repositórios descartáveis e testes Rust. Nenhum teste
  visual novo é necessário: não houve mudança na UI nesta continuação.
- Arquivos ignorados copiados tornam a árvore suja para remoção; precisam ser
  limpos pelo usuário ou por um `pre-remove` aprovado. Isso evita apagar dados
  locais silenciosamente.
- `wt merge`, status semelhante a `wt list` e configuração de usuário do `wt`
  continuam fora deste incremento. O binário `wt` não é exigido.

## Comandos

- `cargo test -p zeron-workers-unpeel`: verde, 248 unitários e todas as
  integrações, incluindo 23 testes de `project_actions`. Após a última
  proteção de ref simbólica e permissões especiais, 11 testes focados de
  `branch_cleanup`, 10 de `copy_ignored` e o teste de retry passaram.
- `cargo test -p zeron-engine -- --test-threads=1`: verde, 498 unitários
  (4 ignorados) e todas as integrações. `m5_repos_diffs_terminals` teve 28
  testes verdes; `worktree_on_run`, 1. A primeira rodada paralela falhou em
  `instance_lock::holder_probe_reports_pid_without_disturbing_the_lock`, fora
  deste ciclo; o mesmo teste passou isolado e na suíte serial.
- `cargo test -p zeron-ui -- --test-threads=1`: verde, 1.963 testes.
- `cargo fmt --all --check`, `git diff --check` e
  `openspec validate complete-worktrunk-worktree-lifecycle --strict`: verdes.

Revisão adicional: a cópia usa descritores de diretório e publicação sem
overwrite para bloquear troca por symlink; o fallback mantém o arquivo privado
enquanto copia. A deleção da branch verifica tanto a ref local quanto a ref
padrão na mesma transação Git e não desreferencia um alias de branch. Não houve
push, merge, release ou deploy.

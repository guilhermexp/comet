# Ciclo de vida unificado de worktrees — verificação

Status: implementação e comprovação local concluídas em 25/09/2026 na branch
`worktree-foundation`. O perfil real do usuário não foi migrado. Não houve push,
merge, release ou deploy.

## Ambiente

- App normal iniciado com `cargo run` no checkout de execução, com perfil isolado
  `/tmp/comet-unify-qa.YhQLIp` e porta IPC `29359`. O outro app/checkout aberto
  não foi encerrado ou alterado.
- Repositório Git descartável `Comet Demo`, com dois worktrees externos, um
  checkout observado e `.config/wt.toml` contendo `pre-start` e `post-start`
  que gravam marcadores.
- Aprovação dos hooks feita na página Settings ▸ Projects desse perfil isolado.

## Observação no app

- Settings ▸ Projects agrupou o checkout principal e os externos, mostrou a
  origem `.config/wt.toml`, os comandos e o estado `Approval required`. Após
  aprovar cada comando, exibiu `Approved` sem sobreposição visual.
- Workers lançou `qa-visual-alpha` e `qa-visual-beta` pelo menu “In a new
  worktree”. As duas linhas apareceram sob `Comet Demo`, cada qual com sessão
  `sh -c "pwd"` e `cwd` próprio em
  `~/.zeron/worktrees/comet-demo-30d0fdcc8b67ad7b/`. Os manifests das sessões
  e `git worktree list --porcelain` confirmaram caminhos e branches distintos.
  Cada checkout recebeu os marcadores `setup-marker` e `post-marker`.
- No Orchestrator, “New worktree” criou o Chat “Resposta simples apenas
  confirmação” em `swift-aspen`, com branch `zeron/resposta-simples-apenas-confirma-o`.
  O Chat respondeu `OK`, e ambos os marcadores apareceram nesse checkout.
- O menu do checkout externo `fix/sidebar` ofereceu Archive, sem remoção física.
  No checkout gerenciado `qa-visual-beta`, a UI mostrou confirmação de remoção;
  ela foi cancelada. Com um Terminal Worker vivo no mesmo checkout e a árvore
  limpa, uma chamada real de `LocalWorkersClient::remove_worktree` no perfil do
  app retornou `Stop active Chats and Workers`; a pasta permaneceu no disco.
  Assim, a recusa de ocupado foi comprovada pelo backend com sessão real, não
  pelo clique destrutivo final da UI.

## Suítes e revisão

- `cargo test -p zeron-workers-unpeel --quiet`: verde, incluindo controller MCP,
  posse, remoção, hooks e registro de projetos.
- `cargo test -p zeron-engine --no-fail-fast --quiet`: verde (498 unitários,
  4 ignorados, mais integrações).
- `cargo test -p zeron-ui --quiet -- --test-threads=1`: 1.963 passaram. A rodada
  paralela teve uma falha de `TestScheduler` em teste de sidebar fora do escopo;
  o mesmo teste passou isolado e na suíte serial.
- `cargo test -p zeron-rpc --quiet`: verde (21 unitários e 13 integrações,
  1 ignorado).
- `cargo fmt --all --check`, `git diff --check` e
  `openspec validate unify-worktree-lifecycle --strict`: verdes.
- Revisão dos contratos de posse, remoção e controller MCP não encontrou
  blocker remanescente após a correção do parser de hooks.
- O app do perfil privado e seus hosts de sessão foram encerrados. Os três
  worktrees criados nesta validação e suas branches foram removidos do
  repositório descartável após a coleta de evidência.
- As quatro specs afetadas foram sincronizadas; `openspec validate --specs`
  passou com 58/58. A validação estrita global ainda aponta dois placeholders
  preexistentes em `global-file-preview` e `macos-dev-identity`.

## Limite deste incremento

Na data desta primeira verificação, o suporte a Worktrunk incluía os hooks de
início e remoção. A cópia opt-in de ignorados e a limpeza conservadora de
branches foram entregues no [incremento seguinte](2026-09-25-worktree-worktrunk-completion.md).
`wt merge`, status estilo `wt list` e configuração de usuário continuam fora
desse ciclo. Nenhum binário `wt` é exigido em runtime.

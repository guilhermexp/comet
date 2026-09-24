# Change: Um ciclo de vida de worktree, para Chat e Workers, no modelo do worktrunk

## Why

O comet cria e apaga worktrees por dois caminhos que nunca se conheceram:

| | Chat (engine) | Workers (`crates/workers-unpeel`) |
|---|---|---|
| Criação | `Repos::create_worktree` (`crates/engine/src/repos.rs`), chamado por `materialize_worktree` (`doc_host.rs`) e pelo RPC `CreateWorktree` | `LocalWorkersClient::create_worktree_at` → `unpeel_core::worktrees::create` (vendor) |
| Raiz | `~/.zeron/worktrees/<repoName>/<adj-noun>` | `~/.unpeel/worktrees/<repo-slug>-<fnv1a>/<branch-slug>` |
| Setup (`.comet/worktree.json`) | não roda | roda (`worktree_config::run_setup_for_project`) |
| Remoção | `Repos::delete_worktree`: aceita qualquer worktree linkado do repo, `git worktree remove --force`, fallback `remove_dir_all`, apaga a branch `zeron/*` | `checkout_lifecycle::validate_removal`: só checkout do app, sob a raiz gerenciada, sem Worker vivo, árvore limpa (inclusive ignorados), HEAD preservado; sem `--force`, sem apagar branch |
| "Em uso" | não considera Workers | considera Workers, não considera Chats |

As consequências são concretas. Um worktree de Worker com trabalho não commitado é apagável pelo RPC da engine, que é `forwardable`. O setup configurado em Settings ▸ Projects é ignorado pela metade do app que cria worktree para Chat. Um Retarget pode apontar um Chat para dentro do checkout onde um Worker está escrevendo. E dois repositórios com o mesmo basename colidem na raiz do Chat.

O [worktrunk](https://worktrunk.dev) (`wt`) resolve esse mesmo problema como ciclo de vida: criar → hooks → trabalhar → remover (apagando a branch só se já foi integrada). Unificar sem esse modelo nos faria redesenhar o serviço de novo na integração. Por isso esta change faz as duas coisas juntas: um único dono de criação, preparo e remoção, **já com a semântica do worktrunk**, lendo o mesmo `.config/wt.toml` que um projeto que usa `wt` já tem. O `wt` não precisa estar instalado: o comet implementa o ciclo nativamente.

## What Changes

- **Um serviço de ciclo de vida de checkout** em `crates/workers-unpeel`, que a engine e a UI já alcançam, é o único ponto que cria, prepara e remove worktree. A engine deixa de chamar `git worktree add/remove` direto, e a fronteira Workers deixa de chamar `unpeel_core::worktrees::create`.
- **Uma raiz canônica para worktree novo: `~/.zeron/worktrees`**, com layout `<repo-slug>-<fnv1a:08x>/<nome>`, que não colide em basename. `ZERON_WORKTREES_DIR` continua sendo o override e passa a valer para os dois lados. As raízes antigas (`~/.zeron/worktrees/<repoName>/…` e `~/.unpeel/worktrees/…`) continuam reconhecidas como gerenciadas. Nenhuma pasta é movida.
- **O nome continua sendo do chamador:** Chat mantém auto-nome `zeron/<adj-noun>` e rename pelo título; Worker mantém a branch dada pelo usuário.
- **Setup em todo worktree que o app cria:** `.comet/worktree.json` (ou `.cursor/worktrees.json`) também roda no worktree criado para um Chat. Falha de setup não desfaz o checkout e chega ao chamador com o comando que falhou.
- **Hooks do worktrunk (`.config/wt.toml`), nativos:** `pre-start`, `post-start`, `pre-remove` e `post-remove`, nos três formatos do `wt` (string, tabela concorrente, pipeline `[[…]]`), com as variáveis de template do `wt` (`{{ branch }}`, `{{ worktree_path }}`, `{{ repo }}`, `{{ repo_path }}`, `{{ base }}`, `{{ commit }}`, filtro `sanitize`…), escapadas para shell. `pre-*` bloqueia; `post-*` roda em segundo plano com log. `pre-remove` que falha aborta a remoção, como no `wt`.
- **Aprovação dos hooks do projeto, como no `wt`:** comandos vindos do `wt.toml` só rodam depois de aprovados em Settings ▸ Projects, e voltam a pedir aprovação quando mudam. Sem aprovação, o worktree nasce e o motivo "hooks não aprovados" aparece para quem criou.
- **Caches por cópia copy-on-write (`copy-ignored`):** quando o repositório tem `.worktreeinclude`, os arquivos ignorados pelo Git que casam com ele (`target/`, `node_modules/`, `.env`…) são clonados do checkout principal para o worktree novo por reflink (APFS/btrfs/XFS; cópia comum onde não há reflink), sem sobrescrever o que já existe. Um `target/` de vários GB passa a custar segundos e quase nada de disco.
- **Uma política de remoção, a mais rígida para o checkout, no modelo do `wt` para a branch:** o `DeleteWorktree` da engine passa pela mesma validação dos Workers.
  - **BREAKING (semântica do RPC):** a engine deixa de usar `--force` e deixa de ter fallback `remove_dir_all` para checkout existente. Remover exige árvore limpa e HEAD preservado em alguma ref. Worktree cuja pasta já sumiu continua sendo podado (`git worktree prune`).
  - **A branch só é apagada quando já foi integrada** à branch padrão (mesmo commit, ancestral, diff three-dot vazio, árvores iguais ou merge que não acrescenta nada), valendo para Chat e Workers. Branch com trabalho não integrado fica. Hoje a engine apaga `zeron/*` com `-D` mesmo sem merge; os Workers nunca apagam.
  - Nenhuma UI deste repo chama `DeleteWorktree` hoje; o impacto fica no contrato do RPC.
- **"Em uso" enxerga os dois lados:** um checkout está em uso quando há um Worker vivo nele **ou** um Chat `Working` com esse `cwd`. Isso vale para remoção (pelos dois caminhos) e para Retarget. Retarget para um checkout com Worker vivo é recusado com motivo visível.

## What this does NOT change

- Identidade de projeto, associação e arquivamento (`workers-repository-identity`) ficam como estão; só a checagem de "em uso" da remoção ganha a metade do Chat.
- O comportamento de Retarget/checkout in place da ADR 0003, o bloqueio por `Working` do próprio Chat e o aviso "worktree · new conversation" ficam como estão.
- O layout de pasta do `wt` (`{{ repo_path }}/../{{ repo }}.{{ branch | sanitize }}`) não é adotado: worktrees novos do app vão para a raiz canônica, e um worktree criado pelo `wt` na mão continua sendo externo (adotável, não apagável), pelo fluxo de adoção que já existe.
- Config de usuário do `wt` (`~/.config/worktrunk/config.toml`) e os hooks `pre/post-switch`, `pre/post-commit`, `pre/post-merge` não entram: não há evento correspondente no app até a próxima change.
- Fica para a change seguinte (`worktree-status-and-merge`): status por checkout no estilo `wt list` (sujo, ahead/behind da branch padrão), o fluxo `wt merge` (commit → squash → rebase → `pre-merge` → fast-forward → remoção) e o isolamento de cada Worker lançado pelo orquestrador no seu próprio worktree.

## Capabilities

### New Capabilities

- `worktree-lifecycle`: onde o app cria worktree, como nomeia, quais raízes reconhece como suas, o setup, os hooks do worktrunk e a cópia de caches que rodam depois, quando a branch é apagada, e o que significa um checkout estar "em uso" para Chat e Workers.

### Modified Capabilities

- `worktree-deletion-safety`: a remoção pela engine passa pela política única — só checkout do app, sem `--force`, árvore limpa, HEAD preservado, `pre-remove` aprovado, branch apagada só se integrada — além de continuar resolvendo o alvo contra os worktrees linkados.
- `workers-repository-identity`: "Actions respect checkout availability and ownership" passa a bloquear a remoção também quando um Chat está rodando no checkout, e a reconhecer as duas raízes gerenciadas.
- `chat-checkout-control`: Retarget para um checkout com Worker vivo é recusado, com o motivo no popover.

## Impact

- `crates/workers-unpeel/src/checkout_lifecycle.rs` (e/ou um módulo novo ao lado): criação, raízes gerenciadas, remoção e o predicado "em uso".
- `crates/workers-unpeel/src/lib.rs`: `create_worktree_at` passa a usar o serviço; `remove_worktree` recebe a metade Chat do "em uso".
- `crates/workers-unpeel/src/worktree_config.rs`: sem mudança de contrato; passa a ser chamado também pelo caminho do Chat.
- Novo `crates/workers-unpeel/src/worktrunk.rs`: leitura de `.config/wt.toml` (hooks nos três formatos), render das variáveis de template, aprovação por hash de comando e o executor `post-*` em segundo plano com log. Novo `copy_ignored` (reflink) ao lado.
- `crates/ui/src/settings/projects.rs`: bloco "Worktrunk hooks" (comandos lidos do `wt.toml`, estado de aprovação, Aprovar) e a linha do `.worktreeinclude`.
- `crates/engine/src/repos.rs`: `create_worktree` e `delete_worktree` delegam ao serviço (via `spawn_blocking`); `rename_worktree_branch` fica.
- `crates/engine/src/doc_host.rs` / `rpc.rs`: `materialize_worktree`, `CreateWorktree`, `DeleteWorktree` e `SetChatCwd` (checagem de Worker vivo).
- `crates/ui/src/{pickers.rs,workers/model.rs}`: motivo de Retarget recusado; passar Chats `Working` à remoção do lado Workers.
- `third_party/unpeel`: **nenhum patch**. O vendor continua com `worktrees::create/remove` e raiz próprios; o comet só para de chamá-los para criar.
- Testes: `crates/engine/tests/m5_repos_diffs_terminals.rs` (DeleteWorktree muda de semântica), `crates/workers-unpeel/tests/project_actions.rs`, testes unitários do serviço.
- Docs: `crates/workers-unpeel/AGENTS.md`, `crates/engine/AGENTS.md`, `CONTEXT.md` (Worker Checkout / raiz), `AGENTS.md` raiz se a raiz de worktree aparecer nos gotchas.

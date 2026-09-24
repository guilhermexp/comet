# Change: Um ciclo de vida de worktree, para Chat e Workers

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

A integração do worktrunk (`wt`), que é a próxima change, precisa de um único dono de criação, setup e remoção para se plugar. Com dois donos, cada ponto de integração teria de ser feito em dobro e com regras diferentes.

## What Changes

- **Um serviço de ciclo de vida de checkout** em `crates/workers-unpeel`, que a engine e a UI já alcançam, é o único ponto que cria, prepara e remove worktree. A engine deixa de chamar `git worktree add/remove` direto, e a fronteira Workers deixa de chamar `unpeel_core::worktrees::create`.
- **Uma raiz canônica para worktree novo: `~/.zeron/worktrees`**, com layout `<repo-slug>-<fnv1a:08x>/<nome>`, que não colide em basename. `ZERON_WORKTREES_DIR` continua sendo o override e passa a valer para os dois lados. As raízes antigas (`~/.zeron/worktrees/<repoName>/…` e `~/.unpeel/worktrees/…`) continuam reconhecidas como gerenciadas. Nenhuma pasta é movida.
- **O nome continua sendo do chamador:** Chat mantém auto-nome `zeron/<adj-noun>` e rename pelo título; Worker mantém a branch dada pelo usuário.
- **Setup em todo worktree que o app cria:** `.comet/worktree.json` (ou `.cursor/worktrees.json`) também roda no worktree criado para um Chat. Falha de setup não desfaz o checkout e chega ao chamador com o comando que falhou.
- **Uma política de remoção, a mais rígida:** o `DeleteWorktree` da engine passa pela mesma validação dos Workers.
  - **BREAKING (semântica do RPC):** a engine deixa de usar `--force`, deixa de ter fallback `remove_dir_all` para checkout existente e **deixa de apagar a branch `zeron/*`**. Remover exige árvore limpa e HEAD preservado em alguma ref. Worktree cuja pasta já sumiu continua sendo podado (`git worktree prune`). Nenhuma UI deste repo chama `DeleteWorktree` hoje; o impacto fica no contrato do RPC.
- **"Em uso" enxerga os dois lados:** um checkout está em uso quando há um Worker vivo nele **ou** um Chat `Working` com esse `cwd`. Isso vale para remoção (pelos dois caminhos) e para Retarget. Retarget para um checkout com Worker vivo é recusado com motivo visível.

## What this does NOT change

- Identidade de projeto, associação e arquivamento (`workers-repository-identity`) ficam como estão; só a checagem de "em uso" da remoção ganha a metade do Chat.
- O comportamento de Retarget/checkout in place da ADR 0003, o bloqueio por `Working` do próprio Chat e o aviso "worktree · new conversation" ficam como estão.
- Nada do worktrunk entra aqui: nem ler `.config/wt.toml`, nem isolamento por worker no orquestrador, nem merge.

## Capabilities

### New Capabilities

- `worktree-lifecycle`: onde o app cria worktree, como nomeia, quais raízes reconhece como suas, o setup que sempre roda depois, e o que significa um checkout estar "em uso" para Chat e Workers.

### Modified Capabilities

- `worktree-deletion-safety`: a remoção pela engine passa pela política única — só checkout do app, sem `--force`, árvore limpa, HEAD preservado, sem apagar branch — além de continuar resolvendo o alvo contra os worktrees linkados.
- `workers-repository-identity`: "Actions respect checkout availability and ownership" passa a bloquear a remoção também quando um Chat está rodando no checkout, e a reconhecer as duas raízes gerenciadas.
- `chat-checkout-control`: Retarget para um checkout com Worker vivo é recusado, com o motivo no popover.

## Impact

- `crates/workers-unpeel/src/checkout_lifecycle.rs` (e/ou um módulo novo ao lado): criação, raízes gerenciadas, remoção e o predicado "em uso".
- `crates/workers-unpeel/src/lib.rs`: `create_worktree_at` passa a usar o serviço; `remove_worktree` recebe a metade Chat do "em uso".
- `crates/workers-unpeel/src/worktree_config.rs`: sem mudança de contrato; passa a ser chamado também pelo caminho do Chat.
- `crates/engine/src/repos.rs`: `create_worktree` e `delete_worktree` delegam ao serviço (via `spawn_blocking`); `rename_worktree_branch` fica.
- `crates/engine/src/doc_host.rs` / `rpc.rs`: `materialize_worktree`, `CreateWorktree`, `DeleteWorktree` e `SetChatCwd` (checagem de Worker vivo).
- `crates/ui/src/{pickers.rs,workers/model.rs}`: motivo de Retarget recusado; passar Chats `Working` à remoção do lado Workers.
- `third_party/unpeel`: **nenhum patch**. O vendor continua com `worktrees::create/remove` e raiz próprios; o comet só para de chamá-los para criar.
- Testes: `crates/engine/tests/m5_repos_diffs_terminals.rs` (DeleteWorktree muda de semântica), `crates/workers-unpeel/tests/project_actions.rs`, testes unitários do serviço.
- Docs: `crates/workers-unpeel/AGENTS.md`, `crates/engine/AGENTS.md`, `CONTEXT.md` (Worker Checkout / raiz), `AGENTS.md` raiz se a raiz de worktree aparecer nos gotchas.

# Tasks

## 1. Serviço: raízes e predicados (`crates/workers-unpeel/src/checkout_lifecycle.rs`)

- [ ] 1.1 `worktrees_root()` (`ZERON_WORKTREES_DIR` não vazio, senão `~/.zeron/worktrees`) e `managed_roots()` (canônica + `unpeel_core::app_paths::worktrees_root()`). Verify: unit — override vazio cai no default; override definido vence.
- [ ] 1.2 `is_managed(path)` sobre caminho canônico contra as duas raízes, e trocar as duas chamadas de `validate_removal` que passam `app_paths::worktrees_root()` para usarem `managed_roots()`. Verify: unit — raiz canônica, raiz Unpeel, caminho externo e symlink externo→raiz (estender `symlink_cannot_make_an_external_checkout_managed`).
- [ ] 1.3 Diretório de repositório `<slug>-<fnv1a:08x>` (reusa `unpeel_core::worktrees::slug`; FNV-1a local). Verify: unit — dois repositórios temporários homônimos em pais diferentes caem em diretórios diferentes; mesmo repositório sempre no mesmo.
- [ ] 1.4 `checkout_in_use(path, busy_chat_cwds, workers)` com o casamento canônico já usado por `remove_owned_checkout`. Verify: unit — Worker vivo bloqueia, Chat `Working` informado bloqueia, nenhum dos dois libera.

## 2. Serviço: criar e remover

- [ ] 2.1 `create_worktree(repo, name, base)`: fetch best-effort, base padrão (`origin/HEAD` → `origin/main|master` → `main|master`), adoção de worktree já registrado na pasta, `git worktree add`, depois `worktree_config::run_setup_for_project`. Verify: unit — repositório temporário com `ZERON_WORKTREES_DIR` em tempdir; cria na raiz canônica; setup com marcador roda; setup que falha mantém o checkout e devolve o comando.
- [ ] 2.2 `remove_managed_checkout(path, repo, in_use)`: `validate_removal` com `managed_roots()`, recheque de uso sob o `CheckoutActionLock`, `git worktree remove` sem `--force`; branch conforme 2A.4. `remove_owned_checkout` passa a usá-lo, mantendo o registro Workers (pending/interrupted/archived) na fronteira. Verify: os testes existentes de `checkout_lifecycle` e `project_actions` seguem verdes.

## 2A. Modelo worktrunk (`crates/workers-unpeel/src/worktrunk.rs`, `copy_ignored.rs`)

- [ ] 2A.1 Leitor de `.config/wt.toml` do checkout principal: `pre-start`, `post-start`, `pre-remove`, `post-remove` nos formatos string/tabela/pipeline; chaves desconhecidas ignoradas. Verify: unit — os três formatos parseiam para a mesma estrutura de etapas; arquivo com chaves de outros hooks parseia sem erro.
- [ ] 2A.2 Render de template (variáveis do D9, filtros `sanitize` e `hash_port`), com escape para shell; token desconhecido → erro que o nomeia. Confirmar o algoritmo do `hash_port` contra o fonte do `wt`; sem confirmação, o filtro não entra. Verify: unit — branch com `/` e espaço, `sanitize`, token desconhecido.
- [ ] 2A.3 Executor: `pre-*` síncrono com o teto do setup, etapas em ordem, concorrência dentro da etapa; `post-*` desanexado, process group próprio, log em `<raiz>/.logs/<slug>/<nome>/<hook>.log`, encerrado na remoção. Verify: unit — cenários "Pipeline roda em ordem" e "pre-start e post-start rodam na criação".
- [ ] 2A.4 Branch na remoção (D10): padrão/outro worktree excluídos; integrada por mesmo commit, ancestral, diff three-dot vazio, árvores iguais ou `merge-tree`; falha vira aviso. Verify: unit — os três cenários de "A branch sai junto… só quando já foi integrada".
- [ ] 2A.5 Aprovação (D12): `~/.zeron/worktree-approvals.json` (respeita o mesmo override de home dos testes), hash SHA-256 por tipo de hook; API `pending_hooks(repo)`/`approve_hooks(repo)`. Verify: unit — cenários "Hook não aprovado não roda" e "Comando alterado volta a pedir aprovação".
- [ ] 2A.6 `copy-ignored` (D11): opt-in por `.worktreeinclude`, ignorados ∩ padrões, exclusões fixas, sem sobrescrever, reflink com fallback (`reflink-copy`). Verify: unit — cenários de "Caches ignorados são clonados quando o projeto pede"; no macOS, conferir que o arquivo clonado não aumenta o uso de disco (`df` antes/depois num arquivo grande).
- [ ] 2A.7 Encadear no serviço: criação = add → copy-ignored → setup → `pre-start` → retorno → `post-start`; remoção = validação → `pre-remove` → remove → branch → `post-remove`. Verify: unit — "pre-remove que falha preserva o checkout".

## 3. Fronteira Workers (`crates/workers-unpeel/src/lib.rs`)

- [ ] 3.1 `create_worktree_at` usa `checkout_lifecycle::create_worktree` no lugar de `unpeel_core::worktrees::create`; `known_before`/rollback continuam como estão. Verify: `cargo test -p zeron-workers-unpeel --test project_actions` com fixture usando `ZERON_WORKTREES_DIR`; o worktree criado está sob o tempdir e nada aparece em `~/.unpeel/worktrees`.
- [ ] 3.2 `remove_worktree` aceita a lista de `cwd` de Chats `Working`. Verify: teste novo em `project_actions.rs` — remoção recusada com um Chat `Working` no checkout, pasta intacta; sem Chat, remove (cenário "A Worker checkout created in the canonical root is removable").
- [ ] 3.3 Fixtures de teste que criavam em `~/.unpeel/worktrees` passam a usar `ZERON_WORKTREES_DIR`; remover a limpeza manual que só existia por causa disso. Verify: `cargo test -p zeron-workers-unpeel` verde e `~/.unpeel/worktrees` sem entradas novas após a suíte.

## 4. Engine (`crates/engine`)

- [ ] 4.1 `Repos::create_worktree` gera o nome como hoje e delega ao serviço em `spawn_blocking` com teto; devolve `Worktree` com o mesmo shape. Verify: `cargo test -p zeron-engine` — testes de repos existentes com `with_worktrees_root` seguem verdes; worktree nasce em `<root>/<slug>-<hash>/<nome>`.
- [ ] 4.2 `materialize_worktree` (`doc_host.rs`): falha de setup não aborta o run — `cwd` aponta para o worktree e a falha vira erro visível no transcript antes da resposta do harness. Verify: integration — Chat em repositório temporário com setup que falha; transcript mostra o comando; run segue; um segundo envio reutiliza o mesmo worktree.
- [ ] 4.3 `Repos::delete_worktree`: mantém `resolve_checkout`; pasta sumida → `worktree prune` como hoje; pasta existente → `remove_managed_checkout` com a lista de Chats `Working` locais (workspace doc + `session_status` + `effective_indicator`). Remove `--force`, o fallback `remove_dir_all` e o `branch -D`. Verify: atualizar `m5_repos_diffs_terminals.rs` (remove e preserva a branch), e novos casos — árvore suja recusada, worktree externo recusado, Worker vivo recusado; `detached_and_vanished_worktrees_stay_deletable` segue verde.
- [ ] 4.4 `Mutate::SetChatCwd` (`rpc.rs`): quando o host do Chat é o device local, recusa se `checkout_in_use` achar Worker vivo no destino. Verify: integration — com Worker vivo no destino o `cwd` não muda e o erro volta; com Worker parado, muda.

## 5. UI (`crates/ui`)

- [ ] 5.1 Remoção pelo lado Workers (`workers/model.rs`) monta a lista de `cwd` de Chats locais `Working` a partir de `AppState::indicator_for` e passa ao cliente. Verify: `cargo test -p zeron-ui` — teste da derivação pura (Chats remotos e não-`Working` ficam fora).
- [ ] 5.2 Conferir que a recusa de Retarget chega ao `switch_error` do popover pelo caminho existente de `switch_live_worktree`; sem UI nova. Verify: leitura + `scripts/dev-demo.sh` (ou `cargo run`) com Worker vivo num worktree.
- [ ] 5.3 Settings ▸ Projects: bloco "Worktrunk hooks" (comandos lidos por tipo, estado aprovado/pendente, botão Aprovar) e linha informando se `.worktreeinclude` existe. Verify: `cargo test -p zeron-ui` para a derivação do estado; `cargo run` e olhar a página num repo com `wt.toml`.

## 6. Docs e fechamento

- [ ] 6.1 DOX: `crates/workers-unpeel/AGENTS.md` (serviço, raízes, "em uso", fim do `create` do vendor, `worktrunk.rs`, aprovação, `copy-ignored`), `crates/engine/AGENTS.md` (`delete_worktree` sem force/branch; criação delegada; setup no Chat), `CONTEXT.md` (Worker Checkout e raiz canônica), `AGENTS.md` raiz se a raiz entrar nos gotchas. Verify: leitura — cada contrato descreve o código.
- [ ] 6.2 Linhas da Test Coverage Matrix para os testes novos nos AGENTS.md donos. Verify: leitura.
- [ ] 6.3 `cargo test -p zeron-workers-unpeel`, `cargo test -p zeron-engine`, `cargo test -p zeron-ui`, `cargo fmt --all --check`, `openspec validate unify-worktree-lifecycle --strict`. Verify: tudo verde.

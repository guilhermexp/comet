## Context

A motivação e o quadro dos dois caminhos estão em `proposal.md` (Why). O que molda o desenho:

- **Camadas:** `engine` já depende de `zeron-workers-unpeel` (`rpc.rs` usa `registered_projects`). O
  contrário não pode existir. Então o dono comum só pode morar em `crates/workers-unpeel` (ou abaixo).
- **`crates/workers-unpeel` é síncrono:** Git passa por `git_command::run_git` / `run_git_mutation`,
  com deadline. A engine é tokio e não pode bloquear (`openspec/project.md`: trabalho síncrono vai para
  `spawn_blocking` com timeout).
- **O vendor fica intocado:** `unpeel_core::worktrees::{create, remove, is_managed}` fixam a raiz
  `~/.unpeel/worktrees`. Nenhum consumidor do comet depende de o vendor criar ali além da nossa própria
  chamada em `create_worktree_at`.
- **A raiz legada do Chat já é a canônica:** o layout antigo do Chat
  (`~/.zeron/worktrees/<repoName>/<nome>`) mora sob o mesmo diretório escolhido como canônico. "Sob a
  raiz" cobre os dois layouts sem regra extra.
- **`SetChatCwd` é uma `Mutate`** aplicada ao doc do workspace pelo engine local (`rpc.rs`), não um
  comando encaminhado ao host do Chat.
- **Remoção do lado Workers já é a política mais rígida** (`checkout_lifecycle::validate_removal` +
  `remove_owned_checkout`). Ela vira a política única.

## Goals / Non-Goals

**Goals:**
- Um módulo dono de: raízes gerenciadas, criação (layout + setup), predicado "em uso" e remoção.
- Engine e fronteira Workers chamam só esse módulo para criar e remover worktree.
- Nenhuma pasta existente é movida; nada é reescrito no registro.

**Non-Goals:**
- Unificar o *registro* (Workers `app-state.json` vs Chat no CRDT). Continua cada lado com o seu.
- Mudar nomes: Chat segue `zeron/<adj-noun>` + rename por título (`rename_worktree_branch`), Worker
  segue a branch do usuário.
- Encaminhar `SetChatCwd` ao host do Chat.
- Qualquer coisa do worktrunk.

## Decisions

### D1. O serviço estende `checkout_lifecycle.rs`

`checkout_lifecycle.rs` já é dono de "archive/restore e remoção guardada; coordenação de
launch/restart/remoção" e tem o `CheckoutActionLock` entre processos. Criação e raízes entram ali, em
vez de num módulo novo (regra DOX: estender em vez de criar helper paralelo). Superfície pública,
síncrona:

- `worktrees_root() -> PathBuf` — `ZERON_WORKTREES_DIR` não vazio, senão `~/.zeron/worktrees`.
- `managed_roots() -> [PathBuf; 2]` — canônica + `unpeel_core::app_paths::worktrees_root()`.
- `is_managed(path) -> bool` — caminho canônico sob alguma raiz; symlink resolvido antes (mesma regra
  já testada em `symlink_cannot_make_an_external_checkout_managed`).
- `create_worktree(repo, WorktreeName, base) -> CreatedWorktree` — `git worktree add` + setup; devolve
  path, branch e o resultado do setup (`SetupOutcome`).
- `checkout_in_use(path, busy_chat_cwds: &[PathBuf], workers: &WorkersBootstrap) -> Option<Reason>`.
- `remove_managed_checkout(path, repo, InUse) -> Result<()>` — o miolo de hoje de
  `remove_owned_checkout` sem a parte de registro Workers, que continua na fronteira.

*Alternativa rejeitada:* colocar o serviço na engine e fazer Workers chamarem a engine. Inverte a
camada (`workers-unpeel` não pode depender de `engine`) e o caminho Workers roda na UI sem engine.

### D2. Layout: `<slug>-<fnv1a:08x>/<nome>` sob a raiz canônica

Mesmo esquema do Unpeel (`slug` do basename + FNV-1a do toplevel canônico), aplicado à raiz nova.
Resolve a colisão de basename do layout antigo do Chat. Reutiliza `unpeel_core::worktrees::slug`
(público). O hash é recalculado no comet — é uma função de 8 linhas, e `fnv1a` é privada no vendor;
exportá-la seria patch de vendor só por isso.

*Alternativa rejeitada:* manter `<repoName>/<nome>`. Dois clones chamados `comet` dividiriam o
diretório e o gerador de nome só evita colisão de pasta, não de origem.

### D3. Workers deixam de chamar o `create` do vendor

`create_worktree_at` troca `unpeel_core::worktrees::create` por `checkout_lifecycle::create_worktree`,
preservando o que o vendor fazia e o comet depende: fetch best-effort, base padrão
(`origin/HEAD` → `origin/main|master` → `main|master`) e *adotar* a pasta quando já existe um worktree
registrado nela. A detecção de adoção (`known_before`) e o rollback só-do-que-criou continuam na
fronteira, como hoje. `WorkersWorktreeResult` não muda de forma.

### D4. A engine delega, em `spawn_blocking`, com deadline

`Repos::create_worktree` e `Repos::delete_worktree` passam a chamar o serviço dentro de
`disposable_worker`/`spawn_blocking`, com o mesmo teto que já protegem o fallback de hoje. A engine
continua fazendo a resolução de alvo que tem hoje (`resolve_checkout` contra o porcelain) — é a
autorização por repositório de um RPC `forwardable` — e só então chama a remoção do serviço com o
caminho resolvido. O caminho "pasta já sumiu → `worktree prune`" continua na engine, porque não há
checkout para validar.

O nome do Chat continua sendo gerado na engine (`ADJECTIVES`/`NOUNS`, colisão contra pasta e branch)
e entra no serviço como nome explícito.

### D5. "Em uso": Workers do serviço, Chats do chamador

O serviço sabe consultar Workers vivos (`LocalWorkersClient::bootstrap`, `session.is_live()`, com o
mesmo casamento por caminho canônico de `remove_owned_checkout`). Ele **não** sabe de Chats: isso é
estado da engine/UI. Por isso a metade Chat entra como parâmetro — a lista de `cwd` canônicos de Chats
hospedados neste device com `effective_indicator == Working` (regra já compartilhada em
`zeron_proto::view`):

- **Engine (`DeleteWorktree`):** monta a lista a partir do workspace doc + `Sessions::session_status`.
- **UI (remoção pelo lado Workers):** monta a partir de `AppState` (`indicator_for`), filtrando Chats
  cujo host é o device local.

*Alternativa rejeitada:* o serviço ler o doc do workspace. Puxaria `zeron-doc`/`loro` para dentro de
`workers-unpeel`, que é exatamente o que o AGENTS.md dela evita (`owner`/`repo` não são derivados ali
pelo mesmo motivo).

### D6. Setup do Chat: roda antes do run; falha não aborta o run

`materialize_worktree` chama o serviço, que roda o setup. Se o setup falha, o worktree fica (mesma
regra dos Workers), o `cwd` do Chat aponta para ele e o run **segue**, com a falha registrada como
erro visível no transcript antes da primeira resposta do harness.

*Por que seguir e não abortar:* abortar deixaria um worktree criado sem Chat apontando para ele; o
próximo envio criaria outro (a reutilização em `materialize_worktree` depende de `chat.cwd` já ser o
worktree). Um `bun install` quebrado num worktree que o agente consegue consertar é melhor que
worktrees órfãos. No lado Workers a regra continua a de hoje (`create_worktree_and_launch` recusa o
launch), porque ali o registro do projeto já guarda o checkout e o usuário relança à mão.

### D7. `DeleteWorktree` perde `--force`, o fallback recursivo e a deleção de branch

Consequência direta de "uma política, a mais rígida". O teste de integração em
`m5_repos_diffs_terminals.rs` que hoje prova "worktree removido e branch `zeron/…` apagada" passa a
provar "removido e branch preservada". O `WORKTREE_REMOVE_TIMEOUT` continua valendo para o
`git worktree remove`.

### D8. Retarget: checagem no handler de `SetChatCwd`, só quando o Chat é local

O handler da `Mutate::SetChatCwd` em `rpc.rs` consulta o serviço (`checkout_in_use` só com a metade
Workers) **quando o host do Chat é o device local**; senão aplica sem checar (limitação registrada na
spec `chat-checkout-control`). O erro volta pelo caminho que `switch_live_worktree` já trata e cai no
`switch_error` do popover — nenhuma UI nova.

## Risks / Trade-offs

- **[`DeleteWorktree` passa a recusar o que antes apagava]** → Nenhuma UI deste repo chama o RPC; o
  erro diz o que preservar. Registrado como BREAKING no proposal.
- **[Setup roda na engine, inclusive disparado por outro device]** → Um Chat hospedado aqui e dirigido
  de outro device roda o setup do repositório local, que é arquivo versionado do próprio repo — o
  mesmo nível de confiança do agente que vai rodar ali. Timeout e limpeza do process group já existem
  no executor.
- **[Setup de até 5 min atrasa o primeiro run de um Chat em worktree novo]** → Aceito; é o mesmo teto
  dos Workers e só acontece na criação.
- **[Retarget em Chat de outro device não checa Workers]** → Explícito na spec; encaminhar a `Mutate`
  ao host é uma change à parte.
- **[Testes de Workers escreviam em `~/.unpeel/worktrees`]** (AGENTS.md: "Teste que cria worktree limpa
  o que criou") → Com o override valendo para Workers, os testes passam a usar tempdir via
  `ZERON_WORKTREES_DIR`.
- **[Duas raízes gerenciadas para sempre]** → Custo de uma comparação a mais; a raiz Unpeel só deixa de
  ser necessária quando não houver mais worktree lá, e isso não é decidido aqui.

## Migration Plan

Sem migração de dados: worktrees existentes ficam onde estão e seguem gerenciados. Rollback é reverter
a change — os worktrees criados na raiz nova continuam worktrees Git normais, e o comet antigo os vê
como externos (arquiváveis, não apagáveis), que é o lado seguro.

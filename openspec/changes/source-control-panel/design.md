## Context

Git no engine hoje é leitura mais três mutações de topologia: `Repos` (`crates/engine/src/repos.rs`) lista repos/branches/refs/histórico, faz `fetch --all`, `switch_ref` e worktrees; `CheckoutDiffSync` (`crates/engine/src/diff_sync.rs`) observa o working tree. Todo spawn de git passa por `ProcessRunner` (`crates/engine/src/process.rs`), com `kill_on_drop: false` em `Repos::git` e teto de 15 min.

Duas lacunas que moldam o desenho:

- `CheckoutDiff` é `git diff HEAD` — **staged e unstaged misturados** — e o porcelain só é lido para descobrir `??` (`diff_sync.rs:1223-1261`). Não dá para derivar as duas seções do painel a partir dele.
- Ahead/behind existe, mas em `Repos::history_comparison` (`repos.rs:740`, `rev-list --left-right --count`), servindo o pill do History. É a mesma conta que o cabeçalho de Source Control precisa.

Na UI, `changes.rs` é o viewer de diff do pane direito (`shell.rs` `ToggleChanges`, `right_tabs`), e a Details Sidebar tem `DetailsTab::{Details, Files}` (`details_sidebar/context.rs:15`).

Motivação: proposal.md — Why. Contrato observável: `specs/source-control/spec.md`.

## Goals / Non-Goals

**Goals**

- Uma fonte de verdade para o estado de git de um checkout, com índice e worktree separados, servindo badge, seções e cabeçalho.
- Operações de índice e de sincronização explícitas, testáveis sem repositório real.
- Reuso do viewer de diff existente; a aba nova não desenha diff.

**Non-Goals**

- Geração de mensagem de commit por modelo (o ícone de varinha do app de referência).
- Merge, rebase, resolução de conflito, stash, cherry-pick, amend, revert.
- Stage por hunk ou por linha.
- Criar/trocar branch pela aba nova — isso continua no card Workspace via `SwitchRef`.
- Criar PR — já existe caminho próprio (`WatchCheckoutChangeRequest` + `gh`).
- Mudar o contrato de `CheckoutDiff` ou o comportamento do painel Changes.

## Decisions

### D1 — Estado novo em stream próprio, não um campo a mais em `CheckoutDiff`

Entra `WatchCheckoutStatus` (stream) + `GetCheckoutStatus` (one-shot) devolvendo `CheckoutStatus { checkoutId, cwd, branch, upstream, ahead, behind, files: [CheckoutStatusFile { path, oldPath, index, worktree }] }`, onde `index` e `worktree` são o par XY do porcelain normalizado em enum.

Alternativa rejeitada: engordar `CheckoutDiff` com `staged`. `CheckoutDiff` carrega patch e é pesado (checksum, truncated, 16 MiB de teto); status precisa ser leve e frequente, e quem consome um não consome o outro. Manter separado também deixa o painel Changes intacto.

O produtor é o mesmo watcher que já observa o checkout (`CheckoutDiffSync`), num segundo canal: o evento de fs que dispara a recaptura de diff dispara a de status. Nada de timer de 2 s.

### D2 — `git status --porcelain=v1 -z` parseado em XY, sem heurística

O parse já existe pela metade (`diff_sync.rs:1228`, só `??`). Vira um parser completo, com `-z` (path com espaço/acento/quebra de linha), rename com par de paths, e `--no-optional-locks` como já se usa. Cada caractere X e Y vira enum (`Unmodified, Modified, Added, Deleted, Renamed, Copied, Untracked, Unmerged`), não `String` — status é decisão, não texto (a lição que `DiffFileSummary.status: String` já cobra).

### D3 — Uma operação de git por método RPC, todas por `ProcessRunner`

`StageFiles`, `UnstageFiles`, `DiscardFiles`, `CommitCheckout`, `PushCheckout`, `PullCheckout`, `SyncCheckout`. Stage/unstage/discard recebem lista de paths (a ação em massa é a mesma chamada com a lista da seção), o que evita um segundo método `*All` e mantém a operação atômica do ponto de vista do usuário.

Comandos: `add --` (stage, cobre delete), `restore --staged --` (unstage), `restore --worktree --` para tracked e remoção direta para untracked (discard), `commit -m` (commit), `push` / `push --set-upstream` (push/publish), `pull --ff-only` (pull), e sync = pull --ff-only seguido de push.

Alternativa rejeitada: um `GitCommand` genérico com argv. Abriria execução arbitrária de git pela borda RPC — exatamente o que a jaula de path do resto do engine evita.

### D4 — Discard de untracked remove o arquivo, e a UI diz isso antes

`git restore` não toca untracked; `git clean -f` sobre lista de paths é fácil de errar. A remoção é feita diretamente, com o mesmo cuidado de path do resto do engine: path relativo validado, nada de `..`, nada de sair do checkout, `.git` recusado. A confirmação da UI nomeia o arquivo e diz que apaga — é requisito de spec, não cortesia.

### D5 — Sync é fast-forward ou nada

`pull --ff-only` depois `push`. Sem merge, sem rebase, sem `--force`. Divergência que não faz fast-forward aborta com a saída do git. Um app que decide sozinho entre merge e rebase quando o usuário só queria sincronizar cria commit que ninguém pediu.

Nenhum caminho implícito de rede: ahead/behind continua vindo de refs locais (`repos.rs:740`), e só Sync/Publish falam com o remoto.

### D6 — Aba nova na Details Sidebar, diff no pane que já existe

`DetailsTab` ganha `SourceControl`; a aba renderiza cabeçalho, mensagem, botões e as duas seções, e ao clicar num arquivo emite o evento que o shell já usa para focar o painel Changes no working tree. Nada de segundo viewer.

Alternativa rejeitada: colocar tudo dentro de `changes.rs`. O pane direito é um host de tabs de conteúdo grande; a lista de staging é navegação persistente, mora com Files, e é onde o badge faz sentido.

`crates/ui/src/details_sidebar/source_control.rs` é arquivo novo: a lógica de derivação (seções, letra, badge, habilitação de botão) fica em funções puras testáveis, como `workers/session_menu.rs` faz para menu.

### D7 — Cobertura sem repositório real, pelo `ProcessRunner` fake

O padrão da casa é `Repos::with_runner` + fake que grava argv (`repos.rs:2619` `FakeGit`, `source_control.rs:769` `FakeProcessRunner`). As operações novas se provam assim: que o argv é exatamente o esperado, que path com espaço não vaza, que discard de untracked não chama `restore`. Os cenários de efeito real (commit consome índice, stage move de seção) usam checkout temporário de verdade no teste de integração, como `crates/engine/tests/` já faz.

### D8 — Ordem em relação à change de Files

`workspace-file-management` toca `entities.rs`, `method.rs`, `rpc.rs` e `details_sidebar/`. Esta change toca os mesmos quatro. Ela entra **depois**, sobre a base já aplicada — não em paralelo. É a razão de `DetailsTab::SourceControl` não estar na change de Files.

## Risks / Trade-offs

- **`push` pode pedir credencial e travar** → `ProcessRunner` roda com `GIT_TERMINAL_PROMPT=0` e teto de tempo; falha vira erro visível com stderr, não spinner eterno.
- **Status frequente em repo grande custa** → o status é disparado pelo mesmo evento de fs já debounced do diff, e `--no-optional-locks` evita disputa com git externo.
- **Discard apaga trabalho sem undo** → confirmação obrigatória nomeando o arquivo; a ação em massa exige a mesma confirmação e lista o que será apagado.
- **Race entre commit e o watcher** → status é recapturado depois do comando terminar; a UI não adivinha o estado pós-comando, ela relê.
- **Divergência de conceito com `CheckoutDiff`** (dois retratos do mesmo checkout que podem discordar por milissegundos) → o painel Changes continua dono do diff e a aba continua dona do status; nenhuma tela mistura os dois números.

## Migration Plan

Nenhuma migração de dado. Peer com engine antigo não conhece os métodos novos: a aba mostra o mesmo estado de incompatibilidade que Files já usa, sem fallback local.

## Open Questions

Nenhuma.

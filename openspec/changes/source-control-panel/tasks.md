## 1. Estado de git

- [x] 1.1 Em `crates/proto/src/entities.rs`, adicionar `CheckoutStatus` (`checkoutId`, `cwd`, `branch`, `upstream`, `ahead`, `behind`, `files`) e `CheckoutStatusFile` (`path`, `oldPath`, `index`, `worktree`) com os estados XY como enum serializada, mais os params de `GetCheckoutStatus`/`WatchCheckoutStatus` — verificar com `cargo test -p zeron-proto`.
- [x] 1.2 Em `crates/engine/src/diff_sync.rs`, substituir a leitura parcial de porcelain por um parser completo de `git --no-optional-locks status --porcelain=v1 -z` (rename com par de paths, path com espaço/acento, untracked distinto de added), com testes unitários de parse sobre buffers literais — `cargo test -p zeron-engine diff_sync`.
- [x] 1.3 Ligar o estado ao watcher existente do checkout, publicando `CheckoutStatus` no mesmo evento de fs que recaptura diff, e reusar `Repos::history_comparison` para ahead/behind sem nenhum comando de rede; teste provando que nenhum `fetch` é executado — `cargo test -p zeron-engine`.
- [x] 1.4 Registrar `GetCheckoutStatus` e `WatchCheckoutStatus` em `crates/rpc/src/method.rs` e os braços em `crates/engine/src/rpc.rs`; cobrir por RPC in-memory em `crates/engine/tests/source_control_ops.rs` que o status separa staged de unstaged e marca untracked.

## 2. Operações de índice

- [x] 2.1 Em `crates/engine/src/repos.rs`, implementar `stage_files`, `unstage_files` e `discard_files` (tracked por `restore --worktree`, untracked por remoção validada), todas com lista de paths validada contra o checkout e executadas por `ProcessRunner`.
- [x] 2.2 Registrar `StageFiles`, `UnstageFiles`, `DiscardFiles` em `method.rs` + braços em `rpc.rs`.
- [x] 2.3 Provar por fake de `ProcessRunner` (padrão `FakeGit`, `repos.rs:2619`) que o argv é exatamente o esperado, que path com espaço não vaza e que discard de untracked não chama `restore` — `cargo test -p zeron-engine repos`.
- [x] 2.4 Em `crates/engine/tests/source_control_ops.rs`, provar efeito real em checkout temporário: stage move de seção, unstage preserva worktree, discard preserva staged, discard de untracked apaga, path com `..` recusado — `cargo test -p zeron-engine --test source_control_ops`.

## 3. Commit e sincronização

- [x] 3.1 Implementar `commit` em `repos.rs` recusando mensagem em branco e índice vazio antes de invocar git, com erro tipado distinto para cada caso.
- [x] 3.2 Implementar `push`, `pull` (`--ff-only`) e `sync` (pull seguido de push), com publish (`--set-upstream`) quando não há upstream, `GIT_TERMINAL_PROMPT=0` e erro carregando o stderr do git.
- [x] 3.3 Registrar `CommitCheckout`, `PushCheckout`, `PullCheckout`, `SyncCheckout` em `method.rs` + braços em `rpc.rs`.
- [x] 3.4 Cobrir em `crates/engine/tests/source_control_ops.rs`: commit consome o índice e aumenta ahead, mensagem vazia recusada, índice vazio recusado, sync fast-forward contra remoto local, divergência aborta sem merge, publish define upstream.

## 4. Aba Source Control

- [x] 4.1 Adicionar `DetailsTab::SourceControl` em `crates/ui/src/details_sidebar/context.rs`, incluir a pill na toolbar de `view.rs` com badge de quantidade e manter a persistência de aba ativa; teste unitário do badge derivado do estado.
- [x] 4.2 Criar `crates/ui/src/details_sidebar/source_control.rs` com as funções puras de derivação (seções Staged Changes / Changes, letra por par XY, habilitação de Commit e de Sync/Publish, rótulo do botão) e seus testes — `cargo test -p zeron-ui source_control`.
- [x] 4.3 Renderizar o cabeçalho (branch, ahead/behind), a caixa de mensagem com `ComposerInput::with_single_line`, os botões Commit e Sync/Publish, e as duas seções com ações por arquivo e por seção.
- [x] 4.4 Ligar cada ação ao RPC correspondente, relendo o status após o comando terminar em vez de adivinhar o estado; erro do git mostrado sem deixar botão em progresso.
- [x] 4.5 Clicar num arquivo emite o evento que foca o painel Changes existente no escopo working tree; teste unitário do evento emitido.
- [x] 4.6 Confirmação de discard (individual e em massa) nomeando o arquivo e avisando quando apaga untracked; teste unitário de que recusar não dispara comando.
- [x] 4.7 Estado vazio explícito para diretório que não é repositório git, sem botão de ação; teste unitário da composição.

## 5. Fechamento

- [x] 5.1 Rodar `cargo fmt --all` e `cargo test --workspace` uma única vez, no fim.
- [x] 5.2 DOX pass: `crates/engine/AGENTS.md` (git deixa de ser só leitura/topologia; matriz de teste), `crates/rpc/AGENTS.md` (métodos novos), `crates/ui/AGENTS.md` (terceira aba; Changes segue sendo só viewer).
- [x] 5.3 `openspec validate source-control-panel --strict` verde e evidência registrada.

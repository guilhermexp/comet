## Why

O app mostra diff e histórico, mas não deixa agir sobre o estado do git: não existe stage, unstage, discard, commit, push ou pull em lugar nenhum do engine (`Repos` só lê, faz fetch, troca ref e cria worktree). Quem trabalha com agentes vê a mudança acontecer no painel Changes e precisa sair para um terminal para transformar isso em commit. Falta também a informação que decide a próxima ação: quais arquivos estão staged, quais não estão, e quantos commits o branch está à frente/atrás do upstream — hoje ahead/behind só existe no pill do History.

## What Changes

- **Engine ganha mutação de git**: stage e unstage por arquivo e em massa, discard (tracked via restore, untracked via remoção), commit com mensagem, push, pull fast-forward e sync (pull seguido de push) — tudo por `ProcessRunner`, como o resto do git do engine.
- **Status estruturado por arquivo**: um estado de checkout com `git status --porcelain -z` parseado em XY (index e worktree separados), mais branch, upstream e contadores ahead/behind, observável em stream. Hoje `CheckoutDiff` mistura staged e unstaged (`git diff HEAD`) e descarta o porcelain fora de `??`.
- **Nova aba Source Control na Details Sidebar**, ao lado de Details e Files, com badge de quantidade de mudanças: cabeçalho com branch e ahead/behind, caixa de mensagem de commit, botão Commit, botão Sync/Publish, e as seções Staged Changes e Changes com letra de status por arquivo (A/M/D/R/U).
- **Ações por arquivo e por seção**: stage, unstage e discard individuais; stage all, unstage all e discard all na barra da seção.
- **Clicar num arquivo abre o diff dele no painel Changes** que já existe, no escopo working tree — sem um segundo visualizador de diff.
- Discard SHALL pedir confirmação; discard de arquivo untracked apaga o arquivo e é dito na confirmação.

## Capabilities

### New Capabilities
- `source-control`: estado de git por checkout (status XY, branch, upstream, ahead/behind) e as operações de índice e de sincronização — stage, unstage, discard, commit, push, pull, sync — mais a superfície de Source Control da Details Sidebar.

### Modified Capabilities
<!-- Nenhuma. `transcript-and-changes-navigation` e `git-history-controls` seguem inalterados: o painel Changes continua sendo leitura de diff e o History continua dono do ahead/behind do seu pill. -->

## Impact

- `crates/engine/src/repos.rs` — operações de índice, commit e sincronização; `crates/engine/src/diff_sync.rs` — parse XY do porcelain para o estado novo, sem mudar o contrato de `CheckoutDiff`.
- `crates/proto/src/entities.rs` — tipos de status de checkout e params das operações.
- `crates/rpc/src/method.rs` + `crates/engine/src/rpc.rs` — métodos e handlers novos.
- `crates/ui/src/details_sidebar/{context.rs,view.rs}` — terceira aba, badge e persistência da aba ativa; `crates/ui/src/details_sidebar/source_control.rs` (novo) — a superfície.
- `crates/ui/src/changes.rs` — só o ponto de entrada para focar o diff de um arquivo; o viewer não muda.
- DOX: `crates/engine/AGENTS.md`, `crates/rpc/AGENTS.md`, `crates/ui/AGENTS.md`.
- Depende da change `workspace-file-management` estar aplicada: as duas tocam `entities.rs`, `method.rs`, `rpc.rs` e `details_sidebar/`.
- Sem dependência nova. Nenhuma operação de rede implícita: fetch/pull/push só por ação explícita do usuário.

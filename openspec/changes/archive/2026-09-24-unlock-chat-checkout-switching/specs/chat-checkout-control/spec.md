## ADDED Requirements

### Requirement: O Chat Checkout tem um único controle, no card Workspace

O controle de Ref SHALL existir apenas na primeira linha do card Workspace do Details
sidebar, como trigger interativo (`Stateful<Div>` com id, cursor e handler) que abre o
popover de Refs. O footer do composer SHALL deixar de renderizar qualquer controle ou
label de Ref, nos dois estados (draft e Chat vivo), e `composer_footer_right_order()`
SHALL passar a ser `[Model]`. O chip de checkout kind (`PickerKind::Checkout`) e o
cluster de model/effort SHALL permanecer no footer.

#### Scenario: Footer sem controle de Ref
Test: unit — `cargo test -p zeron-ui pickers` sobre `composer_footer_right_order`.

- **WHEN** o footer do composer é montado, com ou sem Chat selecionado
- **THEN** nenhum elemento de Ref é produzido
- **AND** a ordem do cluster da direita é `[Model]`

#### Scenario: Um único host de popover
Test: unit — asserção de que o caminho do footer não constrói mais o overlay de `PickerKind::Branch`.

- **WHEN** o popover de Refs é montado
- **THEN** o único `attach_overlay*` que consome `PickerKind::Branch` é o do card Workspace

#### Scenario: Trigger visível no card
Test: none — render gpui sem harness; validação visual por `scripts/dev-demo.sh`.

- **WHEN** o card Workspace renderiza em `DetailsMode::Orchestrator`
- **THEN** a linha de Branch tem hover, cursor de clique e chevron
- **AND** clicar abre o popover de Refs abaixo do trigger, dentro da largura da sidebar

### Requirement: O label do controle vem do Chat, nunca de `config.branch`

O texto do controle SHALL derivar de `DetailsContext.branch` (cópia de `chat.branch`) ou,
quando não há Chat, do Ref efetivo do draft. `Pickers::ref_label` e `Pickers::selected_ref`
SHALL NOT ser a fonte do label num Chat vivo, porque leem `config.branch`/HEAD e
`config.branch` só é resetado em troca de Space.

#### Scenario: Trocar de Chat não vaza o Ref anterior
Test: unit — derivação pura do label a partir de `DetailsContext` + estado do Pickers.

- **GIVEN** o Chat A está no Ref `feature/a` e o Chat B no Ref `main`
- **WHEN** o usuário seleciona A e depois B
- **THEN** o controle mostra `feature/a` e depois `main`
- **AND** nunca mostra o Ref do Chat anterior nem o HEAD do repo

#### Scenario: Chat sem Ref
Test: unit — mesma derivação, `chat.branch = None`.

- **WHEN** o Chat selecionado não tem `branch`
- **THEN** o controle mostra o placeholder de ausência, não um Ref inventado

### Requirement: Ref com worktree faz Retarget; Ref sem worktree faz checkout no cwd do Chat

Num Chat vivo, escolher um Ref cujo `worktree_path` existe SHALL emitir
`SetChatCwd(chat_id, worktree_path)` e SHALL NOT executar git checkout. Escolher um Ref
sem worktree SHALL emitir `SwitchRef` com `repoPath = chat.cwd` — nunca `space.path` — e
o label SHALL ser reconciliado pelo HEAD-watcher, não escrito pela UI.

#### Scenario: Ref com worktree
Test: unit — decisão pura de `pick_ref` (row com `worktree_path` + Chat vivo → Retarget).

- **WHEN** o usuário escolhe um Ref marcado `worktree` num Chat vivo
- **THEN** o Chat passa a apontar para aquela pasta
- **AND** nenhum comando git de checkout é emitido

#### Scenario: Ref sem worktree num Chat que vive num worktree
Test: unit — decisão pura de `pick_ref` asseverando o `repoPath` emitido.

- **GIVEN** um Chat cujo `cwd` difere do `space.path`
- **WHEN** o usuário escolhe um Ref sem worktree
- **THEN** o `SwitchRef` emitido carrega o `cwd` do Chat
- **AND** o `space.path` não é tocado

#### Scenario: Draft segue igual
Test: unit — `pick_ref` sem Chat selecionado.

- **WHEN** não há Chat selecionado
- **THEN** a semântica de draft atual permanece byte-a-byte a mesma

### Requirement: Retarget declara a perda de continuidade antes do clique

Uma row de Ref que faria Retarget SHALL declarar, na própria row, que o próximo run abre
conversa nova do harness (resume é escopo de cwd). A informação SHALL NOT aparecer apenas
depois da ação.

#### Scenario: Row de worktree
Test: none — render gpui sem harness; validação visual por `scripts/dev-demo.sh`.

- **WHEN** o popover lista um Ref com worktree e há Chat vivo
- **THEN** a row diz que a conversa do harness recomeça naquele diretório

### Requirement: A troca é bloqueada enquanto o Chat está Working, com motivo visível

Enquanto `AppState::indicator_for(chat_id, now)` for `Working`, o trigger SHALL renderizar
estado desabilitado, as rows SHALL ser inertes, e o popover SHALL exibir o motivo.
`pick_ref` SHALL manter a mesma guarda como defesa. Árvore suja SHALL NOT receber trava
própria: o erro do git SHALL aparecer na faixa de erro existente do popover.

#### Scenario: Agente rodando
Test: unit — predicado de habilitação sobre `indicator_for` com `now` injetado.

- **GIVEN** o Chat selecionado está `Working`
- **WHEN** o usuário abre o popover e clica numa row
- **THEN** nada é emitido
- **AND** o popover mostra o motivo do bloqueio

#### Scenario: Run termina
Test: unit — mesmo predicado após a transição para `Idle`.

- **WHEN** o Chat deixa de estar `Working`
- **THEN** o controle volta a aceitar escolha de Ref

#### Scenario: Árvore suja
Test: none — depende do git real; validação manual por `scripts/dev-demo.sh` com árvore suja.

- **GIVEN** o `cwd` do Chat tem mudanças não commitadas que conflitam
- **WHEN** o usuário escolhe um Ref sem worktree
- **THEN** a mensagem do git aparece na faixa de erro
- **AND** o popover permanece aberto

### Requirement: O modo Workers mantém a linha decorativa

Em `DetailsMode::Workers` o card Workspace SHALL continuar renderizando a linha de Branch
como texto decorativo, sem trigger nem popover, porque não há Chat nem `Pickers` naquele
modo.

#### Scenario: Details sidebar em modo Workers
Test: none — render gpui sem harness; validação visual por `scripts/dev-demo.sh` na rota Workers.

- **WHEN** o Details sidebar renderiza com contexto de worker
- **THEN** a linha de Branch não tem hover, cursor nem handler

### Requirement: Os Refs são carregados no novo host

O host do controle SHALL disparar `ensure_refs(false, cx)` de forma eager e idempotente
sob gate `space.git_detected`, porque o kick atual vive no caminho de draft do footer e
fica inalcançável num Chat vivo.

#### Scenario: Primeira abertura num Chat vivo
Test: unit — `ensure_refs` idempotência/gate (cobertura existente estendida ao novo host).

- **WHEN** o card Workspace renderiza para um Chat num Space com git
- **THEN** os Refs são carregados sem exigir que o usuário abra o popover primeiro
- **AND** um Space sem git não dispara fetch nenhum

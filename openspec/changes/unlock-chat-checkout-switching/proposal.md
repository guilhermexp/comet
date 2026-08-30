# Change: Unlock Chat Checkout switching and move the Ref control into the Workspace card

## Why

O Chat Checkout aparece hoje em dois lugares, e nenhum dos dois serve. No footer do
composer, `Pickers::render_footer` (`crates/ui/src/pickers.rs:2429`) mostra um chip
interativo enquanto é draft e um `footer_label` morto (`pickers.rs:2323`, usado em 2496)
assim que existe Chat selecionado — o estado normal do app. No card Workspace do Details
sidebar, `property_row(GIT_BRANCH, "Branch", …)`
(`crates/ui/src/details_sidebar/view.rs:1561`) é decorativo por construção:
`property_row` (`details_sidebar/widgets.rs:183`) é um `div()` sem `.id()` e sem handler.

Ou seja: o lugar onde o usuário procura o Chat Checkout não tem função, e o lugar que tem
função só funciona antes do Chat existir. A trava é a *wing's rule*, escrita como
comentário em `pick_ref` (`pickers.rs:1265`) e nunca registrada como decisão. Ela cai
neste change — ver `docs/adr/0003-a-live-chat-can-change-its-checkout.md`.

As duas primitivas de engine necessárias já existem e são anteriores a este change:
`SetChatCwd` (`crates/engine/src/rpc.rs:393`, documentada como "mid-session switch to an
EXISTING worktree") e `SwitchRef` (`rpc.rs:1724` → `Repos::switch_ref`,
`crates/engine/src/repos.rs:576`), com reconciliação de label por HEAD-watcher
(`crates/engine/src/workspace_host.rs:1041`). Nenhuma camada abaixo de `ui` precisa mudar.

## Decisions

- **D-01:** Um Chat vivo pode trocar de Ref. A guarda de `pick_ref`
  (`pickers.rs:1265`) sai; as três travas de UI (render lock no early-return de
  `render_footer:2527`, mount lock em `render_target_selectors:3986`, action lock em
  `pick_ref`) deixam de existir juntas — remover uma só produz picker que abre e não age.
- **D-02:** A mecânica é decidida pelo Ref, não pelo usuário, espelhando o que
  `pick_ref` já fazia no draft (`pickers.rs:1268-1272`). Ref com `worktree_path` →
  **Retarget** por `SetChatCwd`, sem git checkout. Ref sem worktree → `SwitchRef` no
  `cwd` do Chat. O HEAD-watcher reconcilia `chat.branch` depois do checkout; a UI não
  escreve o label por conta própria.
- **D-03:** `Retarget` declara o custo antes do clique: a row diz que o próximo run abre
  conversa nova (resume é escopo de cwd). Descobrir isso depois é indistinguível de bug.
- **D-04:** A troca fica bloqueada enquanto o Chat está `Working`
  (`AppState::indicator_for`, `crates/ui/src/state.rs:1282`), com o motivo visível no
  popover. Árvore suja **não** recebe trava nossa: `switch_ref` já falha com a mensagem
  do git (`repos.rs:570-575`) e o popover já tem faixa de erro (`pickers.rs:2985`).
- **D-05:** O controle de Ref sai por completo do footer do composer. `Pickers` ganha um
  único host novo — o card Workspace — e `composer_footer_right` volta a ser `[Model]`.
  Coexistir é inexpressável: `mounted_kind()` é slot único e `popover_frame` usa um id
  fixo (`pickers.rs:2610`), então o primeiro `attach_overlay*` a rodar consome o
  elemento e o outro chip abre sem popover. Consequência aceita: com a Details sidebar
  fechada (`settings.details_sidebar_open`, `crates/ui/src/shell.rs:2201`) não existe
  seletor de Ref — o usuário abre a sidebar.
- **D-06:** O label do controle vem do Chat, nunca de `Pickers::ref_label`.
  `ref_label`/`selected_ref` (`pickers.rs:1711-1773`) leem `config.branch` ou o HEAD do
  repo e não são Chat-aware; `config.branch` só é resetado em troca de **Space**
  (`pickers.rs:605-616`), não de Chat, então trocar de Chat mostraria o Ref do anterior.
  O card já tem o valor certo em `DetailsContext.branch` (`details_sidebar/context.rs:57`).
- **D-07:** Em `DetailsMode::Workers` (`context.rs:103`) a linha continua decorativa: não
  há Chat nem `Pickers` naquele modo.

## What Changes

- Card Workspace: a primeira linha deixa de ser `property_row` decorativo e passa a ser
  um trigger `Stateful<Div>` que abre o popover de Refs, com estado desabilitado e motivo
  enquanto `Working`, e decorativo em modo Workers.
- Footer do composer: o controle de Ref desaparece nos dois estados (chip de draft e
  label de Chat vivo). O chip de checkout kind (`PickerKind::Checkout`) e o cluster de
  model/effort ficam onde estão.
- `pick_ref`: passa a ter caminho de Chat vivo, com `SetChatCwd` para Ref com worktree e
  `SwitchRef` no `cwd` do Chat para o resto.
- Popover: ancora para BAIXO no novo host (hoje `attach_overlay_end` →
  `anchored_menu_above_end`, `crates/ui/src/popover.rs:510`, abre para cima) e cabe na
  largura da sidebar.

## Capabilities

### New Capabilities

- `chat-checkout-control`: onde o Chat Checkout é exibido e alterado, o que cada Ref
  escolhido faz num Chat vivo, e o que bloqueia a troca.

## Impact

- `crates/ui`: `pickers.rs` (travas, `pick_ref`, host do popover, remoção do controle no
  footer, ordem do footer), `details_sidebar/view.rs` + `details_sidebar/widgets.rs`
  (linha interativa), `shell.rs` (injeção de `Entity<Pickers>` no ctor do
  `DetailsSidebar`, `shell.rs:1503`), `popover.rs` (variante below+end, se necessária).
- `crates/engine`: nenhuma mudança esperada — `SetChatCwd` e `SwitchRef` já existem.
  Provar com teste, não assumir.
- Sem harness de render gpui: validação visual por `scripts/dev-demo.sh`
  (`ZERON_OPEN_PICKER=branch` com `ZERON_UI_CAPTURE=1` alcança o popover).
- DOX: contrato novo em `crates/ui/AGENTS.md` (host único do controle de Ref, bloqueio
  por `Working`, fonte do label).
- `CONTEXT.md`: termos **Chat Checkout**, **Ref**, **Retarget** já adicionados.

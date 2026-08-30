# Tasks

## Fasing

| Fase | U-IDs | Seções | Depends on | Audit state | Audited commit | Entrega | UAT mode |
|---|---|---|---|---|---|---|---|
| F1 | C1-C3 | §1 | — | pending | — | Semântica de troca num Chat vivo, sem UI nova | human-driven |
| F2 | C4-C7 | §2 | F1 | pending | — | Controle interativo no card Workspace | human-driven |
| F3 | C8-C9 | §3 | F2 | pending | — | Footer sem controle de Ref | human-driven |

## 1. Semântica: um Chat vivo pode trocar de Chat Checkout

**must_haves:** `pick_ref` age num Chat vivo; Ref com worktree faz Retarget sem git checkout; Ref sem worktree faz checkout no `cwd` do Chat, nunca no `space.path`; troca bloqueada enquanto `Working`; draft inalterado.

- [ ] C1 Remover a guarda de Chat vivo em `pick_ref` (`crates/ui/src/pickers.rs:1265`) e dar-lhe o caminho de Chat vivo: `row.worktree_path.is_some()` → `SetChatCwd(chat.id, worktree_path)`; caso contrário `SwitchRef` com `repoPath = chat.cwd` (fallback `space.path` só quando `chat.cwd` é `None`). Reusar `switching`/`switch_error` (`pickers.rs:1290-1339`) para single-flight e erro do git. O draft path fica byte-a-byte igual. files: `crates/ui/src/pickers.rs`. verify: `cargo test -p zeron-ui pickers`.
- [ ] C2 Predicado de habilitação puro sobre `AppState::indicator_for` (`crates/ui/src/state.rs:1282`) com `now` injetado: `Working` → desabilitado com motivo. Aplicá-lo como guarda de defesa dentro de `pick_ref` além da UI. files: `crates/ui/src/pickers.rs`. verify: `cargo test -p zeron-ui pickers`.
- [ ] C3 Provar que `SetChatCwd` e `SwitchRef` já bastam — nenhuma mudança em `crates/engine`. Se um teste de engine for necessário para provar o retarget, escrevê-lo; não presumir. files: `crates/engine/tests/` (somente se necessário). verify: `cargo test -p zeron-engine` (escopado ao teste tocado).

## 2. Host: o controle interativo no card Workspace

**must_haves:** um único trigger, ancorado para baixo, dentro da largura da sidebar; label vindo do Chat; Refs carregados sem abrir o popover; Retarget declarado na row; modo Workers decorativo.

- [ ] C4 Injetar `Entity<Pickers>` em `DetailsSidebar::new` (`crates/ui/src/details_sidebar/view.rs:318`) a partir do ctor no Shell (`crates/ui/src/shell.rs:1503-1508`), seguindo o precedente de host externo em `shell.rs:6633-6634`. files: `crates/ui/src/details_sidebar/view.rs`, `crates/ui/src/shell.rs`. verify: `cargo test -p zeron-ui`.
- [ ] C5 Trigger interativo substituindo o `property_row` de Branch (`view.rs:1561`): variante `Stateful<Div>` ao lado de `property_row` em `details_sidebar/widgets.rs:183` (id, cursor, hover, handler; `role` + `aria_label` conforme `crates/ui/AGENTS.md`), mantendo a geometria de 108px da coluna de label e o `Path` decorativo. Em `DetailsMode::Workers`, e sem Chat nem draft aplicável, a linha permanece `property_row`. files: `crates/ui/src/details_sidebar/widgets.rs`, `crates/ui/src/details_sidebar/view.rs`. verify: `cargo test -p zeron-ui` + `scripts/dev-demo.sh`.
- [ ] C6 Novo método de render em `Pickers` (assinatura só-`cx`, como `render_target_selectors:2352`) que produz trigger + popover de Refs ancorados para BAIXO: variante `*_below_end` em `crates/ui/src/popover.rs` (hoje só existe `anchored_menu_above_end`, `popover.rs:510`) ou reuso de `anchored_menu_below`. Label do trigger vindo de `DetailsContext.branch`/draft, nunca de `ref_label`. Conferir a largura de 320px (`pickers.rs:2541`) contra a largura da Details sidebar e ajustar. files: `crates/ui/src/pickers.rs`, `crates/ui/src/popover.rs`. verify: `cargo test -p zeron-ui pickers` + `ZERON_UI_CAPTURE=1 ZERON_OPEN_PICKER=branch scripts/dev-demo.sh`.
- [ ] C7 `ensure_refs(false, cx)` eager no novo host sob gate `space.git_detected`, e rows de worktree declarando na própria row que o próximo run abre conversa nova do harness. files: `crates/ui/src/pickers.rs`. verify: `cargo test -p zeron-ui pickers` + `scripts/dev-demo.sh`.

## 3. Remoção: o footer perde o controle de Ref

**must_haves:** nenhum elemento de Ref no footer em nenhum estado; um único mount de `PickerKind::Branch`; checkout kind e model/effort intactos.

- [ ] C8 Remover do footer o chip de Ref do draft (`pickers.rs:2583-2602`), o `footer_label` de Ref do Chat vivo (`pickers.rs:2496-2506`) e o braço `PickerKind::Branch` da construção do overlay (`pickers.rs:2537-2553`). `composer_footer_right_order()` (`pickers.rs:469`) passa a `[Model]` e a assertion em `pickers.rs:4118` acompanha. Preservar o chip de `PickerKind::Checkout` e o badge de change request (`pickers.rs:2515-2522`). Se `footer_label` ficar sem uso, remover a função. files: `crates/ui/src/pickers.rs`. verify: `cargo test -p zeron-ui pickers`.
- [ ] C9 DOX pass em `crates/ui/AGENTS.md`: contrato de host único do controle de Ref, fonte do label, bloqueio por `Working`, e a linha da Test Coverage Matrix para o novo predicado/decisão. files: `crates/ui/AGENTS.md`. verify: leitura — o contrato descreve o código que existe.

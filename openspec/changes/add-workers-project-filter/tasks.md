# Tasks

## 1. State

- [x] 1.1 Add the filter to `WorkersModel` with a setter, and drop it in
      `apply_snapshot` when its project is no longer in the snapshot.
- [x] 1.2 Persist it: a field in `UiSettings`, the line in
      `apply_shell_settings`, restore on boot and save from the Shell's
      existing workers observation.

## 2. Surface

- [x] 2.1 Render the trigger above the Workers scroll region: folder icon,
      the filter's name or "All projects", chevron.
- [x] 2.2 Render the dropdown card: search input, "All projects" (empty query
      only), matching root projects, "New project…".
- [x] 2.3 Keyboard: ↑/↓ move, ⏎ activates, Esc closes — through
      `popover::{menu_step, classify_key}`.
- [x] 2.4 Apply the filter to the rendered rows, keeping the chosen project's
      subtree.
- [x] 2.5 Fechar o card agenda `popover::reap_popup`. Sem isso o `Popup` fica
      preso na fase de saída: card invisível, overlay `.occlude()` vivo, sidebar
      inteira sem aceitar clique (reportado na tela).
- [x] 2.6 Abrir o card foca o input de busca antes do primeiro paint
      (`window.focus`), a mesma ordem do dropdown de Spaces — senão a busca não
      recebe tecla e ↑↓/⏎ nunca chegam ao handler do card.
- [ ] 2.7 Keep the filtered project's row through the working-set rule: the
      filter is an explicit request for that project, the same standing the
      selection and the launcher already have. Without it, filtering to an
      empty or fully archived root leaves the tree exactly as it was and the
      capability is not delivered.

## 3. Verification

- [x] 3.1 Unit: the filter keeps a project's subtree and drops the rest.
- [x] 3.2 Unit: a filter naming a missing project reads as "All projects".
- [x] 3.3 Unit: the dropdown rows are the root projects, ranked by the query,
      with "All projects" only on an empty query.
- [x] 3.4 `cargo test -p zeron-ui` (1159 passed, 1 failed: the pre-existing
      `weekly_tone_neutral_when_no_usage_or_no_weekly_window` from `804d83fc`)
      and `cargo fmt --all --check` clean.
- [ ] 3.5 Visual check on `scripts/dev-demo.sh` (no render harness for gpui):
      filtering to a project with no sessions narrows the tree to it.
- [ ] 3.6 Unit: filtering to a project with no live session draws that
      project's row.

## 4. Closeout

- [x] 4.1 Record the filter in `crates/ui/AGENTS.md`.
- [ ] 4.2 Archive once 3.5 and 3.6 are confirmed.

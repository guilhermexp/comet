# Design

## Constraints descobertas antes de escrever código

Cada item abaixo é fato verificado no repo, com o file:line que o prova. Nenhum é
opinião, e cada um já quebrou (ou quebraria) uma implementação ingênua deste change.

- **C1 — o label não pode vir de `Pickers`.** `ref_label` (`pickers.rs:1767`) e
  `selected_ref` (`1711`) resolvem `config.branch` ou o HEAD do repo, sem olhar
  `selected_chat_row()`. `selected_ref_index` (`1695`) *é* Chat-aware, então hoje o
  highlight do popover e o label do trigger já discordariam num Chat vivo. Fonte do
  label: `DetailsContext.branch` (`details_sidebar/context.rs:57`), que copia
  `chat.branch`.
- **C2 — `switch_draft_ref` aponta para a pasta errada.** `pickers.rs:1310` manda
  `repoPath = space.path`. Um Chat em worktree tem `chat.cwd != space.path` — é assim que
  o footer detecta worktree hoje (`pickers.rs:2477`). Um checkout no `space.path` mexeria
  no repo errado.
- **C3 — o popover abre para cima.** `attach_overlay_end` (`pickers.rs:3863`) usa
  `anchored_menu_above_end` → `Anchor::BottomRight` (`popover.rs:510`). No topo da
  janela o card de 320×≤640 cobriria o próprio widget. O novo host precisa de
  ancoragem para baixo.
- **C4 — slot único de overlay.** `render_footer` constrói no máximo um
  `Option<(PickerKind, AnyElement)>` (`pickers.rs:2537`) e cada `attach_overlay*` faz
  `take()`. Com dois hosts vivos, o segundo fica sem popover. Daí D-05: mover, não
  duplicar.
- **C5 — `ensure_refs` eager está no caminho de draft.** `pickers.rs:2534`, inalcançável
  depois do early-return de Chat vivo em 2527. Sem um kick no novo host, `refs` fica
  `Idle` até a primeira abertura. Gate obrigatório: `space.git_detected` (`1197`).
- **C6 — `config.branch` sobrevive à troca de Chat.** O observer reseta
  harness/model/reasoning/options por Chat (`pickers.rs:592-600`) mas `branch`/`checkout`
  só por Space (`605-616`).
- **C7 — a ordem do footer é testada.** `composer_footer_right_order()`
  (`pickers.rs:469`) e a assertion em `pickers.rs:4118` codificam `[Model, Branch]`.
- **C8 — modo Workers não tem Chat nem Pickers.** `context_for_worker`
  (`context.rs:79-105`) devolve `chat_id: None`.
- **C9 — `DetailsSidebar` não alcança `Pickers` hoje.** O struct carrega só
  `app_state` e `workers_model` (`details_sidebar/view.rs:290-291`); `Pickers` é do
  `Composer` (`composer.rs:3924`, accessor `4018`). Precedente de host externo já no
  Shell: `shell.rs:6633-6634` renderiza `render_target_selectors` fora do composer.
- **C10 — largura.** `popover_frame` fixa `w(320.0)` para Branch (`pickers.rs:2541`)
  contra a largura da Details sidebar.
- **C11 — `property_row` é `Div`, não `Stateful<Div>`.** `widgets.rs:183`, sem `.id()`.
  Trigger precisa de id, cursor, hover e handler, e (por `crates/ui/AGENTS.md`) role +
  `aria_label` se virar raiz de foco.

## Onde o controle mora

O host é `DetailsSidebar::render_details` (`view.rs:1547`), primeira linha do
`widget_card("workspace-widget", …)`. Duas rotas de acesso ao `Pickers`, ambas já com
precedente no código:

**Rota escolhida — injeção no ctor.** `DetailsSidebar::new` (`view.rs:318`) recebe
`Entity<Pickers>`; o Shell tem o handle na mão no ponto de construção
(`shell.rs:1503-1508`), porque constrói o `Composer`. O render faz
`pickers.update(cx, |p, cx| p.render_checkout_ref_control(cx))`, o mesmo shape que
`shell.rs:6633` já usa. `render_target_selectors` (`pickers.rs:2352`) prova que uma
assinatura só-`cx` basta; `DetailsSidebar::render` também tem um `Window` disponível
(`view.rs:2208`, hoje `_window`) se fizer falta.

**Rota rejeitada — evento para o Shell.** Adicionar variante a `DetailsSidebarEvent`
(`view.rs:212`) e deixar o Shell dirigir o `Pickers`. Rejeitada porque o popover precisa
ser *filho* do trigger para ancorar; via evento o elemento nasceria no lugar errado.

## O que `pick_ref` passa a fazer

```
pick_ref(row):
  chat = selected_chat_row()
  if chat is None:            → comportamento atual (draft), inalterado
  if working(chat):           → no-op (a UI já bloqueou; guarda de defesa)
  if row.worktree_path:       → SetChatCwd(chat.id, row.worktree_path)   # Retarget
  else:                       → SwitchRef(cwd = chat.cwd, ref = row.name)
                                 # HEAD-watcher reconcilia chat.branch
```

`switching` e `switch_error` (já existentes, `pickers.rs:1290-1339`, faixa em `2985`)
cobrem os dois caminhos: single-flight e erro do git na tela sem fechar o popover.

## Não-objetivos

- Trocar de Space num Chat vivo. O Space segue fixado na criação.
- Criar branch ou worktree novo pelo popover. `render_branch_popover` não tem essa row
  hoje e não ganha uma aqui; `PickerKind::Checkout` continua sendo o dono do checkout
  kind, no footer.
- Interromper run para permitir troca. D-04 bloqueia; abortar run é outro change.
- Tornar interativa qualquer outra linha do card Workspace (`Path` fica decorativo).

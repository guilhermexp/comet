# AGENTS.md — zeron-workers-unpeel (`crates/workers-unpeel`)

## Purpose

Typed Comet adapter ("frontier") over `unpeel-core` from the pinned
`third_party/unpeel` (vendorizado, ver Local Contracts) — o backend da
superficie de Workers. Exposes
`LocalWorkersClient` / `WorkersRuntime` and typed models for bootstrap,
projects, worktrees, groups, presets, sessions (launch/actions/viewport/
output), settings snapshots, artifacts, lifecycle, parent notifications, and
the Comet-owned controller MCP. Owns the dispatch of the zeron binary's
internal host modes (`__session_host__` et al.).

## Ownership

| Path | Owns |
|---|---|
| `lib.rs` | All typed `Workers*` models, `LocalWorkersClient`, `WorkersRuntime`, session-host mode detection/dispatch (`is_session_host_mode`, `session_host_launch_args`, `session_host_launcher_path`, `run_session_host_mode_if_requested`) |
| `controller_mcp.rs` | Comet-owned MCP surface for the primary Orchestrator (`CONTROLLER_MCP_ARG`) — intentionally separate from Unpeel's worker-to-worker MCP host |
| `activity_bridge.rs` | Frontend bridge for Unpeel's hook-owned session lifecycle — `#[path]`-includes the state machine directly from `third_party/unpeel/crates/unpeel-tui/src/activity.rs` so Start/Stop/PermissionRequest, durable seeds, runtime generations and output fallbacks cannot drift from the pinned TUI frontend |
| `session_event_journal.rs` | Session output/event journaling |
| `parent_notifications.rs` | Worker→parent task notifications (register/begin/confirm/ack/cancel, completion evidence) |
| `workspace_trust.rs` | Workspace trust decisions |
| `hook_migration.rs` | Legacy hook root migration — installs Comet-managed hooks under `app_hooks_root()`, then prunes the migrated assets out of `<unpeel_home>/hooks` while retaining the entries the pinned upstream still resolves there (`UPSTREAM_OWNED_LEGACY_ASSETS`) |
| `resources.rs` + `resources/{macos,unsupported}.rs` | Host resource sampling (CPU/memory pressure); macOS implementation + unsupported-platform fallback |
| `tests/` | Integration tests per surface |

Depends on: `unpeel-core` (vendorizado em `third_party/unpeel`) only.
Consumed by: zeron-ui (`workers/`), apps/zeron (host-mode dispatch at startup).

## Local Contracts

- **`third_party/unpeel` e codigo vendorizado, nao submodulo.** O upstream
  `unpeel-com/unpeel` deixou de existir publicamente; enquanto foi submodulo,
  NENHUM clone limpo compilava (`unpeel-core` e dependencia path de
  `zeron-workers-unpeel`, que e dependencia de `zeron-ui`, entao o workspace
  inteiro falhava na resolucao) — foi o que fez o gate de push revisar cinco
  rodadas sem executar um teste. Agora sao arquivos comuns rastreados: clone,
  worktree e CI compilam sem nenhum passo de setup. Nao reintroduza o
  submodulo e nao tente `git submodule update`. Mudanca de forma no upstream
  agora se edita AQUI — nao ha mais para onde mandar upstream.
  Proveniência e tree id do snapshot vivem em
  `third_party/unpeel-upstream.toml`; alteração no vendorizado atualiza essa
  metadata no mesmo commit.
- **Session hosts are re-executed zeron binaries.** A Workers session runs as a
  `__session_host__` process (`unpeel_core::session_host::SESSION_HOST_ARG`)
  spawned from the current executable; `run_session_host_mode_if_requested()`
  at startup dispatches into the host instead of the app. **Never kill
  `__session_host__` processes when rebuilding the app** — kill only the exact
  main PID (root AGENTS.md). Other internal host modes dispatched here:
  `CONTROLLER_MCP_ARG`, `MCP_HOST_ARG`, `MCP_GATE_ARG`, the browser/computer
  cleanup args, `COMPACT_OUTPUT_JOURNALS_ARG`, and legacy MCP gate kinds
  (`unpeel_core::integrations::legacy_mcp_gate_kind`). Browser is a *domain* of
  `MCP_HOST_ARG`, never its own server.
- **Controller MCP is Comet-owned.** Only ACP controller sessions receive this
  process in their `mcpServers` list (injected by zeron-harness's
  `workers_mcp_servers*`); it is NOT Unpeel's worker-to-worker MCP host.
- **Activity state machine is shared by include.** `activity_bridge.rs`
  includes o fonte vendorizado via `#[path]` — a disciplina de edicao continua:
  nao forke a maquina de estados numa copia local; mude no proprio
  `third_party/unpeel` para que as duas pontas nao divirjam.
- **O ledger sobrevive ao `remove_project`; o working set nao.** `projects[]`
  em `app-state.json` e o working set: `remove_project` poda o registro E
  todas as sessoes debaixo dele. O ledger mora na chave irma `comet_projects`,
  no mesmo arquivo (mesmo flock, mesma recusa de dropar chave nao modelada), e
  `remove_project_record` enumera as tres chaves que limpa — esta nao esta
  entre elas. Se o ledger algum dia virar filho de `projects`, o teste
  `ledger_survives_the_pruning_that_removes_a_project` fica vermelho, e e para
  ficar. A chave e o PATH, nunca o id: `add_project` cunha um `comet-<uuid>`
  novo a cada entrada. Grupos organizacionais NAO entram: reutilizam o path do
  pai e violariam essa chave; worktrees entram porque têm path próprio. A
  projeção filtra `is_group` e `reconcile` deduplica path defensivamente.
- **`reconcile` e puro e `last_seen_at` so anda com atividade real.** Nunca
  carimbe `now` num projeto vivo e parado: `dirty` viraria true a cada passada
  e abrir a tela escreveria num arquivo compartilhado e travado a cada render.
- **`git --until` ignora data malformada EM SILENCIO.** Medido: `--until=@86400`
  e `--until=86400 +0000` sao aceitos e devolvem HEAD como se nao houvesse
  filtro. So `@<segundos> <offset>` e ISO 8601 filtram. Qualquer mexida em
  `project_git::commit_at` mantem
  `a_date_before_the_first_commit_has_no_anchor`, que e a rede desse silencio.
- **Nada de estado de git no ledger.** `is_repo`, remote, branch e commits
  ancora sao lidos frescos por projeto SELECIONADO. `WorkersProject::git_branch`
  nao serve de fonte: o campo e desserializado de `gitBranch`, mas o
  `controller_host.rs` que o comet usa nunca o emite (so o host TUI emite), entao
  pela rota `comet-local` ele e sempre `None`.
- **`owner`/`repo` nao sao derivados aqui.** Isso e
  `zeron_engine::parse_git_remote`, e puxar `engine` (loro, tokio, rusqlite,
  reqwest) para dentro desta crate por um parser inflaria ate o `cargo test`
  daqui. `project_git` devolve `remote_url`; quem consome — a UI, que ja
  depende das duas — faz a derivacao.
- **A costura entre criar worktree e rodar setup e testada, nao presumida.**
  `worktree_config` prova que os comandos rodam; `worktree_setup_wiring_tests`
  prova que `create_worktree` os CHAMA. Sem o segundo, apagar a chamada deixa a
  suite verde — e um arquivo de setup que ninguem le e exatamente o bug que a
  feature existe para consertar. `create_worktree_at` existe so para dar essa
  costura um caminho de estado injetavel; nao use a variante `_at` em producao.
- **Setup e uma arvore de processo com resultado parcial explicito.** Stderr e
  drenado concorrentemente com cauda de 64 KiB; timeout encerra o process group
  inteiro. Falha mantém o worktree registrado, carrega comando + motivo em
  `WorkersWorktreeResult` e barra `create_worktree_and_launch` antes de iniciar
  a Session.
- **Hook ingress não morre por sinal de filho.** O accept loop trata
  `WouldBlock` e `Interrupted` como transitórios; setup/Worker encerrando
  processos no mesmo host não pode fechar o endpoint e devolver BrokenPipe ao
  próximo hook.
- **Teste que cria worktree limpa o que criou.** `unpeel_core::worktrees::create`
  escreve em `~/.unpeel/worktrees/`, que o state path injetado NAO redireciona.
  O caminho e `<worktrees>/repo-<hash>/<branch>`: apagar so o ramo deixa o
  diretorio do repo vazio para tras e a contagem cresce a cada rodada (cresceu,
  16 vezes, antes do `Drop` do fixture cobrir o pai).
- **Typed frontier only.** The UI and engine consume the `Workers*` types from
  this crate; do not leak raw `unpeel_core` types into zeron-ui — map them
  here.
- **One lifecycle fact carries one event id.** `parent_notifications.rs` derives
  a notification id per event; acknowledging in production compacts the journal,
  which CLEARS `acknowledged_notification_ids` (journal sequences stop meaning
  anything). So a fact spelled two ways can never be acknowledged: the spellings
  alternate forever, one parent command per pass. That is what a dead worker did
  on 2026-08-25 — the journal-less fallback (`{gen}:exited`) and the synthetic
  exit push (`{gen}:{episode}:exited`) both fired, minting ~2 800 notifications
  and a 13 MB parent doc. Never emit a second spelling of an event the pass
  already carries.
- **The tool declaration is product surface, not decoration.** `workers` is
  action-dispatch: with no per-field `description` naming its owning action, a
  caller cannot build a valid `launch_worker` on the first try and pays a
  `help` round-trip to delegate, while editing a file locally costs nothing —
  the asymmetry that makes an orchestrator inspect and never delegate. The
  tool `description` also states when this is *not* the right substance
  (`task` stays inside the caller's session, read-only).
  `tests/controller_mcp.rs` locks both: every action in the enum appears in
  some description, and no field is left without one.
- **An unlisted checkout is an unlaunchable one.** `launch_worker` takes a
  `project_id` and `validate_launch_target` rejects any id absent from the live
  project list, so the surface must also be able to *add* a project
  (`add_project`, idempotent over the canonical path). Without it the caller's
  only working move for an unregistered repo is an ancestor project, and the
  worker runs every command against the wrong tree — observed 2026-08-26, two
  workers briefed on a client repo launched in `$HOME`.
- **A brief is delivered against a stable screen, and booting is not silence.**
  `submit_initial_briefing` measures stability over `briefing_stability_key`,
  never over the raw viewport: the boot counter and the `esc to interrupt`
  status line repaint on their own clock, so raw frames restarted the 300 ms
  window on every tick and the whole budget expired without a single ready
  check — observed 2026-08-27, codex with six MCP servers, brief never
  delivered. `is_booting_screen` blocks readiness because Codex paints its
  composer glyph *while* servers boot, and every boot frame pushes the deadline
  forward: `BRIEFING_READY_WAIT` (45 s) is an idle budget, not a total, and
  `BRIEFING_BOOT_CEILING` (180 s) is the absolute cap.
- **The selection glyph is stripped before a numbered menu is matched.** Codex
  prints `› 1. Update now`, and its `Press enter to continue` footer names
  neither a nav hint nor a cancel key, so `viewport_has_menu_prompt` does NOT
  fire on it. Anchoring `1.` at the start of the line is therefore the only
  thing standing between that menu and the `screen.contains('›')` prompt check
  reading it as a ready composer.
- **A launch that could not deliver its brief still returns Ok.** Returning
  `Err` would orphan a live session whose id the caller never learns, so the
  payload carries `briefing_submitted: false`, `briefing_error` and a
  `next_action` naming the remediation. The caller owns the worker from
  `launch_session` onward: a caller that reads only `launched` leaves it idle
  and its next `send_text` lands in whatever the runtime happens to show — that
  is how a brief ended up typed at a login shell on 2026-08-27. Do not "fix"
  this by killing the session behind the caller's back.
- **A Comet-managed hook never resolves its interpreter through `PATH`.** Codex
  runs hooks with an environment of its own choosing and reports a hook that
  cannot exec as `hook exited with code 127`, naming only the event — no
  command, no trace line, nothing to attribute it to. Measured 2026-08-27: with
  the payload on argv and a `PATH` missing `/bin`, `exec bash` in the codex
  notify normalizer produced exactly that, three times per worker. Both codex
  assets (`notify-normalizer.sh` and the `notify=[…]` config in
  `command-wrapper.sh`) now name `${BASH:-/bin/bash}`, and the normalizer leaves
  a trace breadcrumb instead of failing the turn when no interpreter exists.
  Codex hooks deliver their payload on **stdin** (`argc=0`, measured), so any
  new hook asset that reads `$1` must keep the `cat` fallback.

## Work Guidance

- New Workers capability: extend `LocalWorkersClient` + typed models here, then
  consume from `zeron-ui/src/workers/`.
- Changes that touch session lifecycle must preserve the durable-seed /
  runtime-generation semantics of the included activity state machine.
- Platform-specific resource code goes in `resources/macos.rs` with the
  fallback contract in `resources/unsupported.rs`.

## Verification

Run all: `cargo test -p zeron-workers-unpeel` (part of the publish gate).
Roda em qualquer checkout desde que `third_party/unpeel` foi vendorizado.

### Test Coverage Matrix

| Camada / path | Tier exigido | Como rodar |
|---|---|---|
| `src/lib.rs` (17), `src/activity_bridge.rs` (10), `src/resources.rs` (8), `src/session_event_journal.rs` (7), `src/project_ledger.rs` (11), `src/project_git.rs` (11), `src/worktree_config.rs` (15), `worktree_setup_wiring_tests` (4) | unit | `cargo test -p zeron-workers-unpeel --lib` |
| `tests/controller_mcp.rs` (24) — Comet-owned MCP surface | integration | `cargo test -p zeron-workers-unpeel --test controller_mcp` |
| `tests/parent_notifications.rs` (15) | integration | `--test parent_notifications` |
| `tests/workspace_trust.rs` (10) | integration | `--test workspace_trust` |
| `tests/settings.rs` (8) — settings snapshot/persistence | integration | `--test settings` |
| `tests/project_actions.rs` (5), `tests/local_actions.rs` (4), `tests/session_actions.rs` (3), `tests/local_bootstrap.rs` (2) — client actions over a local runtime | integration | `cargo test -p zeron-workers-unpeel --test <name>` |
| `tests/hook_migration.rs` (5) | integration | `--test hook_migration` |

## Child DOX Index

None — flat domain.

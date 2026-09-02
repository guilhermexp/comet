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
| `lib.rs` | All typed `Workers*` models, including optional device-local model/token telemetry on `WorkersSession`, `LocalWorkersClient`, `WorkersRuntime`, `runtime_catalog_snapshot` (o catálogo pinado, público para a UI provar seus espelhos de ícone/tint contra a fonte em vez de copiá-la à mão), session-host mode detection/dispatch (`is_session_host_mode`, `session_host_launch_args`, `session_host_launcher_path`, `run_session_host_mode_if_requested`) |
| `controller_mcp.rs` | Comet-owned MCP surface for the primary Orchestrator (`CONTROLLER_MCP_ARG`) — intentionally separate from Unpeel's worker-to-worker MCP host |
| `activity_bridge.rs` | Frontend bridge for Unpeel's hook-owned session lifecycle — `#[path]`-includes the state machine directly from `third_party/unpeel/crates/unpeel-tui/src/activity.rs` so Start/Stop/PermissionRequest, durable seeds, runtime generations and output fallbacks cannot drift from the pinned TUI frontend; persists provider Session metadata and refreshes provider telemetry fail-soft without replacing the URL Worker identity; persistence failure invalidates prior telemetry instead of refreshing stale identity; startup migrates the short-lived unbound telemetry marker in the background by recomputing current provider evidence |
| `session_event_journal.rs` | Session output/event journaling |
| `parent_notifications.rs` | Worker→parent task notifications (register/begin/confirm/ack/cancel, completion evidence) |
| `workspace_trust.rs` | Workspace trust decisions |
| `hook_migration.rs` | Legacy hook root migration — installs Comet-managed hooks under `app_hooks_root()` (every runtime attempted, failures accumulated instead of aborting the loop), then prunes the migrated assets out of `<unpeel_home>/hooks` while retaining the entries the pinned upstream still resolves there (`UPSTREAM_OWNED_LEGACY_ASSETS`) |
| `resources.rs` + `resources/{macos,unsupported}.rs` | Host resource sampling (CPU/memory pressure); macOS implementation + unsupported-platform fallback |
| `tests/` | Integration tests per surface |

Depends on: `unpeel-core` (vendorizado em `third_party/unpeel`) only.
Consumed by: zeron-ui (`workers/`), apps/zeron (host-mode dispatch at startup).

## Local Contracts

- **Request id é sequência única do processo.** O `REPLAY_CACHE` do host é global e chaveado por `(principal, request_id)`, e todo `LocalWorkersClient` fala pelo mesmo principal (`comet-local`). Um contador por instância fazia cada `new()` recomeçar em 1 — e a UI criava cinco (terminal, model, resource monitor, workspace, settings/projects). Hoje a UI passa por `crate::workers::client::shared()` e tem **uma** instância, mas o contador compartilhado permanece como segunda defesa: qualquer consumidor novo (controller MCP, teste, host) volta a criar clientes próprios. Colidir com payload diferente devolve `409: request id reused with different request`; colidir com payload **igual** é pior, porque o segundo cliente recebe a resposta do primeiro sem erro nenhum. `next_request_id` é `shared_next_request_id()`, no mesmo padrão `OnceLock` dos outros campos compartilhados.

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
  process in their `mcpServers` list (resolved by zeron-harness's
  `workers_mcp.rs` and rendered into each runtime's dialect); it is NOT
  Unpeel's worker-to-worker MCP host.
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
- **A família pi retoma por id, e `--continue` só sob diretório pinado.** `omp`
  e `prime-agent` compartilham a receita de resume em
  `third_party/unpeel/runtimes/_shared/pi-family/adapter/resume.rs`: aceitam
  `-c/--continue`, `-r/--resume <id>`, `--session-dir` e `--no-session`, e **não**
  têm `--session` nem `--fork` — copiar a receita do `pi` gera comando inválido.
  Todo Worker novo da família nasce com `--session-dir
  <unpeel_home>/pi-sessions/<session_id>` pinado, e é isso que torna
  `--continue` exato: sem diretório pinado ele pegaria a conversa mais recente
  do worktree compartilhado, que na máquina de desenvolvimento é de outro
  Worker. Por isso sessão legada sem marker e sem diretório reinicia limpa em
  vez de continuar. As capabilities `resume`/`restart_agent` vivem no
  `runtime.toml` de cada runtime, e `runtime_catalog_resume_capabilities_match_adapter_callbacks`
  (em `unpeel-core`) fica vermelho se a declaração e o adapter divergirem.
- **A política de hibernação é pura, e o clock dela não é `updated_at_unix_ms`.**
  `hibernation_candidates` decide sobre o snapshot que o painel já busca, sem
  I/O e sem UI: recebe sessões, settings de Resources, a sessão selecionada e
  o relógio, e devolve quem parar, do mais ocioso para o mais novo. O clock é
  `WorkersSession::idle_since_unix_ms`, preenchido pelo `activity_bridge` com
  o máximo entre `screen_changed_at` do manifest e o sinal de atividade
  command-aware do Unpeel. `updated_at_unix_ms` não serve: ele anda com o
  heartbeat de 60 s do host e com o mtime de `output.bin`, e num `omp` parado
  há 24 h os dois tinham idade 0 h enquanto `screen_changed_at` marcava 24,2 h.
  Sem evidência (`None`) o Worker é protegido, nunca hibernado. A elegibilidade
  exige `unpeel_core::resume::can_resume` do comando — é o mesmo teste que
  exclui sessões de terminal, cujo shell não tem receita de resume, e é o que
  impede jogar fora uma conversa irrecuperável. Hibernar `omp`/`prime-agent` só
  ficou seguro depois da receita de resume da família pi, acima.
- **Hibernar exige evidência positiva; ausência de sinal protege.** `idle` no
  wire não basta e `can_resume` não basta. `hibernation_candidates` exige
  também `idle_confirmed_by_hook` e `resumable_conversation`, os dois
  preenchidos pelo `activity_bridge`:
  - `idle_confirmed_by_hook` vem de `ActivityEngine::hook_confirmed_idle`
    (`stopped_at` presente), e é lido DEPOIS de `derive_activity`, que é quem
    roda o sweep do tick. A confirmação é perecível: só sobrevive enquanto
    NADA acontece. Crescimento do sinal de atividade depois da graça de
    re-arme (`STOP_REARM_GRACE`, 5 s) limpa `stopped_at` para qualquer
    runtime, porque codex, cursor-agent, gemini, amp e opencode só postam
    `Stop` e não têm evento de início de turno para revogar a confirmação
    quando o orquestrador manda texto; o estado visível pode seguir `Idle`,
    mas sem confirmação. Sinal parado mantém a confirmação através de
    quantos sweeps forem, e um `Stop` posterior reconfirma. Sem isso o `HookState::Idle` do sweep
    (`HOOK_IDLE_TIMEOUT`, 5 min sem mudança de tela) entrava na política com
    relógio datado do `agent_start` — prazo estourado por construção — e o
    `Archive` matava turno em andamento de `omp` dentro de subprocesso longo
    e silencioso. Sweep e Stop são o MESMO estado em `hook_owned_state`: só
    `hook_confirmed_idle` os separa.
  - `resumable_conversation` é evidência de identidade DESTE Worker,
    verificada diretamente e nunca por comparação de strings (as receitas são
    idempotentes sobre o próprio output, então `resumed != comando` dava
    falso para todo Worker já retomado uma vez): id de conversa de provider
    capturado no marker, OU diretório de sessão gerenciado
    (`resume::managed_storage_path` sob `unpeel_home`) fixado no comando, OU
    id explícito de conversa já no comando
    (`resume::embedded_conversation_id`, callback por adapter). Receita que
    só sabe retomar "a mais recente do diretório" sem nenhuma das três
    (`codex resume --last`, `gemini --resume latest`, `--continue` solto) não
    qualifica: dois Workers no mesmo cwd retomariam a conversa um do outro,
    o que é pior do que não hibernar. Para a família pi essa é a forma das
    sessões legadas há muito ociosas — hiberná-las reiniciaria limpo e
    sumiria com a conversa.
  - `ClearAttention` do menu não é fim de turno: o bridge chama
    `ActivityEngine::clear_attention_unconfirmed`, que leva a `Idle` sem
    gravar `stopped_at`. Só `Stop`/`StopFailure` de hook real confirmam.
  Os dois campos são device-local: `From<SessionWire>` nasce com `false` e
  quem não passa pelo bridge (sessão não-`running`) nunca é candidato.
- **A decisão de hibernar é tomada duas vezes.** `confirmed_hibernation_candidates`
  reavalia a política sobre um bootstrap fresco e arquiva só a interseção. O
  `Archive` para uma sessão viva sem olhar atividade, e entre decidir e
  executar cabe um `send_text`: o snapshot do painel pode ter sido decodificado
  antes do host estampar `screen_changed_at` e antes do hook `agent_start`
  chegar. A segunda passada nunca amplia a primeira.
- **`send_text` num Worker morto é entrada perdida, não erro do host.**
  `live_worker_guard` barra `send_text`/`send_keys` em qualquer sessão que não
  esteja `running` e nomeia `restart_worker` na mensagem, porque a hibernação
  para Workers ociosos por conta própria e o Orquestrador não tem como
  adivinhar. `restart_worker` usa `RestoreAndResume` e devolve o id da sessão
  **resultante**: relançar substitui a Session por uma nova (novo uuid), e o
  endpoint de session-action responde só `{"ok": true}`, então o id novo é
  descoberto comparando a listagem antes e depois. Duas sessões novas ao mesmo
  tempo (outro launch concorrente) devolvem o id antigo em vez de um palpite.
- **Typed frontier only.** The UI and engine consume the `Workers*` types from
  this crate; do not leak raw `unpeel_core` types into zeron-ui — map them
  here.
- **Model/token telemetry is optional and device-local.** Host wire fields
  `totalTokens` and `modelUsage` decode to `Option<u64>` and a default-empty
  typed list. Missing fields are compatibility, not zero usage; this frontier
  does not sync them through Chat/edge state or reinterpret them as Managed
  Provider Usage.
- **Legacy unbound telemetry is a migration trigger, never evidence.** The
  bridge startup recognizes only the old valid `SessionTelemetry` shape,
  recomputes it from the current provider id/canonical path, and writes the
  bound marker atomically. Current bound markers are not rescanned.
- **O andaime do prompt de notificação não pode ser indentado.** Markdown conta
  espaço: cerca de código aceita no máximo 3 de indentação, e com 4+ ela deixa de
  ser cerca — as crases viram texto literal, a cauda vaza como prosa e a
  instrução final aparece dentro de uma caixa de código. O format string
  carregava 9 espaços herdados da indentação do fonte. Teste checa indentação
  linha a linha, porque `contains("```\n…")` casa a cerca sem enxergar o que vem
  antes dela — foi assim que o teste antigo passou com o markdown quebrado.
- **`\r` no output tail é retorno de carro, não "mais um controle".** Um TUI
  repinta a status line dezenas de vezes e o journal guarda cada repaint;
  `clean_output` tira o ANSI mas mantém o `\r`, então mapeá-lo para espaço junto
  com os outros concatenava todas as versões numa linha só. Fica o último paint,
  que é o que um terminal mostraria. Simplificação conhecida: o último segmento
  vence inteiro, e um repaint mais curto que o anterior deixaria cauda visível
  num terminal de verdade — status line redesenha do mesmo tamanho, então não
  paga um emulador aqui.
- **O prompt de notificação é markdown, e a quebra de linha do output tail é
  conteúdo.** `build_worker_parent_notification_prompt` monta título + bullets +
  bloco de código cercado; `safe_prompt_field` continua achatando os campos de
  uma linha (e troca crase por apóstrofo, pois eles entram em `code` inline),
  mas o tail passa por `safe_output_block`, que **preserva `\n`**. Achatar o
  tail junto com os campos era o que entregava uma parede de texto de milhares
  de caracteres numa linha só — ilegível no overlay do chat e sem estrutura
  para o agente. A cerca vem de `code_fence_for`: crases dentro do tail
  fechariam o bloco cedo e derramariam o resto como markdown. Todo o resto dos
  caracteres de controle (e ANSI, via `clean_output`) continua saindo.
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
- **O orquestrador é dono da duração de `wait_for_status`; o teto (`WAIT_FOR_STATUS_MAX_TIMEOUT_SECONDS` = 4h) é só sanidade de transporte.** Schema (`maximum`), help (`limits.wait_seconds`) e `.clamp` derivam da mesma constante. Default continua 30s. Expiração devolve `timed_out: true` + snapshot + `next` (`WAIT_TIMED_OUT_NEXT`): esperar de novo com timeout do tamanho do trabalho, ou encerrar o turno e receber `[worker-task-notification]`. Wait curto repetido é polling e custa um turno inteiro do modelo por chamada — foi o que aconteceu com teto de 120s (≈100 chamadas por attempt em worker de horas).
- **`serve` despacha concorrente e cancelável.** `run_stdio` é casca sobre `serve(reader, writer, handler)`: uma thread por request, `stdout` atrás de `Mutex`, registro de ids em voo. `notifications/cancelled` flipa o flag do request (`wait_until` checa a cada tick de 250ms) e o request cancelado **não recebe resposta** (contrato MCP). EOF flipa só o flag de saída — waits pendentes morrem, respostas em voo ainda são escritas. `wait_until` é o núcleo puro do wait (poll injetado) para testar deadline, cancel e `next` sem host.
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
- **Os três CLIs da família pi são hook-owned, `pi` inclusive.** `pi`, `omp` e
  `prime-agent` aceitam `-e/--extension` e rodam a mesma API de extensão
  (`agent_start`/`agent_end`), então os três recebem
  `runtimes/_shared/pi-family/assets/lifecycle-extension.js` e declaram
  `lifecycle_hooks` + `notify_when_done`. O append idempotente do flag mora em
  `setup::with_lifecycle_extension`; o gate de alias fica no adapter de cada
  runtime, porque `pi` tem resume/context próprios e não inclui o `mod.rs`
  compartilhado. Runtime sem hooks no catálogo hoje é o `agy` — é ele que os
  testes usam para exercitar o ramo hookless de `derive_activity`.
- **Reinstalação limpa de CLI é o caso normal, não a exceção.** Apagar
  `~/.unpeel` (ou a poda do root legado) some com o diretório onde a extensão
  de lifecycle é escrita, e `write_file_atomic` falhava com `No such file or
  directory`: medido em 2026-09-01, `Failed to install Comet hooks for runtime
  sh.omp.cli (omp)` no `trace.log`. Duas regras saíram disso: o writer cria o
  diretório pai (como `write_executable_script` sempre fez), e
  `install_comet_managed_hooks` acumula falhas em vez de sair no primeiro `?`
  — o loop é o único instalador, então abortar nele deixava todo runtime
  atrás do que falhou (ordem do catálogo) sem hooks, com o usuário vendo
  "todos os outros funcionam".
- **Hook alheio sob `/tmp` não é asset nosso.** `config_has_stale_managed_hook`
  casa root temporário e nome de hook gerenciado **na mesma linha**: o teste
  por arquivo inteiro fazia um wrapper de outra ferramenta em
  `~/.codex/hooks.json` (`/private/tmp/orchestrator-…`) parecer asset stale e
  bloqueava a migração para sempre.

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

**O socket aceito do ingress de hook precisa voltar a bloquear.** O listener é
não-bloqueante de propósito (o loop de accept checa o shutdown), e macOS/BSD
propaga esse `O_NONBLOCK` para o socket **aceito**; `set_read_timeout` não
desfaz, porque `SO_RCVTIMEO` nem é consultado enquanto `O_NONBLOCK` valer. Sem
o `set_nonblocking(false)` em `handle_connection`, todo POST cujos bytes cheguem
depois do accept devolve `WouldBlock` na primeira leitura, `read_request` sai
por `?` **sem escrever resposta**, e o hook é descartado em silêncio — evento de
lifecycle perdido em produção, flake sob paralelismo na suíte (falhava 5/5
rodadas, passava com `--test-threads=1`). Medido em 2026-08-28 com sonda no
`read`: exatamente um `WouldBlock` por falha.

### Test Coverage Matrix

| Camada / path | Tier exigido | Como rodar |
|---|---|---|
| `src/lib.rs` (19 + 10 de hibernação, incluindo portões de evidência e segunda passada), `src/hook_migration.rs` (2 — loop de instalação com instalador injetado, composição install+prune), `src/activity_bridge.rs` (18 local + 11 shared upstream), `src/resources.rs` (8), `src/session_event_journal.rs` (7), `src/project_ledger.rs` (11), `src/project_git.rs` (11), `src/worktree_config.rs` (15), `worktree_setup_wiring_tests` (4) | unit | `cargo test -p zeron-workers-unpeel --lib` |
| `tests/controller_mcp.rs` (29) — Comet-owned MCP surface | integration | `cargo test -p zeron-workers-unpeel --test controller_mcp` |
| `tests/parent_notifications.rs` (17) | integration | `--test parent_notifications` |
| `tests/workspace_trust.rs` (10) | integration | `--test workspace_trust` |
| `tests/settings.rs` (9) — settings snapshot/persistence e preset migration v2 | integration | `--test settings` |
| `tests/project_actions.rs` (5), `tests/local_actions.rs` (4), `tests/session_actions.rs` (4), `tests/local_bootstrap.rs` (2), `tests/dev_demo_fixture.rs` (1) — client actions and deterministic demo state over the local runtime | integration | `cargo test -p zeron-workers-unpeel --test <name>` |
| `tests/hook_migration.rs` (6) | integration | `--test hook_migration` |

## Child DOX Index

None — flat domain.

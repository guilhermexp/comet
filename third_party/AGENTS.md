# AGENTS.md — third_party

Código externo fixado dentro do repositório e referências locais de pesquisa.

## Purpose

- `unpeel/` — snapshot vendorizado de `unpeel-core`, runtimes, protocolos e
  apps do Unpeel. São arquivos Git comuns deste repositório, consumidos por
  `crates/workers-unpeel` via dependência path.
- `cmux/` — checkout local-only do terminal macOS baseado em Ghostty, usado
  apenas como referência de pesquisa.
- `unpeel-upstream.toml` — proveniência verificável do snapshot vendorizado.
- `zui/` — snapshot vendorizado do renderer gpui (`zeronsh/zui`), consumido por `path` no `Cargo.toml` raiz.
- `zui-upstream.toml` — proveniência verificável do snapshot gpui.
- `rust/` — snapshots licenciados de crates.io com patches mínimos de compatibilidade do toolchain e transporte, documentados em `rust/PATCHES.md` e consumidos por `[patch.crates-io]`.

## Ownership

- O projeto mantém o snapshot exato de `unpeel/` e suas patches de
  compatibilidade locais, preservando a licença MIT e atribuição upstream.
- O projeto mantém os snapshots em `rust/` apenas enquanto as dependências pinadas exigirem as correções documentadas, preservando versão, API, licença, checksum de origem e justificativa do patch.
- `cmux/` não é propriedade nem dependência do projeto e permanece untracked.

## Local Contracts

- **`unpeel/` não é submódulo.** `.gitmodules` e o gitlink foram removidos no
  commit `216b61e8`; clone, worktree e CI recebem os arquivos diretamente. Não
  executar `git submodule` para este path.
- A base conhecida antes da vendorização era
  `f27e61a6e4fa5e7180f0cd28c129a3b110a89bbc`. O snapshot veio do working tree
  e carregava 16 mudanças locais; o patch original separado não foi retido.
  `unpeel-upstream.toml` registra essa limitação e o tree id reproduzível.
- O conteúdo autoritativo é a árvore Git em `third_party/unpeel`; a metadata
  descreve proveniência e nenhum build tool a lê.
- O workspace continua com `exclude = ["third_party/unpeel"]` porque o snapshot
  contém workspaces próprios. Só `unpeel-core` entra no build do Comet pela
  dependência path explícita do `Cargo.toml` raiz.
- **`apps/website` foi deletado localmente** (unpeel.com: site, docs, UI de
  compra e o serviço de licença React/Hono em Worker). Nada do Comet o
  consumia — nenhum `Cargo.toml`, script, workflow ou `edge/` o referenciava —
  e ele carregava a única árvore React do repositório. A remoção limpou o que
  ficaria pendurado: os scripts `dev:website`/`build`/`check` do
  `package.json` do workspace Unpeel, o `bun.lock` (regerado, 95 pacotes),
  o gate de changelog em `apps/native/release.sh` e o `cd apps/website` dos
  dois `release:updates:*`, hoje apontando para `apps/releases`. A prosa
  upstream (`unpeel/README.md`, `unpeel/AGENTS.md`, `docs/`) continua citando
  o site de propósito: é documentação do upstream e o sync a sobrescreve.
  Sync futuro do Unpeel reapresenta `apps/website` como adição nova — deletar
  de novo e recomputar `vendored_tree`.
- Patch necessária ao Comet é editada no próprio fonte vendorizado, com teste
  downstream e atualização simultânea de `vendored_tree` na metadata.
- `cmux/` não é rastreado, está excluído em `.git/info/exclude`, e nenhum build,
  CI ou documento operacional pode depender da sua presença.
- O renderer gpui (`zeronsh/zui`) mora em `zui/`. O workspace **exclui** `third_party/zui` porque o snapshot tem workspace próprio; só `gpui` / `gpui_platform` / `gpui_tokio` entram no build do Comet pela dependência path. Não editar o conteúdo do vendor: se o head vendorizado não compilar, reportar — não patchar código de terceiro aqui. Crates GPL do Zed permanecem proibidas.
- O blur WindowServer no macOS retém uma `NSVisualEffectView` padrão, independente do material customizado, atrás do conteúdo: `UnderWindowBackground`, `BehindWindow`, `Active`, alpha 0,01. Reaplicação e troca de raio reutilizam a camada; saída para material ou fundo opaco remove a camada. Mudanças nesse contrato são feitas em fonte separado do zui e copiadas ao snapshot com proveniência, sem alterar tint ou raio do tema para encobrir defeitos.
- `Window::paint_glyph_transformed` (upstream zui `0966d065`) rasteriza glifos monocromáticos uma vez e aplica transformação/blur gaussiano ao compor. Mantenha os layouts de `MonochromeSprite` e `SubpixelSprite` alinhados entre Rust, Metal, WGSL e HLSL, inicialize os campos de blur/padding em todos os construtores e limite as amostras de blur ao tile do atlas.
- O snapshot zui `dce5c1f` acrescenta `Window::native_composition`: handles emprestados com geração para superfícies Windows, `None` nas demais plataformas. Consumidores que retêm handles precisam AddRef e remontar após mudança de geração; o sync preserva os patches locais de vidro, headless e ambas as regras de pontuação.
- Patches em `rust/` não são atualização de dependência: a versão publicada permanece idêntica e a mudança deve se limitar ao diagnóstico documentado que motivou a vendorização. Nova correção exige proveniência em `rust/PATCHES.md`.

## Work Guidance

- Para atualizar Unpeel, obter uma base identificável, comparar a árvore nova
  com o snapshot atual, reaplicar/revisar patches locais explicitamente,
  preservar `LICENSE` e atualizar `base_revision` + `vendored_tree` no mesmo
  commit.
- Não importar `.git`, worktree state, credenciais, caches ou artefatos que não
  sejam dependências binárias intencionais já documentadas.
- Mudanças em `third_party/unpeel` precisam provar o consumidor real com
  `cargo test -p zeron-workers-unpeel`.
- `unpeel-tui/src/activity.rs` also recovers newer durable hooks after latching, consuming each disk version once. Hook-port registry writers in the Comet bridge, Rust TUI and native Swift client must preserve older registrations beyond sixteen entries; registration age is not liveness evidence. The downstream activity/registry regression lives in `activity_bridge` (see its DOX).
- O catálogo de criação distingue ID desconhecido (400) de projeto cadastrado
  bloqueado por identidade (409 com motivo). `checkout_identity_recovery`
  verifica esse contrato pelo adapter; `controller_` verifica o host vendorizado.
- **A máquina de estados de atividade é lida por dois consumidores, e o sweep
  não é fim de turno.** `unpeel-tui/src/activity.rs` é incluída por `#[path]`
  no `activity_bridge` do Comet, então acessor novo se acrescenta AQUI, nunca
  numa cópia local. `hook_owned_state` devolve `Idle` tanto para um
  `Stop`/`StopFailure` real quanto para o sweep de `HOOK_IDLE_TIMEOUT` (5 min
  sem mudança de tela); `hook_confirmed_idle` (patch local) separa os dois por
  `stopped_at`, porque consumidor que age de forma destrutiva sobre ociosidade
  — a hibernação de Workers — mataria turno em andamento com o primeiro. O
  re-arme de `Busy` por crescimento de saída após um `Stop` desconfiado
  (`distrust_stops_while_output_grows`, codex) limpa `stopped_at` como
  `Start`/`UserPromptSubmit`: turno vivo de novo não tem fim confirmado, e
  o sweep seguinte precisa voltar a ler como não confirmado. Para qualquer
  runtime, qualquer crescimento do sinal em `Idle` limpa `stopped_at`; a
  `STOP_REARM_GRACE` decide apenas se `Busy` também será re-armado. Runtime que só posta `Stop`
  não tem hook de início para revogar a confirmação no turno seguinte. A limpeza de
  atenção pelo app (`clear_attention_unconfirmed`, patch local) leva a `Idle`
  sem gravar `stopped_at`, porque um clique não é o runtime dizendo que o
  turno acabou. `PermissionRequest` (incluindo prompt de pergunta ao usuário)
  toma atenção: a regra latch-only para `AskUserQuestion` saiu. Atenção de
  prompt explícito não é limpa por crescimento de sinal de tela; encerra em
  Start/`UserPromptSubmit`, marker de input do controller, Stop, ou o teto
  `HOOK_IDLE_TIMEOUT` (crescimento não rearma). Evento latch-only não avança
  `last_hook_at`.
  `ResumeAdapter::embedded_conversation_id` (patch local, um
  callback por runtime) expõe o id de conversa que o comando já fixa, para a
  sonda de retomada do Comet não depender de comparar receitas.
- **Atividade e hibernação automática se encontram no Session Host.**
  O protocolo 5 minta um token com a revisão em memória do Host mais hook,
  tela, marker de input, geração e incarnação persistidos. `Write`,
  `StreamInput`, resume e cada leitura do PTY avançam a revisão sob o mesmo
  lock usado por `Hibernate`, e o Host recusa mintar (e hibernar) enquanto a
  última atividade estiver dentro de `HIBERNATION_QUIET_WINDOW_MS` — o teto de
  latência entre atividade observada e traço persistido, para o token nunca
  absorver atividade que a avaliação do cliente ainda não viu. Output já
  pendente também rejeita a ação antes do Kill. Todo input de cliente passa por
  `session_ops::write_session_input` (marker + lifecycle lock). `session_ops`
  segura o lifecycle lock, espera o manifest `exited` e só então grava Archive.
  O Archive manual permanece sem precondição.
- **Os dois relógios do caminho de output andam juntos.**
  `SESSION_OUTPUT_BATCH_FLUSH_MS` (session_host, escrita no journal) e
  `OUTPUT_WAIT_POLL_MS` (controller_host, long-poll do `/mobile/output`) são
  independentes, então a diferença entre eles vira batimento: em 32/20 ms um
  quadro de TUI chegava ao Controller em intervalos de 20/40/52/60 ms e o
  terminal do app parecia travado com a vazão média inteira. Hoje são 8/4 ms,
  na faixa dos 12 ms do batcher da engine. Mexer num sem o outro reintroduz a
  jitter — o sintoma não é lentidão, é irregularidade.
- **Runtime packages em `unpeel/runtimes/` são descobertos automaticamente.**
  O pacote `agy/` integra o Antigravity CLI com descriptor `runtime.toml`,
  setup idempotente de workspace trust em `~/.gemini/antigravity-cli/settings.json`,
  resume adapter e ícone autoral. `bun run check:runtimes` verifica somente a
  projeção de descriptors/ícones para o catálogo Swift gerado; não roda Cargo.
  `bun run validate:runtimes` acrescenta
  `cargo test --manifest-path crates/Cargo.toml -p unpeel-core runtime_catalog`,
  então a validação combinada local roda dentro de
  `scripts/cargo-verify.py -- bun run --cwd third_party/unpeel validate:runtimes`.
  Mudanças Rust-only em adapters não alteram o catálogo Swift nem exigem
  geração: rode apenas filtros Rust focados através de uma única invocação do
  wrapper. Se `check:runtimes` encontrar drift, compare a projeção esperada com
  os descriptors/ícones canônicos; atualize o Swift rastreado somente quando
  ele estiver realmente desatualizado. Em 2026-10-08, foi corrigido um drift
  preexistente no catálogo Swift (reliability de atenção de Pi/OMP/Prime e
  capability transcript de OMP) a partir dos descriptors sem alterá-los.
- **Configured worker `initial_text` is native startup, not PTY follow-up.**
  OMP, Claude, Pi and Codex declare `Integration.native_initial_input`.
  `unpeel_core::native_initial` stages a 0600 file under a 0700 session dir.
  `prepare_native_initial_argv` appends a quoted `@file` (OMP/Pi) or `--` plus
  the quoted body (Claude/Codex) only when the body exists; never `$(cat)`.
  `submit_with_native_reservation` claims `.pending` → `.claimed`
  immediately before spawn/PTY write, restores pending only on observed
  pre-submit failure, and never ACKs a missing body. Successful submit keeps
  the Host alive if receipt persistence fails (reservation stays consumed).
  Native is PasteAndSubmit-only via `native_initial_startup_enabled`.
  The stored Session command never carries the task; restart must not replay.
  The Codex command wrapper must not log argv. Before exec it removes PATH
  entries resolving to itself by file identity, so upstream launchers cannot
  recurse through the managed wrapper. Prime-agent keeps interactive delivery.
- **Lifecycle da família pi e isolamento de subagentes OMP.** `pi`, `omp` e
  `prime-agent` emitem `Start`/`Stop` em `agent_start`/`agent_end` e
  `PermissionRequest`/`UserPromptSubmit` em `ui_prompt_start`/`ui_prompt_end`
  (prompt bloqueante, com `tool_name`), com id de conversa e transcript do
  provider. `attention_reliable = true` no `runtime.toml` dos três descreve
  esse transporte. Pi e Prime compartilham
  `pi-family-lifecycle-extension.js`; OMP usa `omp-lifecycle-extension.js` para
  descartar eventos `ctx.agent.kind === "sub"` e, para extensões antigas sem
  esse campo, reconhecer somente o layout canônico de artefato filho OMP.
  `kind === "main"` prevalece sobre o fallback de caminho, permitindo que o
  Worker primário retome um transcript após troca/reabertura. O append e a
  migração idempotentes moram em
  `_shared/pi-family/adapter/setup.rs::with_lifecycle_extension`; o gate de
  alias fica no adapter de cada runtime, porque `pi` tem resume/context próprios
  e não inclui o `mod.rs` compartilhado. `runtime.toml` com `source = "hooks"`
  exige a capability `lifecycle_hooks` (e `completion_reliable` exige
  `notify_when_done`) — o catálogo valida os dois pares. `omp` também declara
  `transcript`; o adaptador lê JSONL só no diretório gerenciado da sessão
  sob `<unpeel_home>/pi-sessions` (stem exato, sem walk global).
- **Asset gerenciado cria o diretório dele.** `hook_assets::write_file_atomic`
  faz `create_dir_all` do pai: um root apagado (reinstalação limpa de CLI,
  poda do root legado) fazia a instalação inteira morrer com `No such file or
  directory`.
- Telemetria provider-owned permanece no pacote do runtime: o adapter OMP lê
  somente JSONL canônico sob o diretório `sessions` resolvido por
  `--session-dir` explícito ou pelo layout oficial do agent padrão,
  `PI_CODING_AGENT_DIR`, profile nomeado ou XDG existente; exige que o registro
  `session` declare o provider Session ID persistido e limita a leitura a 2 MiB
  por linha, 16 MiB totais, 100.000 registros e 128 modelos. A última transição
  de modelo/thinking vira ativa imediatamente, mesmo ainda com zero tokens.
  `unpeel-core` persiste apenas a projeção provider-neutral vinculada ao ID e ao
  path canônico, invalida-a de forma fail-closed em rejeição definitiva de
  confiança/budget mesmo quando o marker não pode ser removido e não publica
  marker de binding anterior. O Host publica campos opcionais;
  transcript bruto, custo e conteúdo de mensagem nunca atravessam a fronteira
  vendorizada.
  O formato local transitório sem binding nunca é aceito diretamente: ele só
  dispara uma releitura do provider atual no startup, que o substitui pelo
  marker vinculado; marker já vinculado não é relido por essa migração.

## Verification

### Test Coverage Matrix

| Camada / path | Tier exigido | Como rodar |
|---|---|---|
| `third_party/unpeel/scripts/generate-runtime-client-catalog.mjs` + `third_party/unpeel/apps/shared/UnpeelShared/Sources/UnpeelShared/GeneratedRuntimeCatalog.swift` | unit — projeção determinística de descriptors/ícones; sem Cargo | `bun run --cwd "$PWD/third_party/unpeel" check:runtimes` |
| `third_party/unpeel/runtimes/**` + `crates/unpeel-core/src/session_telemetry.rs` | unit + integration downstream — parser/provider fixtures, trusted path e Host wire | `scripts/cargo-verify.py -- bash -c 'cargo test --manifest-path third_party/unpeel/crates/Cargo.toml -p unpeel-core && cargo test -p zeron-workers-unpeel'` |
| `third_party/unpeel/crates/unpeel-core/src/{session_host,session_ops}.rs` + `crates/unpeel-host/tests/agent_restart_process.rs` (15) | integration — protocolo real do Host, incluindo invalidação de hibernação por input/output, janela de quietude e o caminho aceito | `scripts/cargo-verify.py -- cargo test --manifest-path third_party/unpeel/crates/Cargo.toml -p unpeel-host --test agent_restart_process` |
| `third_party/cmux` | none — referência local untracked | — |
| `third_party/rust/*` | integration — compatibilidade transitiva do build macOS | `cargo check -p zeron-ui --message-format short` |
| `third_party/zui` | unit no vendor (API de blur e ambas as regras de pontuação) + compile downstream da API de glifo transformado | `cargo test --manifest-path third_party/zui/Cargo.toml -p gpui_macos --features runtime_shaders --lib window_blur_` · `cargo test --manifest-path third_party/zui/Cargo.toml -p gpui --features gpui_platform/runtime_shaders --lib closing_punctuation_attached` · `cargo build -p zeron` |
| `third_party/zui/crates/gpui_macos/tests/window_glass_backing.rs` | integration — setters reais, configuração/ordem nativa, reutilização, resize e limpeza; main thread AppKit | `cargo test --manifest-path third_party/zui/Cargo.toml -p gpui_macos --features runtime_shaders --test window_glass_backing` (macOS com sessão gráfica; no CI `ui-tests.yml`, build `--no-run` e execução em bundle isolado com timeout, exit zero e marcador PASS obrigatório) |
| Estabilidade óptica do vidro durante repaint | none — asserções AppKit não comprovam desaparecimento das ondas; comparação headed no mesmo desktop é evidência separada | Captura/comparação do vidro real sem trocar wallpaper ou preferências do usuário |

## Child DOX Index

| Domínio | Doc | Papel |
|---|---|---|
| Unpeel | [`unpeel/AGENTS.md`](unpeel/AGENTS.md) | Contratos upstream e verificação executável de hooks do fork |
| zui | — | Snapshot vendorizado do gpui; regra local vive neste doc |

# AGENTS.md — third_party

Código externo fixado dentro do repositório e referências locais de pesquisa.

## Purpose

- `unpeel/` — snapshot vendorizado de `unpeel-core`, runtimes, protocolos e
  apps do Unpeel. São arquivos Git comuns deste repositório, consumidos por
  `crates/workers-unpeel` via dependência path.
- `cmux/` — checkout local-only do terminal macOS baseado em Ghostty, usado
  apenas como referência de pesquisa.
- `unpeel-upstream.toml` — proveniência verificável do snapshot vendorizado.
- `rust/` — snapshots licenciados de crates.io com patches mínimos de compatibilidade do toolchain, documentados em `rust/PATCHES.md` e consumidos por `[patch.crates-io]`.

## Ownership

- O projeto mantém o snapshot exato de `unpeel/` e suas patches de
  compatibilidade locais, preservando a licença MIT e atribuição upstream.
- O projeto mantém os snapshots em `rust/` apenas enquanto a resolução transitiva do GPUI pinado exigir as versões incompatíveis, preservando versão, API, licença, checksum de origem e justificativa do patch.
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
- Patch necessária ao Comet é editada no próprio fonte vendorizado, com teste
  downstream e atualização simultânea de `vendored_tree` na metadata.
- `cmux/` não é rastreado, está excluído em `.git/info/exclude`, e nenhum build,
  CI ou documento operacional pode depender da sua presença.
- O fork gpui (`wingleeio/zed`) continua uma dependência Git do Cargo, não um
  diretório desta árvore. Crates GPL do Zed permanecem proibidas.
- Patches em `rust/` não são atualização de dependência: a versão publicada permanece idêntica e a mudança deve se limitar ao diagnóstico futuro que motivou a vendorização. Nova correção exige proveniência em `rust/PATCHES.md`.

## Work Guidance

- Para atualizar Unpeel, obter uma base identificável, comparar a árvore nova
  com o snapshot atual, reaplicar/revisar patches locais explicitamente,
  preservar `LICENSE` e atualizar `base_revision` + `vendored_tree` no mesmo
  commit.
- Não importar `.git`, worktree state, credenciais, caches ou artefatos que não
  sejam dependências binárias intencionais já documentadas.
- Mudanças em `third_party/unpeel` precisam provar o consumidor real com
  `cargo test -p zeron-workers-unpeel`.
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
  turno acabou. `ResumeAdapter::embedded_conversation_id` (patch local, um
  callback por runtime) expõe o id de conversa que o comando já fixa, para a
  sonda de retomada do Comet não depender de comparar receitas.
- **Atividade e hibernação automática se encontram no Session Host.**
  O protocolo 5 minta um token com a revisão em memória do Host mais hook,
  tela, geração e incarnação persistidos. `Write`, `StreamInput` e cada leitura
  do PTY avançam a revisão sob o mesmo lock usado por `Hibernate`; output já
  pendente também rejeita a ação antes do Kill. `session_ops` segura o
  lifecycle lock, espera o manifest `exited` e só então grava Archive. O
  Archive manual permanece sem precondição.
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
  resume adapter e ícone autoral. Validação usa `bun run validate:runtimes`
  em `third_party/unpeel`.
- **A extensão de lifecycle da família pi serve os três CLIs.** `pi`, `omp` e
  `prime-agent` recebem `--extension
  <unpeel_home>/hooks/pi-family-lifecycle-extension.js` e emitem `Start`/`Stop`
  com id de conversa e transcript do provider. O append idempotente é
  `_shared/pi-family/adapter/setup.rs::with_lifecycle_extension`; cada runtime
  mantém o próprio gate de alias, porque `pi` tem resume/context próprios e não
  inclui o `mod.rs` compartilhado. `runtime.toml` com `source = "hooks"` exige
  a capability `lifecycle_hooks` (e `completion_reliable` exige
  `notify_when_done`) — o catálogo valida os dois pares.
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
| `third_party/unpeel/runtimes/**` + `crates/unpeel-core/src/session_telemetry.rs` | unit + integration downstream — parser/provider fixtures, trusted path e Host wire | `bun run --cwd "$PWD/third_party/unpeel" validate:runtimes` · `cargo test --manifest-path third_party/unpeel/crates/Cargo.toml -p unpeel-core` · `cargo test -p zeron-workers-unpeel` |
| `third_party/unpeel/crates/unpeel-core/src/{session_host,session_ops}.rs` + `crates/unpeel-host/tests/agent_restart_process.rs` (14) | integration — protocolo real do Host, incluindo invalidação de hibernação por input/output | `cargo test --manifest-path third_party/unpeel/crates/Cargo.toml -p unpeel-host --test agent_restart_process` |
| `third_party/cmux` | none — referência local untracked | — |
| `third_party/rust/*` | integration — compatibilidade transitiva do build macOS | `cargo check -p zeron-ui --message-format short` |

## Child DOX Index

None — flat domain.

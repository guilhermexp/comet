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
- Telemetria provider-owned permanece no pacote do runtime: o adapter OMP lê
  somente JSONL canônico sob a raiz confiável de Sessions, e `unpeel-core`
  valida/persiste apenas a projeção provider-neutral. O Host publica campos
  opcionais; transcript bruto, custo e conteúdo de mensagem nunca atravessam
  a fronteira vendorizada.

## Verification

### Test Coverage Matrix

| Camada / path | Tier exigido | Como rodar |
|---|---|---|
| `third_party/unpeel/runtimes/**` + `crates/unpeel-core/src/session_telemetry.rs` | unit + integration downstream — parser/provider fixtures, trusted path e Host wire | `bun run --cwd "$PWD/third_party/unpeel" validate:runtimes` · `cargo test --manifest-path third_party/unpeel/crates/Cargo.toml -p unpeel-core` · `cargo test -p zeron-workers-unpeel` |
| `third_party/cmux` | none — referência local untracked | — |
| `third_party/rust/*` | integration — compatibilidade transitiva do build macOS | `cargo check -p zeron-ui --message-format short` |

## Child DOX Index

None — flat domain.

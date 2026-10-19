# comet — instruções do repo

Fork de [zeronsh/comet](https://github.com/zeronsh/comet) (MIT). Controlador multi-device de coding agents (Claude Code / Codex): cada máquina roda uma **engine** (daemon Rust) e as sessões sincronizam por **CRDT Loro** através de **Durable Objects** na Cloudflare. Rewrite nativo em Rust + gpui do comet original em TS/Electron.

Detalhe canônico de arquitetura: `ARCHITECTURE.md`. Paridade contra o app original: `docs/PARITY.md`. Inventário funcional: `FUNCTIONAL-BASELINE.html`.

Mapa de referência do streaming do Chat: [`docs/streaming-guide.md`](docs/streaming-guide.md) — catálogo de eventos/rows, agrupamentos, duplicações e caminhos de render, observado em 2026-09-09. Consulte antes de corrigir o streaming e revalide os trechos afetados; o guia é uma fotografia, não substitui os contratos DOX/OpenSpec.

Terminologia canônica de produto vive em [`CONTEXT.md`](CONTEXT.md). Leia antes de nomear qualquer coisa. Duas colisões que o código já carrega: **Chat** é a conversa durável (a linha da sidebar, o que se exporta) enquanto **Session** é o estado de execução dela num device — chamar Chat de "session" nomeia a coisa errada. E **Managed Provider Usage** é quota de assinatura device-local, nunca billing de API nem usage sincronizado.

## Stack

- **Rust workspace** (edition 2024, `resolver = "2"`) — `crates/{proto,doc,sync,harness,engine,rpc,syntax,theme,ui,visual,visual-gpu,zatex,update,workers-unpeel,sessions-mcp,computer-use-mcp,preview,voice,audio,voice-media,voice-session,orb,veil}` + `apps/zeron` (membro padrão do workspace).
- **UI = gpui**, snapshot vendorizado de `zeronsh/zui` em `third_party/zui` (Apache-2.0; proveniência em `third_party/zui-upstream.toml`). Não usamos as crates GPL do Zed (`markdown`, `ui`, `theme`, `editor`) — markdown, componentes e tema são nossos.
- **Sync = loro 1.13 + loro-protocol 0.3** (twin Rust do pacote npm que a edge fala).
- **Edge = TypeScript** (`edge/`) — Worker + SessionRoom DO (por chat) + DeviceRoom DO (por device) + R2 + auth WorkOS. Sem Postgres nem Hono server. Sync não usa WebRTC; previews de servidores usam RTC autenticado por PreviewRoom.
- **apps/ios** — cliente iOS (projeto Xcode), fora do workspace Cargo.
- Binário único `zeron`: headed (gpui) ou `zeron headless` (só engine).

## Comandos

| Ação | Comando |
|---|---|
| Build do app (membro padrão do workspace) | `cargo build` |
| Dev nativo (macOS com identidade Zeron) | `cargo run` |
| Monitor de nitidez nativa (macOS, app já aberto) | `python3 scripts/ui-sharpness-monitor.py` · [guia](docs/ui-sharpness-monitor.md) |
| Suite completa | `cargo test --workspace` |
| Testes de uma crate | `cargo test -p zeron-ui` |
| Verificação Cargo local no macOS/Linux | `scripts/cargo-verify.py -- cargo test -p zeron-ui` |
| Formatação (obrigatória antes de merge do upstream) | `cargo fmt --all` |
| Demo local offline (harness mock, seeded) | `scripts/dev-demo.sh` (`--slow` pra ver streaming) |
| Smoke e2e | `scripts/e2e-smoke.sh` |
| Edge | `npm -C edge run dev\|test\|typecheck\|deploy` |
| Packaging | `scripts/package-linux.sh` · `scripts/package-macos.sh` (local: também instala em `/Applications/Zeron.app`, sem abrir) |

O job Rust em `.github/workflows/rust.yml` provisiona Bun para os testes executáveis da extensão lifecycle pi-family do Unpeel vendorizado.

Para baterias pesadas, prefira o CI existente e confira revisão, plataforma, jobs e etapas realmente executados conforme a skill `implement` do harness. Considere também a compilação: mesmo um filtro de `cargo test -p zeron-ui` pode criar gigabytes de dependências. Checks locais baratos e validação nativa específica continuam úteis; quando Cargo local for necessário, use a proteção abaixo. Reaproveite provas válidas da revisão avaliada e não repita a bateria local apenas para espelhar CI. `no-mistakes` não é etapa automática de desenvolvimento ou publicação neste projeto; só executar por pedido explícito do usuário.

Cobertura remota: `rust.yml` executa workspace + Unpeel no Linux quando mudam inputs Rust, incluindo `edge/src/install.sh`, consumido pelos testes do binário (não por alterações isoladas em `apps/ios/`, `scripts/ios/` ou outros arquivos de `edge/`); `ui-tests.yml` seleciona jobs Linux, macOS e iOS por arquivos e dependências Cargo transitivas em PRs e pushes na `main`. O networking Linux de preview está no `core-tests` de UI; `preview-tests.yml` cobre networking macOS e coordinator Linux. Os filtros de UI/native/iOS incluem `visual`, `visual-gpu` e `zatex`; o core nextest inclui suas suites unitárias. Isso descreve a receita, não garante sucesso: confira os jobs e etapas realmente executados na revisão avaliada. O job nativo macOS mantém orçamento de 90 minutos, mas seleciona separadamente app check, geometria, notificações, anexos, browser, preview e GPUI. Código compartilhado preserva cobertura nativa completa. Linux não cobre `native_preview_focus` (AppKit com sessão gráfica macOS).

CI de testes cancela PRs e pushes ultrapassados na mesma ref. Para não perder alterações de runs cancelados, UI compara pushes com um ancestral aprovado pelo próprio workflow; sem base confiável, roda tudo. `release-profile-tests.yml` mantém consulta diária, mas só reaproveita o mesmo SHA quando os dois grupos realmente passaram há menos de sete dias; falha, cancelamento, mudança de SHA, prazo semanal ou disparo manual executam a suíte. Detalhes e testes baratos dos seletores: [`scripts/AGENTS.md`](scripts/AGENTS.md). Publicação por tag (`release.yml`) e deploy do Worker (`deploy.yml`) são independentes desses filtros.

## Remotes e publicação

- `origin` = `guilhermexp/comet` (nosso fork) · `upstream` = `zeronsh/zeron` (terceiro, MIT; renomeado de `zeronsh/comet`). O upstream reassinou o histórico em set/2026. Receita, decisões e histórico anterior: [`docs/upstream-sync.md`](docs/upstream-sync.md). Desde 2026-10-09, cada execução da skill `upstream-sync` gera um [registro YAML versionado](.upstream-sync/history/), inclusive sem novidades ou com falha; confronte os registros com o Git atual antes do próximo sync.
- **Nunca pushar para o upstream.** Qualquer push vai pro fork.
- `gh` resolve pro upstream por default: **sempre passar `-R guilhermexp/comet`** em `gh run list`, `gh release view`, etc.
- `.github/workflows/deploy.yml` é herdado do upstream: publica o Worker Cloudflare em push na `main` que toque `edge/`. `release.yml` é do fork e só Mac: tag `v<versão do Cargo.toml>` publica dmg + tarball do app + `manifest.json` numa GitHub Release, e o app instalado se atualiza dela (`crates/update`, `COMET_RELEASES_URL`). Versão própria a partir de `1.0.0`; em merge do upstream, manter a nossa.
- **Publicação direta e validação proporcional** (decisão do dono, 2026-10-19): após os checks focados necessários, commit e `git push origin HEAD:main`. **Não iniciar `no-mistakes` automaticamente**, mesmo em mudanças grandes, de wire ou múltiplos subsistemas. Só executar quando o usuário pedir explicitamente esse gate. Não acrescentar rodadas de revisão ou repetir provas já válidas apenas como ritual de publicação.
- **Sync do upstream:** [sync-fork.yml](.github/workflows/sync-fork.yml) roda só por disparo manual (sem cron desde 2026-10-15). Política autorizada: sync direto se limpo; PR draft de diagnóstico se conflitar; PR aberta pausa novos syncs. Resolução por agente na `agents-us` é sob demanda, nunca automática. Credencial, limites, prova remota e procedimento da box ficam em [`docs/upstream-sync.md`](docs/upstream-sync.md#automação-recorrente).

## Gotchas duráveis

- **Verificações Cargo locais no macOS/Linux usam `scripts/cargo-verify.py -- <comando>`.** O wrapper impede verificações simultâneas entre checkouts, usa um `CARGO_TARGET_DIR` isolado de caminho fixo (para o sccache reaproveitar as dependências), compila incremental, roda com QoS de background no macOS (só E-cores; `COMET_CARGO_VERIFY_FOREGROUND=1` desliga), limita os jobs a 4 por padrão, recusa começar com menos de 60 GiB livres, interrompe se cair abaixo de 20 GiB e remove o target ao terminar ou receber SIGINT/SIGTERM/SIGHUP. Se o wrapper levar SIGKILL, um watchdog derruba o grupo do Cargo e a próxima execução remove o target órfão. Agrupar testes da mesma rodada em uma invocação (`scripts/cargo-verify.py -- bash -c 'cargo test -p zeron-engine && cargo test -p zeron-ui'`) para reutilizar a compilação dentro dela. Vale também para testes locais durante o no-mistakes; CI remoto e Windows seguem os comandos Cargo da tabela. `cargo run` do app de desenvolvimento continua usando o target incremental normal. Não compartilhar um target persistente entre checkouts com fontes diferentes: isso pode reutilizar artefatos stale.
- **No macOS, rode um único gate `no-mistakes` com testes Rust por vez.** Cada run tem um checkout/target próprio, e dois runs simultâneos podem somar dezenas de gigabytes mesmo quando cada um limpa no fim. Antes de abrir outro gate, aguarde o primeiro terminar e confira o espaço livre. Não rode testes Cargo manuais em paralelo com o gate.

- **`cargo run` usa o checkout atual do Comet e o OMP instalado.** Não impor OMP de fonte em `.cargo/config.toml`: desenvolvimento com o checkout irmão é opt-in via `OMP_EXECUTABLE="$PWD/scripts/omp-dev" cargo run` (contrato em `scripts/AGENTS.md`). Outro worktree tem código e binário próprios; executar ali não inclui mudanças locais deste checkout.

- **Sync com o upstream é frequente** (várias versões por semana). Na integração assistida, rode `cargo fmt --all` do nosso lado **antes** do merge. Preserve a intenção do fork ao resolver cada conflito e registre o motivo. O closeout da skill `upstream-sync` atualiza seu histórico e reconcilia `docs/upstream-sync.md`; a automação recorrente tem o contrato próprio nessa documentação.
- `crates/tui` / `apps/tui` foram **deletados** (upstream removeu o viewport ratatui). Isso **não** é o painel de terminal dentro do app — esse vive em `crates/ui/src/terminal/` e está intacto.
- `dist/` guarda **assets-fonte** de packaging (ícone, `.desktop`, `Info.plist`), consumidos por `scripts/package-*.sh` e pelo workflow de release. Só `edge/dist/` é gerado/ignorado — não apagar a `dist/` da raiz.
- Build do gpui é caro; `[profile.dev]` já usa `opt-level = 2` pras deps. Primeira build leva minutos.
- Re-vendorizar `third_party/zui` exige verificar ambas as regras de `comet/line-wrap-closing-punctuation` (`line_wrapper` e `line_layout`); não editar o conteúdo do vendor no lugar.
- **Codex voice usa o helper standalone instalado**, com media local/remote na engine host e clientes desktop/iOS; não embute runtime Codex/GStreamer no pacote. A integração antiga de Live Voice OMP foi removida; não restaurar seu microfone, probes ou RPCs no composer.
- Este é um repo de terceiro sob MIT. Preservar licença e atribuição.

## Onde mudar o quê

- Mudança de **comportamento de capability** → abrir change no OpenSpec (`openspec/`) antes de editar. Convenções de autoria em `openspec/project.md`.
- Mudança **local de código** → ler a cadeia de `AGENTS.md` até o alvo (Child DOX Index abaixo) e fazer a edição mínima ali.

## DOX Framework

- Este repo usa DOX: AGENTS.md hierárquico, 1 por domínio/pasta durável. Cada AGENTS.md é contrato vinculante da sua subárvore.
- DOX é o eixo ESPAÇO (onde o código mora, como editar aqui). O eixo TEMPO (o que mudar, capability nova/breaking) é OpenSpec — antes de mudar comportamento, ver `openspec/` e seguir `openspec/project.md`. DOX não reescreve as rules do OpenSpec.

### Read Before Editing
1. Ler este AGENTS.md (raiz) + identificar cada path que vai tocar.
2. Caminhar da raiz até cada alvo, lendo todo AGENTS.md no caminho (Child DOX Index aponta o próximo).
3. Doc mais próximo controla detalhe local; pais controlam regra repo-wide. Em conflito, o mais próximo vence no detalhe — nenhum filho enfraquece DOX nem OpenSpec.
4. Não confiar em memória: re-ler a cadeia DOX na sessão atual antes de editar. Fazer a edição MÍNIMA no lugar certo (não duplicar função, não criar helper novo se dá pra estender).

### Update After Editing (DOX pass — obrigatório no closeout)
- Toda mudança significativa: atualizar o AGENTS.md dono mais próximo + pais afetados + Child DOX Index. Remover texto stale na hora.
- Atualizar quando muda: propósito, escopo, ownership, estrutura durável, contratos, workflow, inputs/outputs/permissões/constraints, preferência durável do usuário, ou criação/move/rename de AGENTS.md.
- Mudança de comportamento de capability → também rodar o ciclo OpenSpec (validate → archive).

### Child Doc Shape
Criar AGENTS.md filho quando a pasta vira boundary durável com regra própria — boundary aqui é seam (o comportamento pode ser substituído sem editar o consumidor), não tamanho nem contagem de arquivos. Seções (vazias se não há padrão ainda):
- **Purpose** · **Ownership** · **Local Contracts** · **Work Guidance** · **Verification** · **Child DOX Index**

A seção **Verification** carrega a **Test Coverage Matrix** local (`camada/path → tier`: `unit` | `e2e` | `integration` | `none`) — fonte de verdade do tier que cada `Scenario` do OpenSpec carimba (`Test:`). Semeada das convenções que **já existem** na subárvore, incrementalmente por change. `none` é decisão com motivo, não omissão.

### Closeout
1. Re-checar paths mudados contra a cadeia DOX.
2. Atualizar docs donos + pais/filhos afetados + cada Child DOX Index.
3. Remover texto stale/contraditório.
4. Rodar verificação existente (testes/lint) + ciclo OpenSpec se mudou comportamento.

## Verification

Suítes canônicas por superfície (detalhe e matriz `Test:` ficam no `AGENTS.md` dono):

| Superfície | Comando |
|---|---|
| Workspace Rust | `cargo test --workspace` (no macOS/Linux local: `scripts/cargo-verify.py -- cargo test --workspace`) |
| Edge Cloudflare | `npm -C edge run test` (unit + workerd) |
| Cliente iOS | `xcodebuild test -project apps/ios/Zeron.xcodeproj -scheme Zeron -destination 'platform=iOS Simulator,name=<device do runtime instalado>'` |
| Fluxo headed/headless | `scripts/e2e-smoke.sh` |
| Sync recorrente | Disparar `gh workflow run sync-fork.yml -R guilhermexp/comet --ref main`; acompanhar o run exato e conferir SHA/PR conforme [`docs/upstream-sync.md`](docs/upstream-sync.md#comprovação-da-automação) |

### Build

```json
{
  "command": {
    "argv": ["cargo", "build"],
    "cwd": "."
  }
}
```

## Child DOX Index

| Domínio | Doc | O que mora ali |
|---|---|---|
| Workspace Rust | [`crates/AGENTS.md`](crates/AGENTS.md) | As crates da lib: wire types, docs CRDT, sync, harnesses, engine/uploads, RPC, syntax, tema, workers-unpeel, updater, UI/composer, export de Chat Transcript e decoração markdown |
| Binário e clientes | [`apps/AGENTS.md`](apps/AGENTS.md) | `apps/zeron` (CLI headed/headless) e `apps/ios` |
| Edge Cloudflare | [`edge/AGENTS.md`](edge/AGENTS.md) | Worker, SessionRoom/DeviceRoom DOs, R2, auth WorkOS |
| Scripts | [`scripts/AGENTS.md`](scripts/AGENTS.md) | Dev demo, smoke e2e, packaging Linux/macOS |
| Código externo | [`third_party/AGENTS.md`](third_party/AGENTS.md) | Unpeel e zui vendorizados, patches Rust licenciados, proveniência e referências locais |

<!-- trail:start -->
## Trail — repo context graph

This repo is indexed in `.trail/cache/`: small linked markdown nodes that explain each
system and carry exact file:line spans, kept in sync with the code through git.

For ANY task here — understanding how something works, finding where code lives,
or scoping a change — get context from the graph before grepping or opening
source files. Re-ask freely (it's cheap) and reuse literal identifiers you
already have (symbol, error string, file name) as the query. New to this repo?
Run `trail map` first — a token-budgeted orientation (dir clusters, hubs,
hotspots), no LLM, no key.

- Run `trail ask "<your question>" --source` → ranked nodes with the relevant
  code spans inlined (each hit's ≤8-line crux by default; `--full` for whole
  definitions when the crux isn't enough). Match the tool to the task shape:
  for understanding or editing, the top node IS the answer — cite its
  `covers:` file:line spans and edit straight from `--source`. For
  exhaustive tasks ("every occurrence / every caller of this pattern"), ranked
  results are top-N, not complete — run `trail grep "<literal>"` instead
  (exhaustive over indexed files, grouped by enclosing symbol), falling back
  to raw `grep -rn` only for unindexed files.
- `trail skeleton <file>` → every definition's signature + span, ~10× cheaper
  than reading the file; use it to skim an API surface.
- `trail callers <symbol>` gives precomputed, exact edges — who calls this.
  Add `--direction out` for what it calls, or `--depth N` to walk
  transitively for the full blast radius. For structural questions, skip
  ranking and use this directly.
- Or browse: `.trail/cache/INDEX.md` lists every node; follow the links.
- Monorepos and folders of multiple repos rank fairly across sub-projects —
  hits carry `[scope/]` labels naming which one they're from. Narrow with
  `trail ask "<task>" --in <scope>/` once you know where you're working.

If a returned span is truncated ("+N more lines"), open the file at that exact
range before finalizing. Only open source files when a node genuinely lacks a
needed detail, and then at the exact file:line the node points to — never
re-read whole files.

After big code changes, refresh the graph with `trail build` (deterministic,
no API key, $0).
<!-- trail:end -->

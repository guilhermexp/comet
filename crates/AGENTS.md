# crates — workspace Rust

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

As bibliotecas que compõem o comet. A camada de dependência sobe assim: `proto` (tipos) → `doc` (schema CRDT) → `sync` (transporte Loro) → `harness` (agentes) → `engine` (backend) → `rpc` (fronteira tipada) → `ui` (consumidor). `preview` é consumida pela engine para HTTP/RTC; `syntax`, `theme` e `workers-unpeel` são fronteiras laterais consumidas pela `ui` sem depender da engine. A engine também usa `workers-unpeel` para o serviço comum de worktree, a atividade local e a autorização de Source Control de checkouts Worker pelo link com o projeto (Space). `workers-unpeel` fala com a engine só pelo RPC (`zeron-proto`/`zeron-rpc`), para ler e criar projetos no registro único. `voice` (ditado local, modelo Parakeet opcional) é consumida só pela `ui`, sem RPC nem sync. `sessions-mcp` é fronteira lateral de stdio: depende só de `proto` e `rpc`, e o binário `zeron` a lança; a engine não linka essa crate. Nada abaixo depende de nada acima.

## Ownership

Todas as crates são internas (`publish = false`) e versionadas juntas pelo `[workspace.package]` da raiz. Adicionar crate exige entrada em `members` **e** em `[workspace.dependencies]` do `Cargo.toml` raiz.

## Local Contracts

- **Versões de dependência moram no `Cargo.toml` da raiz**, não nas crates. Crate filha usa `dep = { workspace = true }`. Não pinar versão local.
- `edition = "2024"` em todas.
- Runtime async é **tokio** em todo lugar; a UI faz a ponte por `gpui_tokio` (`Tokio::spawn` vira `Task` do gpui). A UI nunca bloqueia na engine.
- Bloquear dentro de contexto async é bug, não estilo — já custou findings de review (`rpc.rs`, `repos.rs`).
- **Configured worker presets receive `launch_worker` briefing at native
  startup.** OMP, Claude, Pi and Codex all use `unpeel-core::native_initial`
  (claim before spawn; ACK after submit without abandoning the child).
  Adapters declare `FileArgument` (`@file`: OMP/Pi) or `PositionalPrompt`
  (`--` + quoted body: Claude/Codex). Prime-agent and other integrations keep
  interactive delivery. Regression seam:
  `cargo test -p zeron-workers-unpeel --test worker_initial_briefing`.
- Live Voice é lifecycle da `engine`; a `ui` só projeta e controla, e navegação de Chat nunca manda stop. Em `Working`/`AwaitingInput`, a elegibilidade exige suporte OMP a contexto operacional silencioso; em `Idle`, um handle OMP estacionado permanece elegível com Live básico.

## Work Guidance

- Mudança que atravessa duas crates quase sempre indica que o tipo está na camada errada: tipo de fio vai pra `proto`, forma de documento vai pra `doc`.
- `update/` e `syntax/` são crates de arquivo único (auto-update do binário; tokenizer de highlight) — cada uma tem doc próprio, mas nenhuma tem submódulo interno.

## Verification

- Comandos: `cargo test --workspace` · `cargo test -p <crate>` · `cargo fmt --all` · `cargo build -p zeron`

| Camada / path | Tier exigido | Como rodar |
|---|---|---|
| `crates/*/src/**` (lógica pura) | unit (`mod tests` co-located) | `cargo test -p <crate>` |
| `crates/{engine,harness,rpc,workers-unpeel,doc,sync}/tests/**` | integration | `cargo test -p <crate>` |
| `crates/ui/src/**` (estado, derivações) | unit | `cargo test -p zeron-ui` |
| `crates/ui` (render gpui) | none — sem harness de render; validação é visual no `scripts/dev-demo.sh` | — |
| `crates/update` | unit (versão/manifest, swap symlink, shutdown, restart gate) | `cargo test -p zeron-update` |

## Child DOX Index

| Crate | Doc | Papel |
|---|---|---|
| `zeron-proto` | [`proto/AGENTS.md`](proto/AGENTS.md) | Tipos de fio + derivações puras compartilhadas |
| `zeron-doc` | [`doc/AGENTS.md`](doc/AGENTS.md) | Schemas Loro (session/workspace) + mirror layer |
| `zeron-sync` | [`sync/AGENTS.md`](sync/AGENTS.md) | Cliente de room Loro + DocsStore |
| `zeron-harness` | [`harness/AGENTS.md`](harness/AGENTS.md) | Adaptadores Claude Code / Codex / mock |
| `zeron-engine` | [`engine/AGENTS.md`](engine/AGENTS.md) | Backend: sessões, doc host, repos, terminais, uploads, auth |
| `zeron-rpc` | [`rpc/AGENTS.md`](rpc/AGENTS.md) | UiRpc/ControlRpc tipados sobre WS + transporte in-memory |
| `zeron-syntax` | [`syntax/AGENTS.md`](syntax/AGENTS.md) | Tokenizer tree-sitter paint-only compartilhado pelas surfaces |
| `zeron-theme` | [`theme/AGENTS.md`](theme/AGENTS.md) | Modelo source-neutral, catálogo e importação de temas |
| `zeron-sessions-mcp` | [`sessions-mcp/AGENTS.md`](sessions-mcp/AGENTS.md) | MCP `comet-sessions` (`help`, `list_spaces`, `create`) |
| `zeron-workers-unpeel` | [`workers-unpeel/AGENTS.md`](workers-unpeel/AGENTS.md) | Fronteira tipada sobre `third_party/unpeel`: projetos, worktrees, sessões de Worker, controller MCP, notificações ao parent |
| `zeron-preview` | [`preview/AGENTS.md`](preview/AGENTS.md) | Catálogo, proxy HTTP/WebSocket e pairing RTC autenticado |
| `zeron-ui` | [`ui/AGENTS.md`](ui/AGENTS.md) | App gpui: shell, transcript e export puro, composer/intake e decoração paint-only, terminal, diff |
| `zeron-update` | [`update/AGENTS.md`](update/AGENTS.md) | Checagem de release e auto-update do binário |
| `zeron-markdown` | — (`markdown/src/lib.rs`) | Parser markdown por blocos + reparse incremental e heurística de path (`file_path`), UI-free; compartilhado por `zeron-ui` e pelo core mobile |
| `zeron-text` | — (`text/src/lib.rs`) | Medição e quebra de linha analítica (rustybuzz + fallback CoreText) do app iOS |
| `zeron-client` | — (`client/src/lib.rs`) | Thin client sem engine (registry, chat2, ledger de comandos, RPC via relay, modo Demo) |
| `zeron-mobile` | — ([`docs/mobile-rewrite.md`](../docs/mobile-rewrite.md)) | Fachada UniFFI do core mobile; build por `scripts/ios/build-core.sh` |

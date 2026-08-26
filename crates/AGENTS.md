# crates — workspace Rust

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

As bibliotecas que compõem o comet. A camada de dependência sobe assim: `proto` (tipos) → `doc` (schema CRDT) → `sync` (transporte Loro) → `harness` (agentes) → `engine` (backend) → `rpc` (fronteira tipada) → `ui`/`mcp` (consumidores). Nada abaixo depende de nada acima.

## Ownership

Todas as crates são internas (`publish = false`) e versionadas juntas pelo `[workspace.package]` da raiz. Adicionar crate exige entrada em `members` **e** em `[workspace.dependencies]` do `Cargo.toml` raiz.

## Local Contracts

- **Versões de dependência moram no `Cargo.toml` da raiz**, não nas crates. Crate filha usa `dep = { workspace = true }`. Não pinar versão local.
- `edition = "2024"` em todas.
- Runtime async é **tokio** em todo lugar; a UI faz a ponte por `gpui_tokio` (`Tokio::spawn` vira `Task` do gpui). A UI nunca bloqueia na engine.
- Bloquear dentro de contexto async é bug, não estilo — já custou findings de review (`rpc.rs`, `repos.rs`).

## Work Guidance

- Mudança que atravessa duas crates quase sempre indica que o tipo está na camada errada: tipo de fio vai pra `proto`, forma de documento vai pra `doc`.
- `update/` é uma crate de arquivo único (auto-update do binário) — não tem doc próprio; edita direto e registra aqui se ganhar seam.

## Verification

- Comandos: `cargo test` · `cargo test -p <crate>` · `cargo fmt --all` · `cargo build -p comet`

| Camada / path | Tier exigido | Como rodar |
|---|---|---|
| `crates/*/src/**` (lógica pura) | unit (`mod tests` co-located) | `cargo test -p <crate>` |
| `crates/{engine,harness,rpc,mcp,doc,sync}/tests/**` | integration | `cargo test -p <crate>` |
| `crates/ui/src/**` (estado, derivações) | unit | `cargo test -p comet-ui` |
| `crates/ui` (render gpui) | none — sem harness de render; validação é visual no `scripts/dev-demo.sh` | — |
| `crates/update` | none — wrapper fino sobre download/replace, coberto no smoke | — |

## Child DOX Index

| Crate | Doc | Papel |
|---|---|---|
| `comet-proto` | [`proto/AGENTS.md`](proto/AGENTS.md) | Tipos de fio + derivações puras compartilhadas |
| `comet-doc` | [`doc/AGENTS.md`](doc/AGENTS.md) | Schemas Loro (session/workspace) + mirror layer |
| `comet-sync` | [`sync/AGENTS.md`](sync/AGENTS.md) | Cliente de room Loro + DocsStore |
| `comet-harness` | [`harness/AGENTS.md`](harness/AGENTS.md) | Adaptadores Claude Code / Codex / mock |
| `comet-engine` | [`engine/AGENTS.md`](engine/AGENTS.md) | Backend: sessões, doc host, repos, terminais, uploads, auth |
| `comet-rpc` | [`rpc/AGENTS.md`](rpc/AGENTS.md) | UiRpc/ControlRpc tipados sobre WS + transporte in-memory |
| `comet-mcp` | [`mcp/AGENTS.md`](mcp/AGENTS.md) | Servidor MCP stdio com as worker tools |
| `comet-ui` | [`ui/AGENTS.md`](ui/AGENTS.md) | App gpui: shell, transcript e export puro, composer/intake e decoração paint-only, terminal, diff |
| `comet-update` | — | Auto-update do binário (arquivo único, sem doc próprio) |

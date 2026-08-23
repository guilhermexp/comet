# Project Context — comet

## Purpose

Controlador multi-device de coding agents (Claude Code / Codex). Cada máquina roda uma **engine** (daemon Rust) que sincroniza sessões via CRDT **Loro** através de **Durable Objects** na Cloudflare: começa o agente num device, acompanha e dirige de outro; um daemon numa máquina always-on mantém o agente rodando com o laptop fechado.

Fork de [zeronsh/comet](https://github.com/zeronsh/comet) (MIT). É um rewrite nativo em Rust + gpui do comet original em TS/Electron. O display de usage combina quotas remotas e totais locais de 24h/7d/30d como snapshots device-local de engine→UI; esses dados não entram no CRDT nem sincronizam pelo edge.

## Tech Stack

- **Rust** edition 2024, workspace `crates/{proto,doc,sync,harness,engine,rpc,mcp,update,ui}` + `apps/comet`.
- **UI**: gpui, fork do Zed (`wingleeio/zed`) pinado por rev. Sem as crates GPL do Zed.
- **Sync**: `loro` 1.13 + `loro-protocol` 0.3.
- **Edge**: TypeScript no Cloudflare Workers — Worker + SessionRoom DO + DeviceRoom DO + R2 + auth WorkOS.
- **Terminal**: `alacritty_terminal` + `portable-pty`. **MCP**: `rmcp` 3.x sobre stdio.
- **iOS**: `apps/ios`, projeto Xcode fora do workspace Cargo.
- Runtime async: tokio em todo lugar; a UI faz ponte por `gpui_tokio`.

## Project Conventions

### Code Style

- `cargo fmt --all` é obrigatório — inclusive **antes** de merge do upstream, senão o merge conflita em ruído de formatação.
- Versões de dependência ficam no `Cargo.toml` raiz; crate filha usa `{ workspace = true }`.
- Nada bloqueante em contexto async: trabalho síncrono vai pra `spawn_blocking` com timeout.
- Campos de tool MCP em camelCase; resto do código em snake_case idiomático.

### Architecture Patterns

- **Camadas não sobem**: `proto` → `doc` → `sync` → `harness` → `engine` → `rpc` → `ui`/`mcp`.
- **Comando é dado durável**, não chamada: send/steer/interrupt/respondInput viram entradas no ledger do session doc, executadas pelo device host do chat.
- **Um protocolo só** para in-process, daemon local e device remoto — o modo in-process roda sobre duplex em memória sem atalho de serialização.
- **Regra derivada compartilhada** entre UI e engine mora em `comet-proto::view`, nunca duplicada nos dois lados.
- **Usage de provider fica fora do CRDT**: `AgentAccountsSnapshot` carrega janelas remotas e linhas locais apenas pelo RPC engine→UI.
- Mapa de onde editar: `AGENTS.md` na raiz e por subárvore (DOX).

### Testing Strategy

- Unit co-located (`mod tests`) em toda crate; integration e e2e em `crates/*/tests/`.
- Edge: vitest (`npm -C edge run test`) + `typecheck` obrigatório.
- Convergência Rust↔edge se prova em `crates/sync/tests/edge_convergence.rs`.
- **Render gpui não tem harness**: mudança visual se valida rodando `scripts/dev-demo.sh` e olhando a tela. Suite verde não é evidência de UI correta.
- O tier exigido por camada é o da **Test Coverage Matrix** do `AGENTS.md` da subárvore tocada — cada `Scenario` carimba `Test:` a partir dela.

### Git Workflow

- Branch `main`; `origin` = fork `guilhermexp/comet`; `upstream` = `zeronsh/comet`.
- **Nunca pushar pro upstream.** `gh` precisa de `-R guilhermexp/comet` (o default resolve pro upstream).
- Push passa pelo gate `no-mistakes` (`git push no-mistakes <branch>`).
- Tag `v*` dispara release no R2; push na `main` tocando `edge/` dispara deploy do Worker. Ambos herdados do upstream — não disparar sem intenção.

## Domain Context

- **Session doc** (por chat): transcript + fila durável de comandos. **Workspace doc** (por org, `ws2/{orgId}`): spaces, chats, devices, status.
- **Space** = par sincronizado device+pasta, unidade de organização do app.
- **Host** de um chat é o device dono; só ele executa os comandos do ledger.
- Mudança de nome ou shape de container CRDT é **destrutiva cross-device** — o `2` em `ws2/` é cicatriz disso.

## Important Constraints

- Dois devices em versões diferentes falam o mesmo fio: mudança de tipo serializado é breaking. Campo novo entra opcional.
- Licença: MIT de terceiro. Preservar atribuição; não puxar crate GPL do Zed.
- Sync com o upstream é frequente (várias versões por semana) — mudança grande e espalhada encarece todo merge seguinte.
- Build do gpui é caro; primeira build leva minutos.

## External Dependencies

- **Cloudflare** — Workers, Durable Objects, R2 (releases em `comet.zeron.sh`).
- **WorkOS** — auth (JWKS, code exchange, refresh), via edge.
- **wingleeio/zed** — fork do gpui pinado por rev; bump exige rebase da branch `comet/line-wrap-closing-punctuation`.
- **Claude Code / Codex** — CLIs externas dirigidas pelos harnesses; mudança de formato de saída delas quebra o parse.

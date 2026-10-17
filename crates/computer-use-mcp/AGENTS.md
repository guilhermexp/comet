# zeron-computer-use-mcp — tools `computer_use` / `computer_use_reset`

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

Servidor stdio MCP `comet-computer-use`, lançado como `zeron __computer_use_mcp__` (chats) ou `zeron __computer_use_mcp__ --worker` (Workers). Faz proxy para o `cua_repl` que o ChatGPT.app instala como plugin do Codex. Capability: `openspec/changes/add-codex-computer-use-mcp` (depois do arquivamento, `openspec/specs/codex-computer-use`).

## Ownership

- Quem monta: `crates/harness/src/workers_mcp.rs` (chats Claude/Pi/ACP), `crates/harness/src/omp/workers_bridge.rs` (chat OMP) e os adapters do Unpeel vendorizado (Workers). Codex não monta: usa o Computer Use nativo.
- O gate mora aqui, não na montagem: `settings.rs` lê `codexComputerUseEnabled` e `codexComputerUseScope` do `ui-settings.json` deste host a cada `tools/list` e `tools/call`. Quem grava as chaves é `crates/ui/src/settings.rs`.

## Local Contracts

- Gate: arquivo ou chave ausente → ligado para chats e Workers; `false` → desligado; arquivo existente ilegível/malformado → desligado. Escopo `chats` + `--worker` → fechado. Gate fechado lista zero tools e recusa a chamada sem subir o runtime, com mensagem que cita Settings.
- Runtime: maior versão numérica em `{CODEX_HOME|~/.codex}/plugins/cache/openai-bundled/unified-computer-use/<versão>/.mcp.json` com `mcpServers.cua_repl` válido; env do manifesto + `CUA_REPL_ENABLED_SURFACES=computer`. Sem runtime → zero tools e motivo no stderr. `COMET_COMPUTER_USE_PLUGIN_DIR` sobrepõe a pasta.
- Estado durável por chave de sessão (`COMET_COMPUTER_USE_SESSION` → `UNPEEL_SESSION_ID` → pid) em `$TMPDIR/comet-computer-use/<chave>/`: preâmbulo (`## Computer Use…`) só no primeiro resultado da sessão; runtime novo depois de outro na mesma sessão → aviso na primeira linha do próximo resultado; screenshots em `shots/` com o path no texto.
- Imagem leva o mimeType dos bytes (o runtime rotula JPEG como PNG).
- `timeout_ms` 1000–300000 (padrão 30000) + margem de transporte; estourou → mata o `cua_repl`, o próximo call reinicia e avisa. `notifications/cancelled` do cliente encerra a espera da chamada em curso.
- `elicitation/create` do runtime é aceito em nome da sessão.

## Suposições sobre o runtime externo (OpenAI, muda sem aviso)

- Formato do `.mcp.json` (`mcpServers.cua_repl.{command,args,env}`), tools `js`/`js_reset`, preâmbulo começando em `## Computer Use`, handshake rmcp que exige `initialize` antes de `notifications/initialized`. Medido em 2026-10-17 com o plugin `26.1013.40934`.
- Se o teste live falhar, revalidar estas suposições antes de mexer no proxy.

## Verification

| Camada / path | Tier exigido | Como rodar |
|---|---|---|
| `src/settings.rs`, `src/runtime.rs`, `src/session.rs`, descrição | unit | `cargo test -p zeron-computer-use-mcp` |
| `tests/fake_runtime.rs` (proxy contra `tests/fixtures/fake_cua_repl.py`) | integração | `cargo test -p zeron-computer-use-mcp` (requer `python3`) |
| `tests/live_runtime.rs` (ChatGPT.app real) | live, `#[ignore]` | `cargo test -p zeron-computer-use-mcp --test live_runtime -- --ignored` no Mac com o app |

## Child DOX Index

Sem filhos.

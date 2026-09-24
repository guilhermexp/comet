# zeron-sessions-mcp — tool `sessions`

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

Servidor stdio MCP `comet-sessions`, lançado como `zeron __sessions_mcp__`. Expõe uma tool `sessions` com `help`, `list_spaces` e `create`.

## Ownership

Não cria Chat sozinho: `create` chama o RPC local `SpawnChat` no endpoint que a engine carimbou. A crate `workers-unpeel` não muda.

## Local Contracts

- Startup exige `COMET_SESSIONS_CONTROLLER=1` e remove do ambiente o marker, o parent chat, o endpoint e o engine id antes de servir.
- Toda action confere `EngineInfo.deviceId` com `COMET_SESSIONS_ENGINE_ID`. Identidade diferente ou engine inalcançável devolve erro e não chama `SpawnChat`.
- `create` recusa prompt vazio e campos de harness/model/reasoning/sandbox. Não há override de modelo.

## Work Guidance

- Intercepto do argv fica em `apps/zeron/src/main.rs`, antes do parse de CLI e do host de workers.

## Verification

| Camada / path | Tier exigido | Como rodar |
|---|---|---|
| `src/lib.rs` (identidade, prompt, overrides, list_spaces, scrub) | unit | `cargo test -p zeron-sessions-mcp` |

## Child DOX Index

Sem filhos.

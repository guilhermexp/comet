# comet-mcp — worker tools sobre MCP

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

Expõe os terminais da engine como ferramentas MCP para o agente que roda dentro do comet: `spawn_worker`, `read_worker`, `wait_worker`, `kill_worker`. Servido por `comet mcp-server` sobre stdio (`rmcp` 3.x, feature `transport-io`), falando com a engine por um client seam — não por acesso direto ao estado.

## Ownership

Dona do contrato das worker tools. Injetada no agente via `--mcp-config` (Claude) e `-c mcp_servers.*` (Codex), sem `--strict`.

## Local Contracts

- Campos das tools são **camelCase** (`workerId`, `afterSeq`, `timeoutMs`) — é o que o cliente MCP espera; snake_case quebra a chamada sem erro claro.
- `rmcp` precisa da feature de servidor ligada; só `transport-io` compila um build client-only que não serve nada.
- O comando do worker é entregue ao shell como **um argumento quotado** (`exec /bin/sh -c '<script>'`), porque `OpenTerminal` sobe um shell de login interativo (ver `../engine/AGENTS.md`).
- Profundidade herdada de worker tem piso pelo marcador de ambiente — sem isso um worker aninhado se acha raiz.

## Work Guidance

- Findings abertos e conhecidos (não resolvidos):
  - o teto de 1 MiB em `read`/`wait` avança `next_seq` além dos eventos descartados, então quem retoma pelo cursor perde o miolo em silêncio; o certo é parar de consumir antes de estourar;
  - `shell_quote` usa aspas simples POSIX, que não são inertes em csh/tcsh quando o comando tem `!`. O robusto exige argv explícito no `OpenTerminal` — mudança de RPC da engine, logo change no OpenSpec.
- Tool nova segue o mesmo seam: lógica sobre o client da engine, nunca sobre o estado interno dela.

## Verification

- Comandos: `cargo test -p comet-mcp`

| Camada / path | Tier exigido | Como rodar |
|---|---|---|
| `src/**` (lógica das tools, quoting) | unit | `cargo test -p comet-mcp` |
| `tests/worker_e2e.rs` | e2e — ciclo de vida contra engine real | `cargo test -p comet-mcp` |
| Handshake stdio com cliente real | e2e manual — `comet mcp-server` + agente configurado | — |

## Child DOX Index

Sem filhos.

# comet-proto — tipos de fio e derivações compartilhadas

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

O vocabulário que todo mundo fala: `AgentEvent` (incluindo `ToolCallPreview` não durável), `ToolCall`, `RunRequest`, `Model`, `FileToolInputSnapshot` sanitizado, entidades, snapshots device-local de usage (`AgentUsageWindow`/`AgentUsageLine`) e envelopes de RPC (serde, framing ndjson). Além dos tipos, o módulo `view` guarda as **derivações puras** que UI e engine precisam concordar — ordenação, gating de staleness, agrupamento, boot gate.

## Ownership

Crate-base do workspace. Não depende de nenhuma outra crate do repo — se você precisou importar algo daqui pra cima, o tipo está no lugar errado.

## Local Contracts

- Todo tipo que cruza processo (UI↔engine, engine↔engine via DeviceRoom, engine↔edge) mora aqui.
- Mudar shape de tipo serializado é **breaking cross-device**: dois devices em versões diferentes falam o mesmo fio. Campo novo entra opcional/`#[serde(default)]`; remoção exige change no OpenSpec.
- `view` é puro: sem I/O, sem tokio, sem gpui. É o que permite testar a regra sem subir engine nem janela.
- Tipos de usage são compatíveis por serde e cruzam apenas engine↔UI; não são persistidos em Loro nem sincronizados pelo edge.

## Work Guidance

- Lógica de apresentação que a UI e a engine derivam do mesmo estado pertence a `view`, não a `comet-ui` — duplicar ali é como o comportamento diverge entre headed e headless.

## Verification

- Comandos: `cargo test -p comet-proto`

| Camada / path | Tier exigido | Como rodar |
|---|---|---|
| `src/view` (derivações puras) | unit | `cargo test -p comet-proto` |
| `src/**` (tipos serde) | unit — roundtrip de serialização quando o shape tem regra | `cargo test -p comet-proto` |

## Child DOX Index

Sem filhos.

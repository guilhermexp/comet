# comet-rpc — a fronteira tipada

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

`UiRpc` e `ControlRpc`: request/response/stream tipados sobre WebSocket (`tokio-tungstenite`), mais um transporte in-memory e os sockets virtuais do device room (frames `{s,k,to,from}`).

## Ownership

Dona da fronteira UI↔engine. É o que mantém honesto o modo in-process: mesmo protocolo, sem atalho de serialização, rodando sobre um duplex em memória.

## Local Contracts

- **Um protocolo só** para in-process, daemon local e device remoto. Atalho que só existe no modo in-process quebra headless silenciosamente.
- Frame do device room é o envelope de relay — método novo que precisa ser dirigível de outro device tem que ser relay-forwardable.
- `FetchToolInput` é unary e relay-forwardable ao device dono; a engine valida ownership do chat antes de ler o journal local.
- Handler é async e não bloqueia: enumerar path, ler arquivo e afins vão pra `spawn_blocking`.

## Work Guidance

- RPC novo = tipo em `comet-proto` + método aqui + handler na engine. Os três no mesmo commit, senão a UI compila contra um contrato que não existe.

## Verification

- Comandos: `cargo test -p comet-rpc`

| Camada / path | Tier exigido | Como rodar |
|---|---|---|
| `src/**` (envelopes, transporte) | unit | `cargo test -p comet-rpc` |
| `tests/device_room.rs` | integration — roteamento de socket virtual | `cargo test -p comet-rpc` |

## Child DOX Index

Sem filhos.

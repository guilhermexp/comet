# zeron-rpc — a fronteira tipada

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
- No IPC local, `ProtocolError::HandshakeIncomplete` significa que o peer TCP saiu antes do upgrade e fica em debug; handshakes completos inválidos, `Origin` de browser e demais falhas continuam em warning.
- `LinkCache::new` instala o watcher de credenciais antes de retornar; sign-out não pode perder a primeira versão do `watch` nem manter sockets autenticados em cache.
- `WatchTrajectory` e `RevealTrajectoryRaw` são métodos estritamente device-local (IPC local apenas; nunca relay-forwarded). Cursors e watermarks de Trajectory utilizam a tupla completa `(source_seq, sub_seq)` para garantir desambiguação exata.

## Work Guidance

- RPC novo = tipo em `zeron-proto` + método aqui + handler na engine. Os três no mesmo commit, senão a UI compila contra um contrato que não existe.

## Verification

- Comandos: `cargo test -p zeron-rpc`

| Camada / path | Tier exigido | Como rodar |
|---|---|---|
| `src/**` (envelopes, transporte) | unit | `cargo test -p zeron-rpc` |
| `src/lib.rs` (Trajectory wire contracts, cursor ordering, params/items serde) | unit | `cargo test -p zeron-rpc trajectory` |
| `src/server.rs` (classificação do handshake IPC) | unit | `cargo test -p zeron-rpc server::tests::only_an_incomplete_websocket_handshake_is_benign -- --exact` |
| `tests/device_room.rs` | integration — roteamento de socket virtual | `cargo test -p zeron-rpc` |
| `tests/device_room.rs` (revogação de credencial) | integration | `cargo test -p zeron-rpc --test device_room sign_out_closes_cached_peer_links -- --exact` |

## Child DOX Index

Sem filhos.

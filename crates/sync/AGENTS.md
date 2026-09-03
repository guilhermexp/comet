# zeron-sync — cliente de room Loro e persistência local

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

Como o estado **viaja e persiste**: cliente de room sobre `loro-protocol` (join, backfill por version vector, fragmentos, backoff), presença efêmera via `EphemeralStore`, e o `DocsStore` — snapshots em SQLite mais o ledger de comandos já processados.

## Ownership

Único ponto que fala o protocolo de room com os Durable Objects. Se `zeron-doc` diz *qual* é a forma, aqui é *como* ela chega e volta.

## Local Contracts

- O par de crates `loro` / `loro-protocol` é o twin Rust do pacote npm que o edge fala — frames byte-idênticos. Bump de versão exige revalidar a convergência contra o edge, não só compilar.
- Join é **supervisionado**: falha de sync não pode ficar silenciosa. Retry, probe e escalonamento existem porque o modo de falha real era "trava sem dizer nada".
- Presença é efêmera por design — substitui escrita de heartbeat a cada 15s. Não persistir presença no doc.

## Work Guidance

- Bug de "device sumiu" / "não converge": comece pelo `tests/registry_edge.rs`, que roda contra o edge real, antes de suspeitar do schema.

## Verification

- Comandos: `cargo test -p zeron-sync --features mock-server` — a feature **não** é implícita: `tests/registry_client.rs` importa `zeron_sync::registry::mock_server`, então a invocação sem ela nem compila.

| Camada / path | Tier exigido | Como rodar |
|---|---|---|
| `src/**` (backoff, VV, DocsStore) | unit | `cargo test -p zeron-sync --features mock-server --lib` |
| `tests/registry_client.rs` | integration — cliente contra o DO mock in-process | `cargo test -p zeron-sync --features mock-server --test registry_client` |
| `tests/registry_edge.rs` | e2e, `--ignored` por padrão; precisa de `wrangler dev` + `AUTH_MODE=dev` | `ZERON_EDGE_WS=ws://127.0.0.1:27640 cargo test -p zeron-sync --test registry_edge -- --ignored` |

## Child DOX Index

Sem filhos.

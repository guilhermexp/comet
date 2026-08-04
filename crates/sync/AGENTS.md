# comet-sync — cliente de room Loro e persistência local

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

Como o estado **viaja e persiste**: cliente de room sobre `loro-protocol` (join, backfill por version vector, fragmentos, backoff), presença efêmera via `EphemeralStore`, e o `DocsStore` — snapshots em SQLite mais o ledger de comandos já processados.

## Ownership

Único ponto que fala o protocolo de room com os Durable Objects. Se `comet-doc` diz *qual* é a forma, aqui é *como* ela chega e volta.

## Local Contracts

- O par de crates `loro` / `loro-protocol` é o twin Rust do pacote npm que o edge fala — frames byte-idênticos. Bump de versão exige revalidar a convergência contra o edge, não só compilar.
- Join é **supervisionado**: falha de sync não pode ficar silenciosa. Retry, probe e escalonamento existem porque o modo de falha real era "trava sem dizer nada".
- Presença é efêmera por design — substitui escrita de heartbeat a cada 15s. Não persistir presença no doc.

## Work Guidance

- Bug de "device sumiu" / "não converge": comece pelo `edge_convergence.rs`, que roda contra o edge real, antes de suspeitar do schema.

## Verification

- Comandos: `cargo test -p comet-sync`

| Camada / path | Tier exigido | Como rodar |
|---|---|---|
| `src/**` (backoff, VV, DocsStore) | unit | `cargo test -p comet-sync` |
| `tests/edge_convergence.rs` | integration — convergência real contra o edge | `cargo test -p comet-sync` |

## Child DOX Index

Sem filhos.

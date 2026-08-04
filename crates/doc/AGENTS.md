# comet-doc — schemas Loro e mirror layer

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

Como o estado sincronizado é **formado**: o schema do **session doc** (por chat — transcript + fila durável de comandos) e do **workspace doc** (por org — spaces, chats, devices, status), o fold de parts, o split de continuação, o command ledger e os sidecars. Junto vem o **mirror layer**: aplicação *incremental* dos diffs de `doc.subscribe` num estado tipado em cache, sem re-hidratar o doc inteiro a cada mudança.

## Ownership

Dona do formato dos documentos CRDT. O edge (TypeScript) materializa o mesmo shape — nome e forma de container aqui e em `edge/src/session-doc/` são **um contrato só**.

## Local Contracts

- Corpo de mensagem é **LoroText**, nunca reescrita LWW de valor — é a forma medida em 1.03× de oplog. Trocar isso multiplica o histórico.
- Command ledger segue as regras 1–3: entradas append-only por device; outcome só do host; dedupe/TTL/supersede avaliados na leitura.
- Constantes carregadas do comet original (`STREAM_COMMIT_MS=120`, `DO_FLUSH_MS=5s`, compactação em 8MB, retenção 30d, tail 64) são compatibilidade, não preferência — mudar exige olhar o lado do edge.
- Split de continuação em 256KB. Tool parts renderizáveis vão pro doc; inputs completos ficam no run journal local do host.
- Mudança de nome/shape de container é **destrutiva cross-device** (foi o motivo do `2` em `ws2/{orgId}`). Vai por OpenSpec.

## Work Guidance

- Bug de "a UI não atualizou" quase sempre é diff não aplicado no mirror, não render — comece por aqui antes de `comet-ui`.
- Mudou schema? Confira o materializador gêmeo em `edge/src/session-doc/` no mesmo commit.

## Verification

- Comandos: `cargo test -p comet-doc`

| Camada / path | Tier exigido | Como rodar |
|---|---|---|
| `src/**` (fold, ledger, mirror) | unit | `cargo test -p comet-doc` |
| `tests/attachments_roundtrip.rs` | integration | `cargo test -p comet-doc` |
| Interop de shape com o edge | integration — pelo `crates/sync/tests/edge_convergence.rs` | `cargo test -p comet-sync` |

## Child DOX Index

Sem filhos.

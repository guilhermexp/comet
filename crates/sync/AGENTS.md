# zeron-sync — cliente de room Loro e persistência local

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

Como o estado **viaja e persiste**: cliente de room sobre `loro-protocol` (join, backfill por version vector, fragmentos, backoff), presença efêmera via `EphemeralStore`, e o `DocsStore` — snapshots em SQLite mais o ledger de comandos já processados.

## Ownership

Único ponto que fala o protocolo de room com os Durable Objects. Se `zeron-doc` diz *qual* é a forma, aqui é *como* ela chega e volta.

## Local Contracts

- O par de crates `loro` / `loro-protocol` é o twin Rust do pacote npm que o edge fala — frames byte-idênticos. Bump de versão exige revalidar a convergência contra o edge, não só compilar.
- Join é **supervisionado**: falha de sync não pode ficar silenciosa. Retry, probe e escalonamento existem porque o modo de falha real era "trava sem dizer nada".
- O semáforo de processo `MAX_CONCURRENT_DIALS` limita handshakes WS e ciclos de fallback HTTPS chat2. No WS, a espera observa shutdown e revalida cancelamento antes de conectar; o permit é liberado ao terminar o handshake, antes de tratar sucesso, erro ou backoff. Socket estabelecido não segura slot. O fallback HTTPS segura o permit durante seu ciclo finito de push/pull. Implementação e regressões: `src/chat_client.rs` e `src/chat_client/tests.rs`.
- Presença é efêmera por design — substitui escrita de heartbeat a cada 15s. Não persistir presença no doc.
- Chat row import distingue operações aplicadas de dependências causais pendentes. Row pendente segura o cursor e força checkpoint mesmo com frontier aparentemente contida; HTTP e WebSocket usam a mesma regra. Reparo inclui rows próprias e uma geração impede que catch-up antigo limpe um gap mais novo. `CaughtUp` não é emitido enquanto faltar história causal.

## Work Guidance

- Bug de "device sumiu" / "não converge": comece pelo `tests/registry_edge.rs`, que roda contra o edge real, antes de suspeitar do schema.

## Verification

- Testes async de ChatClient compartilham a trava `PATH_AND_TIMING`: o estado de rede e o orçamento de dials são globais; um runtime não pode reter permits enquanto outro avança seu relógio virtual.

- Comandos: `cargo test -p zeron-sync --features mock-server` — a feature **não** é implícita: `tests/registry_client.rs` importa `zeron_sync::registry::mock_server`, então a invocação sem ela nem compila.

| Camada / path | Tier exigido | Como rodar |
|---|---|---|
| `src/chat_client.rs` + `src/chat_client/tests.rs` (cursor, checkpoint, recuperação causal, cap de dials) | unit | `cargo test -p zeron-sync --features mock-server --lib chat_client` |
| `src/**` (backoff, VV, DocsStore) | unit | `cargo test -p zeron-sync --features mock-server --lib` |
| `tests/registry_client.rs` | integration — cliente contra o DO mock in-process | `cargo test -p zeron-sync --features mock-server --test registry_client` |
| `tests/registry_edge.rs` | e2e, `--ignored` por padrão; precisa de `wrangler dev` + `AUTH_MODE=dev` | `ZERON_EDGE_WS=ws://127.0.0.1:27640 cargo test -p zeron-sync --test registry_edge -- --ignored` |

## Child DOX Index

Sem filhos.

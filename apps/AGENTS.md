# apps — binário e clientes

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

Os executáveis. `apps/comet` é o binário único (headed por padrão, `headless` como subcomando) e a superfície de CLI: auth, update, daemon. `apps/ios` é o cliente iOS, projeto Xcode, fora do workspace Cargo.

## Ownership

`apps/comet` é casca: parse de argumento, escolha de modo, wiring. Comportamento mora nas crates. Lógica que apareceu em `main.rs` provavelmente pertence a `comet-engine` ou `comet-ui`.

## Local Contracts

- **Modo headed**: se já existe daemon escutando na porta IPC, conecta nele; senão roda a engine **in-process** (RPC sobre duplex em memória — mesmo protocolo) **e serve essa engine na porta IPC**. A engine embutida não é privada: outro viewport pode se anexar ao app rodando.
- Bind da porta é best-effort: porta ocupada não impede a janela de abrir, só perde a capacidade de hospedar peers.
- **Modo headless**: só engine; imprime URL de sign-in no TTY (fluxo de paste-code), serve IPC em localhost e hospeda o próprio DeviceRoom.
- Subcomandos vivem em arquivos separados (`auth_cli.rs`, `update_cli.rs`, `daemon.rs`) — `main.rs` só despacha.
- `apps/ios` não entra no `cargo build`; build e teste são pelo Xcode.

## Work Guidance

- Subcomando novo = arquivo novo + uma linha de dispatch. Não engordar `main.rs`.

## Verification

- Comandos: `cargo build -p comet` · `scripts/e2e-smoke.sh`

| Camada / path | Tier exigido | Como rodar |
|---|---|---|
| `comet/src/**` (wiring, dispatch) | none — casca fina; o comportamento é testado nas crates | `cargo build -p comet` |
| Fluxo headed/headless real | e2e | `scripts/e2e-smoke.sh` · `scripts/dev-demo.sh` |
| `ios/**` | none — sem suite; validação manual no Xcode | Xcode |

## Child DOX Index

Sem filhos.

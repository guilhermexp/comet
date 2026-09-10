# apps — binário e clientes

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

Os executáveis. `apps/zeron` é o binário único (headed por padrão, `headless` como subcomando) e a superfície de CLI: auth, update, daemon. `apps/ios` é o cliente iOS, projeto Xcode, fora do workspace Cargo.

## Ownership

`apps/zeron` é casca: parse de argumento, escolha de modo, wiring. Comportamento mora nas crates. Lógica que apareceu em `main.rs` provavelmente pertence a `zeron-engine` ou `zeron-ui`.

## Local Contracts

- A feature Cargo `browser-fixture` de `zeron` expõe os binários opt-in `browser-fixture` e `preview-fixture` em `zeron/src/fixtures/`. Não empacotar nem publicar. O modo `browser-fixture <output> --resize-regression` verifica hit testing do divisor e geometria fracionária sem gravação de tela; sucesso exige `result.txt` com PASS (o lifecycle GPUI pode retornar exit 0 mesmo após erro do fixture). Dev deps de `zeron-ui` ativam `gpui/test-support`, cujo flush desenha sem apresentação nativa; por isso esses rigs são binários do app, nunca exemplos/test targets da UI. Build nativa deve selecionar só `-p zeron --bin <fixture>`, sem `--workspace` que unificaria as features de teste.


- **Modo headed**: se já existe daemon escutando na porta IPC, conecta nele; senão roda a engine **in-process** (RPC sobre duplex em memória — mesmo protocolo) **e serve essa engine na porta IPC**. A engine embutida não é privada: outro viewport pode se anexar ao app rodando.
- Bind da porta é best-effort: porta ocupada não impede a janela de abrir, só perde a capacidade de hospedar peers.
- **Modo headless**: só engine; imprime URL de sign-in no TTY (fluxo de paste-code), serve IPC em localhost e hospeda o próprio DeviceRoom.
- Subcomandos vivem em arquivos separados (`auth_cli.rs`, `update_cli.rs`, `daemon.rs`) — `main.rs` só despacha.
- O allocator customizado do binário é mimalloc v2 apenas no macOS; Linux e demais plataformas mantêm o allocator de sistema.
- No iOS, `NativeTranscriptTable` preserva identidade/posição de células e aplica keyboard inset com a animação nativa. `SessionStore.lastSubmittedMessageId` distingue envio local de entradas remotas; folding de mensagem vive no store quente. `ComposerEditorController` confirma IME antes do envio e aplica o draft resultante imediatamente. Disclosure de tools é local à célula, sem reconfigurar todo o transcript.
- `apps/ios` não entra no `cargo build`; build e teste são pelo Xcode.

## Work Guidance

- Subcomando novo = arquivo novo + uma linha de dispatch. Não engordar `main.rs`.

## Verification

- Comandos: `cargo build -p zeron` · `scripts/e2e-smoke.sh`

| Camada / path | Tier exigido | Como rodar |
|---|---|---|
| `zeron/src/**` (wiring, dispatch) | none — casca fina; o comportamento é testado nas crates | `cargo build -p zeron` |
| `ios/ZeronTests/{TranscriptFollow,TranscriptLayout,TranscriptPresentation,ComposerEditor}Tests.swift` e `ios/ZeronUITests/MobilePolishTests.swift` | unit / integration / e2e — UIKit real; exige Xcode + simulador | `xcodebuild test -project apps/ios/Zeron.xcodeproj -scheme Zeron -destination 'platform=iOS Simulator,name=iPhone 17 Pro'` |
| Fluxo headed/headless real | e2e | `scripts/e2e-smoke.sh` · `scripts/dev-demo.sh` |
| `ios/**` (`apps/ios/ZeronTests/`) | unit / integration (XCTest: tracking de PRs/checkout, RPC/stream de device relay, gates de versão/resiliência de rede, wire layout de chat frames, merge/conformance de registry e persistência/HLC de RegistryDoc) | `xcodebuild test -project apps/ios/Zeron.xcodeproj -scheme Zeron -destination 'platform=iOS Simulator,name=iPhone 17 Pro'` ou Product → Test no Xcode |

## Child DOX Index

Sem filhos.

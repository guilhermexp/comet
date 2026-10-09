# apps — binário e clientes

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

Os executáveis. `apps/zeron` é o binário único (headed por padrão, `headless` como subcomando) e a superfície de CLI: auth, update, daemon. `apps/ios` é o cliente iOS, projeto Xcode fora do workspace Cargo: UIKit sobre o core Rust `crates/mobile` (desde o sync v0.2.94, que substituiu o app SwiftUI do fork; adaptações antigas de OMP/streaming precisam ser refeitas no app novo). A fase de build do Xcode roda `scripts/ios/build-core.sh`.

## Ownership

`apps/zeron` é casca: parse de argumento, escolha de modo, wiring. Comportamento mora nas crates. Lógica que apareceu em `main.rs` provavelmente pertence a `zeron-engine` ou `zeron-ui`.

## Local Contracts

- No Linux, `zeron appshot` é somente ativação IPC local para o app headed em execução; captura/permissões/destino pertencem à UI. Não inicia uma engine nem captura no daemon.

- A feature Cargo `browser-fixture` de `zeron` expõe os binários opt-in `browser-fixture` e `preview-fixture` em `zeron/src/fixtures/`. Não empacotar nem publicar. O modo `browser-fixture <output> --resize-regression` verifica hit testing do divisor e geometria fracionária sem gravação de tela; sucesso exige `result.txt` com PASS (o lifecycle GPUI pode retornar exit 0 mesmo após erro do fixture). Dev deps de `zeron-ui` ativam `gpui/test-support`, cujo flush desenha sem apresentação nativa; por isso esses rigs são binários do app, nunca exemplos/test targets da UI. Build nativa deve selecionar só `-p zeron --bin <fixture>`, sem `--workspace` que unificaria as features de teste.


- `main` chama `zeron_engine::raise_nofile_limit` depois do tracing e **antes** de `run_app` / headless — o gpui/Metal precisa abrir `default.metallib` mesmo com dezenas de rooms.
- **Modo headed**: se já existe daemon escutando na porta IPC, conecta nele; senão roda a engine **in-process** (RPC sobre duplex em memória — mesmo protocolo) **e serve essa engine na porta IPC**. A engine embutida não é privada: outro viewport pode se anexar ao app rodando.
- Bind da porta é best-effort: porta ocupada não impede a janela de abrir, só perde a capacidade de hospedar peers.
- **Modo headless**: só engine; imprime URL de sign-in no TTY (fluxo de paste-code), serve IPC em localhost e hospeda o próprio DeviceRoom.
- Subcomandos vivem em arquivos separados (`auth_cli.rs`, `update_cli.rs`, `daemon.rs`) — `main.rs` só despacha.
- `zeron __sessions_mcp__` é interceptado em `main` antes do parse de CLI e do host de workers, e delega para `zeron-sessions-mcp`. Argumento extra recusa com usage. Não é subcomando clap.
- O allocator customizado do binário é mimalloc v2 apenas no macOS; Linux e demais plataformas mantêm o allocator de sistema.
- No iOS, `NativeTranscriptTable` preserva identidade/posição de células e aplica keyboard inset com a animação nativa. `SessionStore.lastSubmittedMessageId` distingue envio local de entradas remotas; folding de mensagem vive no store quente. `ComposerEditorController` confirma IME antes do envio e aplica o draft resultante imediatamente. Disclosure de tools é local à célula, sem reconfigurar todo o transcript.
- Imagens geradas no transcript carregam apenas `path/name/mimeType` no ChatDoc. O iOS valida os quatro MIME raster suportados, tenta o device dono antes do host e decodifica uma única frame com limite de bytes e dimensões antes de medir/renderizar; cache genérico e cache com MIME esperado são chaves distintas.
- Appshots no iOS preservam o screenshot como anexo e projetam no transcript apenas origem/título; o texto AX observado fica oculto. O parser escolhe o último trailer de anexos válido e ignora marcadores que apareçam dentro do XML observado.
- Voz Codex no iOS usa o estado compartilhado de `crates/voice-session` e media nativa via WebRTC/CoreAudio, com Live Activities/Dynamic Island; permanece cliente da engine host do Chat. Falha ao abrir Chat real retorna à navegação real com erro, nunca a fixture/demo. O Live OMP mantém seu contrato independente na engine.
- `apps/zeron/build.rs` inclui a declaração de microfone de `Info-unbundled.plist` no Mach-O para launches sem bundle; o runner de dev continua usando identidade própria de Zeron Dev.
- `apps/ios` não entra no `cargo build`; build e teste são pelo Xcode.

## Work Guidance

- Subcomando novo = arquivo novo + uma linha de dispatch. Não engordar `main.rs`.

## Verification

- Comandos: `cargo build -p zeron` · `scripts/e2e-smoke.sh`

| Camada / path | Tier exigido | Como rodar |
|---|---|---|
| `zeron/src/**` (wiring, dispatch) | none — casca fina; o comportamento é testado nas crates | `cargo build -p zeron` |
| Fluxo headed/headless real | e2e | `scripts/e2e-smoke.sh` · `scripts/dev-demo.sh` |
| `ios/**` | unit / integration (XCTest do app UIKit; core Rust coberto por `cargo test -p zeron-mobile -p zeron-client -p zeron-text`) | `xcodebuild test -project apps/ios/Zeron.xcodeproj -scheme Zeron -destination 'platform=iOS Simulator,name=iPhone 17 Pro'` |

## Child DOX Index

| Domínio | Doc | Papel |
|---|---|---|
| Cliente iOS | [`ios/AGENTS.md`](ios/AGENTS.md) | UIKit, core UniFFI, Chat, media nativa e Live Activities |

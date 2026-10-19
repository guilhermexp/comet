# apps/ios — cliente UIKit

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

Cliente iOS do Comet sobre `zeron-mobile`. O telefone é um peer que acompanha
Chats em hosts remotos; não executa agentes. Rust decide o conteúdo e a
geometria do transcript, e UIKit pinta as frames, navega e recebe gestos.

## Ownership

`Zeron/` contém o app UIKit, o bridge UniFFI gerado e as surfaces de sessão,
composer e voz. `ZeronShared/` fornece tipos e intents compartilhados com
`ZeronLiveActivity/`, a extensão de Dynamic Island e Lock Screen. A mídia local
WebRTC e o lifecycle remoto ficam em `Zeron/Voice/`.

## Local Contracts

- Uma falha ao abrir um Chat real registra o erro, mantém o usuário na
  navegação real de Sessions e mostra uma falha visível. Nunca substitua o Chat
  solicitado por `FixtureSessionSource`; fixtures pertencem ao lab e aos
  fixtures explícitos.
- Codex Voice conecta ao host do Chat por `RemoteVoiceController` e
  `NativeVoiceMedia`. Trocar de tela não encerra a chamada; mute, encerramento,
  falha e shutdown fecham a mídia local. O indicador da chamada permite
  reabrir o palco a partir do transcript; Settings mostra a voz selecionada.
  O preview DEBUG usa um Chat seeded explícito, sem fallback na rota real.
- Voice Activity reflete estado efêmero, oferece mute/end e é encerrada ao
  encerrar a chamada ou ao iniciar o app com estado antigo.
- Ao ir para background após um envio recente ou ainda pendente, o app solicita
  uma `UIApplication` background task de até 25 segundos (ou o prazo menor do
  sistema) para concluir a entrega já persistida. Ela termina ao voltar ao
  foreground ou sair da conta. O recibo durável retoma no próximo launch; force
  quit ou perda de rede antes do Edge aceitar os bytes ainda exige reabrir o
  app para transmitir.
- `Core/Generated/zeron_core.swift` é gerado a partir do core Rust. Não editar à
  mão; preserve os avisos de licença de WebRTC e Thinking Orbs.
- Sessions oferece `Group by Project`, preferência local do telefone compartilhada
  pelo menu da lista e Settings. Somente Chats fora das seções são agrupados;
  clones/checkouts do mesmo repositório usam a identidade de projeto e a ordem
  recente permanece. Chats sem Project formam um grupo por device.

- SplitRoot adapts between compact tabs and regular split presentation while retaining navigation/fold state. Transcript/composer geometry uses the actual uneven safe areas and readable width. Section drag moves Chats across sections/pins with neighbor anchors. Native `VisualView` consumes Rust-owned visual display lists; math faces resolve from the shared bundled table. These paths require Simulator/device interaction tests, including fold/unfold with keyboard and selection.

## Work Guidance

Mudança de API UniFFI exige regenerar as bindings pelo fluxo de
`scripts/ios/build-core.sh` para os alvos iOS e simulator. Mudança Swift isolada
pode usar `ZERON_SKIP_CORE=1`. Não use chamadas de serviço pagas como prova de
testes offline de voz.

## Verification

| Camada / path | Tier | Como rodar |
|---|---|---|
| `ZeronTests/**` | unit / integration | `xcodebuild test -project apps/ios/Zeron.xcodeproj -scheme Zeron -destination 'platform=iOS Simulator,id=<simulador instalado>'` |
| `ZeronUITests/SessionFlowTests.swift` | e2e | Mesmo comando com `-only-testing:ZeronUITests/SessionFlowTests` |
| `ZeronUITests/VoiceFlowTests.swift` | e2e | Mesmo comando com `-only-testing:ZeronUITests/VoiceFlowTests` |
| Microfone, peer remoto, palco visual e Live Activity em dispositivo real | none — requer hardware e serviço de voz reais | — |

## Child DOX Index

Sem filhos.

Chat drag sources use row identity plus movable/archive/search policy, independently of destination policy. Project groups remain disallowed destinations but their Chat rows can be dragged to Pinned/custom sections/Recent. `ZeronTests/SessionDragTests.swift` is a UIKit unit regression for the policy used by the actual drag delegate; execute it on an iOS Simulator, never infer native acceptance from Linux source review.

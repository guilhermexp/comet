# Tasks

**Goal:** entregar V1, V2 e V3 da proposta sem alterar o runtime da chamada.

**Architecture:** um único `VoiceController`; visibilidade efetiva de Details derivada do stage; uma preferência de UI; um widget global no Shell. Um único Worker é dono da implementação destes arquivos compartilhados.

**Tech Stack:** Rust, GPUI existente e store device-local de UI, sem novas dependências.

**Spec:** `specs/codex-live-voice/spec.md`; decisões e evidências em `design.md`.

**Execução:** após aprovação, seguir a skill `implement` do harness e TDD; não iniciar implementação com esta proposta ainda pendente. Usar uma única rodada de compilação coordenada por vez. O contrato do Worker define o dono da verificação; não disparar Cargo em paralelo entre checkouts.

## Global Constraints

- Desktop somente. Não alterar engine, grants, providers, protocolo, helper de voz, ditado ou clientes mobile.
- `voiceOrbEnabled` é independente de Reduce motion, persiste localmente e tem default `true` para configurações existentes.
- Desligado significa ausência do orb e de seus ticks, não um frame estático ou alpha zero.
- Não mudar o Chat selecionado para mostrar/controlar a chamada; nenhum controle novo de texto ou saída de áudio.
- Não iniciar outra cópia completa do app, engine ou providers para QA; respeitar `scripts/AGENTS.md:53` e capturar somente componentes reais em fixture nativa isolada.
- Sem push, nova tag, publicação ou instalação. Não reverter mudanças alheias nem apagar `.tmp` inteiro.

## Review Focus

- **R1:** false persistido deve sobreviver a alteração de largura e save posterior do Shell (grupo 2).
- **R2:** navegação com panes nativos, janela estreita e modais não pode esconder o widget nem bloquear conteúdo alheio (grupo 3 e smoke integrado).
- **R3:** hover estacionário, foco e clique em encerrar não podem causar flicker, deslocamento ou retorno à chamada (grupo 3).
- **R4:** orb desligado, sidebar recolhida e stage fechado não podem manter ticks de entidades invisíveis (grupos 2 e 3).
- **R5:** fixtures nativas não podem inicializar Shell, engine ou providers; provas de componente não equivalem a uma chamada real (grupo 4).

## 1. V1 — Stage sem Details

Arquivos: `crates/ui/src/shell.rs`, `crates/ui/src/shell/voice_stage.rs`; testes GPUI existentes no Shell e, se necessário, `shell/panel_interaction_tests.rs`. Contrato: `crates/ui/AGENTS.md`.

- [x] 1.1 Adicionar e executar a regressão `voice_stage_excludes_details_without_saving_preferences`: abrir Details, abrir stage em tema glass/opaco, verificar ausência de corpo/header/alvos de resize, fechar stage e verificar restauração de visibilidade/largura e contexto do Chat atual. Registrar falha de comportamento antes da correção, não apenas erro de compilação.
- [x] 1.2 Corrigir a composição e a visibilidade efetiva sem salvar fechamento ou nova largura do Details. Cobrir entrada/saída do stage e rodar a regressão de 1.1 mais os testes existentes de navegação e painéis; aceite: nenhum Details sob o stage e preferências intactas.
- [x] 1.3 Atualizar o contrato de composição no `crates/ui/AGENTS.md`, incluindo a distinção entre ocultação efetiva durante voz e preferência persistida; conferir que descreve o comportamento observado em 1.2.

## 2. V2 — Desligamento exclusivo do orb

Arquivos: `crates/ui/src/settings.rs`, `crates/ui/src/dictation/model.rs`, `crates/ui/src/shell/voice_stage.rs` e sincronização em `shell.rs` conforme o store atual. Reutilizar `Orb::set_visible`; não alterar o motor genérico de orbs.

- [x] 2.1 Adicionar regressões `voice_orb_preference_survives_shell_save` e `voice_orb_disabled_removes_visuals_without_ending_call`: carregar JSON com `voiceOrbEnabled: false`, preservar false após outra alteração de Shell/reload, manter preferências independentes, retirar a área do orb do stage e conservar estado/controles da chamada. Exercitar arquivo legado sem a chave e reativação durante a chamada. Registrar o comportamento red relevante antes da implementação.
- [x] 2.2 Implementar `UiSettings.voice_orb_enabled: bool`/`voiceOrbEnabled`, default true, e o switch `Show voice orb` em Settings → Voice pelo store canônico. Aplicar imediatamente às superfícies e parar ticks retidos quando ocultas, inclusive ao recolher a sidebar. Aceite: testes de 2.1 verdes, sem mudança de Reduce motion, áudio ou ditado.
- [x] 2.3 Sincronizar o contrato da preferência e dos saves no `crates/ui/AGENTS.md`; verificar nomes e default contra a serialização real, sem criar documentação paralela.

## 3. V3 — Widget de chamada fora do stage

Arquivos: `crates/ui/src/shell/voice_stage.rs`, `crates/ui/src/shell.rs` e testes GPUI de voz do Shell. Interface privada: `render_voice_widget(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement>`, usando os handlers existentes de mudo, abertura do stage e encerramento.

- [x] 3.1 Adicionar regressões `voice_widget_follows_live_call_across_routes`, `voice_widget_mute_and_return_preserve_call` e `voice_widget_end_does_not_reopen_stage`: navegar por dois Chats, Workers e Settings, colapsar sidebar, voltar ao stage e encerrar/conectar/falhar. Verificar uma única chamada, seleção intacta, mudo refletido no stage e ausência do widget após término. Substituir os testes do orb de composer cujo contrato deixa de existir, sem manter pins obsoletos.
- [x] 3.2 Implementar o widget global no canto inferior direito com margem de 16 px, ações estáveis e handlers existentes. Remover `render_voice_composer_orb`, sua entidade/call site e qualquer caminho antigo redundante. Quando o orb estiver desligado, oferecer retorno estático identificado, sem animador. Aceite: regressões de 3.1 verdes e nenhum controle de texto/alto-falante novo.
- [x] 3.3 Adicionar e executar `voice_widget_hover_and_focus_keep_targets_stable` e `voice_widget_does_not_block_other_surfaces`: hover parado, foco de teclado, encerrar sem propagação, bounds dentro da janela estreita/larga e precedência de diálogo. Mudo fica indisponível sem estado/controlador válido de áudio, mas cancelar conexão continua possível. Verificar ausência de flicker e de hitbox de janela inteira.
- [x] 3.4 Atualizar `crates/ui/AGENTS.md` e sua matriz de cobertura para o widget em vez do orb de composer; registrar V1–V3 no `fork_changelog.md` com apenas provas efetivamente obtidas.

## 4. Prova integrada e revisão

- [x] 4.1 Criar `crates/ui/examples/desktop-corrections-fixture.rs` usando a convenção `appshots-fixture` e os presenters de produção dos componentes, com estado sintético e sem inicializar Shell/engine/providers. Fazer somente a extração mínima de apresentação necessária para reutilizar o código real. Capturar stage com/sem orb, switch, widget normal/mudo/hover/foco em glass/opaco e janelas estreita/larga; verificar convivência com preview nativo. Capturas e recibos em `.tmp/evidence/desktop-corrections/mac-native-final/` e `mac-native-qa-final/`, distinguindo limites de aceite no app completo.
  - Rodada 2 (MR3, 2026-11-01): o desvio anterior (Shell de produção com `WorkersModel::detached`) foi removido. A fixture fica em `crates/ui/examples/desktop-corrections-fixture.rs` (nome mantido porque também cobre C1 e o MR9) e não inicializa Shell, `WorkersModel`, engine nem providers. A extração mínima é `crates/ui/src/voice_surfaces.rs` (stage, barra de chamada, widget, posicionamento/faixa nativa e linha `Show voice orb`), e o Shell e Settings → Voice renderizam por esses mesmos presenters. A fixture monta o Composer real sobre `AppState` sintético com o Todo real, e um `BrowserSurface` real em contexto efêmero com página loopback servida pelo próprio processo. A sobreposição só é verificada depois de `fixture_native_visible`. Cobre stage com e sem orb, a linha de preferência nos dois estados, o widget normal/mudo/hover/foco, o card colado no composer sob o Todo, um diálogo sobre o widget, o Browser largo (widget ao lado), o takeover com sidebar recolhida (canto mais faixa reservada; no macOS também `fixture_page_hit` fora/dentro da faixa), a janela estreita e o encerramento liberando a faixa. Os hooks de fixture da Shell e o `WorkersModel::detached` público foram removidos. A detecção de vazamento continua ativa (`appshots-fixture`). Captura ausente, página nativa invisível ou checagem falha geram exit ≠ 0 com `result.json`. A execução visual no macOS fica com o orquestrador.
  - Rodada 3 (gate em par): o widget só fica ao lado de uma view nativa quando a cápsula inteira cabe (154×62 px, das mesmas constantes de estilo; fronteira exata e 1 px a menos testados). O stage focado volta a expor `Role::Group` com o rótulo "Voice call". Na fixture, a prova nativa exige página sem erro e com o título "Fieldnotes", e falha ao gravar o `result.json` vira saída ≠ 0. As execuções macOS anteriores (três temas, 15 capturas e 7 checagens cada) valem para a rodada 2; esses caminhos alterados precisam de nova execução no Mac.
  - Rodada 6 (GR16): no macOS `quit` encerra via `NSApplication terminate:` sem voltar de `run`; a fixture agora imprime a razão e encerra com `App::shutdown` + `process::exit(1)` antes do `quit` quando o resultado é falha (relatório não gravado, captura ou checagem falha). Sucesso continua saindo por `quit`.
  - Aceite Mac (2026-11-02): rodada final em glass/opaco/claro com 45 PNGs e 21 checks explicitamente conferidos no JSON, incluindo título/visibilidade nativa, hit testing, faixa de 94 px e liberação do Browser. Inspeção visual em lote e dos estados composer/widget concluída. GR16 comprovado depois: sucesso com 15 capturas/7 checks; relatório impossível de gravar e captura impossível retornam 1 com diagnóstico, e o segundo conserva JSON de falha. Recibos: `.tmp/evidence/desktop-corrections/mac-native-final/verification-receipt.json` e `.tmp/verify/desktop-corrections/mac/qa-final/acceptance-and-cleanup.json`. São componentes reais, não outra instância completa do app nem chamada/áudio real; execução do detector global em `Drop` no macOS não é alegada.
- [x] 4.2 Executar a rodada coordenada sob `heavy-run -- scripts/cargo-verify.py -- <comando>`: `cargo test --locked -p zeron-ui --lib -- shell:: browser:: markdown:: transcript:: url_chips voice dictation settings:: pasted_text_card rows_keep_creation_order` selecionou 1.025 testes, todos aprovados, incluindo voz, Settings e interação de painéis. O smoke compilou a fixture com `cargo build --locked -p zeron-ui --features appshots-fixture,browser-fixture --example desktop-corrections-fixture` e executou o binário nativo nos cenários de 4.1. Comandos, exits e logs em `.tmp/verify/desktop-corrections/mac/final/results.json` e `mac/qa-final/results.json`; os resultados negativos iniciais são preservados e os três defeitos da verificação têm prova posterior. Não foi exercitado áudio real, app completo, device remoto real ou outra plataforma nativa.
- [x] 4.3 Rodar `cargo fmt --all`, `openspec validate refine-desktop-voice-surfaces --strict` e o gate `/Users/guilhermevarela/orchestrator.v5/scripts/review-pair.sh --repo <checkout> --base <sha-inicial> --out <checkout>/.tmp/verify/refine-desktop-voice-surfaces/review-pair/<rodada-nova> --include-untracked --gate` antes do commit final. Entregar `summary.md` com exit 0, ou resolver/refutar achados conforme o contrato do harness. Gate indisponível, sem autenticação/serviço ou exit 4 autoriza o fallback de revisão manual completa registrada (`.tmp/verify/desktop-corrections/manual-review/1/summary.md`); sucesso automatizado nunca é declarado sem execução e segredo não resolvido continua bloqueando.

## Workflow follow-up

- O orquestrador confere os artefatos, resultados reais e cada critério de V1–V3 antes de aceitar a entrega.
- Com as tarefas comprovadas, arquivar pelo CLI OpenSpec, sincronizar somente os AGENTS.md necessários e fechar a branch/worktree conforme as skills do harness, preservando alterações alheias.
- Não editar `openspec/specs/` manualmente nem publicar outra release sem pedido do usuário.

Recibo da revisão final local do pacote: `.tmp/verify/desktop-corrections/final-review-receipt.json` registra a modalidade realmente executada, a identidade da árvore e o caminho do relatório automático ou da contingência manual, sem equiparar as duas modalidades.

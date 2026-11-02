# Design

## Context

Motivação e escopo: `proposal.md`; contrato de comportamento: `specs/codex-live-voice/spec.md`. Referências visuais: cinco imagens fornecidas pelo usuário nesta conversa; preservar o estilo nativo existente do Comet, não redesenhar o app.

Evidências da implementação atual:

- `shell.rs:16295-16455` monta Details e seu header separadamente da opacidade aplicada ao Chat e aos painéis da página.
- `shell/voice_stage.rs:512-537` monta o stage como overlay com `occlude()`. Em glass, ele não pinta fundo opaco: Details continua visível por baixo, mas o overlay recebe a interação. Não é necessário atribuir isso a uma sessão de voz perdida.
- `shell/voice_stage.rs:94-121` já recolhe o stage ao navegar sem terminar a chamada. `VoiceController` é independente do Chat selecionado.
- `render_voice_composer_orb` atualmente oferece retorno somente no Chat da chamada. Os handlers existentes `toggle_mute`, `end_voice` e `set_voice_stage_open` já fornecem as ações necessárias ao widget.
- Settings → Voice usa `dictation::VoiceCard` em `dictation/model.rs`, embora o runtime de ditado seja independente. A nova preferência pertence às configurações de UI, não ao estado do ditado.
- `Orb::set_visible(false)` e a convenção de visibilidade existente permitem interromper animação; reduzir movimento apenas congela sua apresentação e não atende V2.

## Goals / Non-Goals

**Goals:** um único dono do estado da chamada; separação entre visibilidade efetiva e preferências de painéis; preferência de orb aplicada a todas as suas apresentações de voz; um widget que acompanha a chamada e não o Chat selecionado.

**Non-Goals:** novos comandos de engine/provider, controle de saída de áudio, texto no widget, alteração de ditado ou de Reduce motion global, mudanças mobile, nova release ou restart do app instalado.

## Decisions

### D1 — Visibilidade de Details é derivada da presença do stage

Excluir corpo, header e alvos de interação de Details durante a presença do stage, inclusive na transição de saída. Não salvar `details_sidebar_open = false` nem alterar largura preferida para simular a exclusão. Usar o estado efetivo nos pontos de layout/visibilidade existentes e preservar a restauração no Chat atual. Apenas aplicar opacidade zero é insuficiente: deixaria alvos ou atividade de apresentação ocultos.

### D2 — Preferência independente e persistida

Adicionar `UiSettings.voice_orb_enabled: bool`, serializado como `voiceOrbEnabled`, com default `true` para arquivos existentes. Em Settings → Voice, incluir o switch **Show voice orb**, com ajuda **Hide voice visuals without ending the call.** Gravar pelo store atual com `SavePolicy::Immediate`; preservar o campo nos saves posteriores do Shell.

Desabilitado: nenhuma entidade orb é desenhada nas superfícies de voz; entidades retidas recebem visibilidade falsa para parar ticks. Não reservar `STAGE_ORB_BOX`, desenhar um frame congelado ou apenas esconder com alpha. Legenda e barra de chamada continuam. Ao reabilitar, reapresentar o estado atual, respeitando a política global de movimento sem modificá-la.

### D3 — Widget global alimentado pelo controller existente

Renderizar um único `render_voice_widget` no Shell pronto, fora do conteúdo de um Chat específico, quando `voice.is_live()` e o stage não estiver apresentado. Cobrir Chat, Workers e Settings, com sidebar aberta ou recolhida. Retirar `render_voice_composer_orb`, sua entidade específica e os testes do comportamento que foi substituído; não manter duas soluções concorrentes.

Manter o trigger da sidebar para iniciar/abrir voz. Quando os orbs estiverem desligados, ele e o widget usam controles estáticos identificáveis, sem uma órbita substituta.

Reutilizar a linguagem da barra atual: cápsula neutra com frost conforme o tema, hairline, controles arredondados e ícones existentes. Âncora inferior direita dentro da janela, com margem de 16 px. A cápsula possui alvos estáveis para microfone, retorno ao stage e encerramento. O encerramento aparece no hover de toda a cápsula ou no foco de qualquer controle, sem mudar o significado de outro botão ou deslocar alvos. Não adicionar o botão de digitação nem o alto-falante das referências, pois o pedido se limita a ações existentes de mudo/encerrar.

O microfone usa `VoiceController::toggle_mute`; retorno usa `set_voice_stage_open(true)`; encerrar usa `end_voice`, cujo cancelamento fecha o estado local imediatamente e mantém a parada do provider em background. Cliques nos controles não propagam para retorno/navegação. Sem snapshot de áudio válido, mudo não anuncia uma alteração fictícia; encerrar continua disponível nas fases de conexão.

### D4 — Composição e acessibilidade

O widget captura entrada apenas na sua cápsula, não em um invólucro de janela inteira. Fica abaixo dos diálogos e não cobre toasts; conferir sobreposição e recorte com panes nativos, sem ocultar todo o preview para exibi-lo. Hover deve pertencer a uma região estável para evitar o ciclo de aparecer/desaparecer sob o cursor já documentado em outros controles do Shell. Foco de teclado revela encerrar; cada ação tem role, label e foco visível. O clique de encerrar não seleciona o Chat da chamada.

## Approaches

A opção adotada nesta proposta é uma mudança de apresentação sobre `VoiceController`, com estado efêmero de hover/foco e uma única preferência nova. Ela reutiliza a sessão e as ações atuais.

Alternativas descartadas: manter o retorno apenas no composer não atende à navegação entre Chats; recriar ou retargetear uma chamada por Chat mudaria o ciclo de vida sem necessidade; usar Reduce motion ou um orb transparente não atende ao pedido de desligá-lo. Não há motivo para introduzir outro controller ou dependência.

## Risks / Trade-offs

- **R1: preferências sobrescritas por saves stale do Shell** → salvar no store canônico e cobrir disable, mudança de painel e reload juntos.
- **R2: widget oculto por panes nativos ou acima de um diálogo** → verificar a composição real e seus recortes existentes em janela larga/estreita; não aceitar apenas um teste de predicado de visibilidade.
- **R3: controles instáveis em hover ou clique encerrar que também abre o stage** → geometria fixa, handlers separados, propagação interrompida e teste com ponteiro estacionário e foco.
- **R4: desligar o orb sem parar ticks retidos** → sincronizar visibilidade das entidades também ao recolher sidebar, navegar e trocar a preferência, não somente no caminho que as desenha.
- **R5: prova nativa tocar configurações reais de providers** → fixture somente de componentes reais com estado sintético, sem inicializar Shell, engine ou providers. O lançamento de outra cópia completa continua proibido por `scripts/AGENTS.md:53`.

## Verification

Regressões de comportamento no harness GPUI existente: Details ausente e restauração das preferências; persistência independente do orb; widget em outros Chats/Workers/Settings; mudo e cancelamento sobre o mesmo estado; closed/failed; hover/foco sem reflow nem clique duplo; janela estreita e modais.

Smoke nativo: apresentar os componentes reais do stage, controles, preferência e widget numa fixture isolada, capturando temas glass/opaco, orb ligado/desligado, widget normal/mudo/hover/foco, dimensões estreita/larga e convivência com preview nativo. Ela não substitui aceite de chamada real no app instalado. Não iniciar provider ou capturar áudio real para esta mudança de UI; qualquer limite não exercitado será registrado como `notProven`, sem afirmar funcionamento visual a partir de unit tests.

## Migration Plan

Sem migração de wire ou sessão. Arquivos antigos usam default habilitado; desativação é local e persistida. Uma versão anterior ignora o campo adicional. A implementação será feita por um único Worker após a aprovação, pois os três pedidos compartilham Shell e visibilidade de voz; o orquestrador aceita provas e acompanha o gate e o fechamento. Publicação requer pedido separado.

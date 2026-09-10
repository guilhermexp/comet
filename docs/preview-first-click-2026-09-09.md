# Preview: primeiro clique e superfície — 2026-09-09

F1. O `InteractiveText` do GPUI pinado instala o listener de mouse-up somente
no paint posterior ao mouse-down. O teste que despacha press/release sem draw
intermediário falhou com zero aberturas. A correção usa o `div.on_click` dos
controles comuns, validando os dois pontos pelos glifos do link. O mesmo teste
passou, incluindo clique com repaint, botão secundário, área fora do link e
deslocamento de seleção. Cursor de mão continua restrito aos ranges dos links.
O teste compartilha o lock de seleção com os outros testes desse estado global.

F2. `FilePreview` e `Changes` já eram GPUI. A diferença de fundo vinha do
`.bg(theme.bg)` opaco no primeiro. Removido esse fill, ambos herdam a superfície
do mesmo host. O cabeçalho de arquivos passou para a escala de Changes: 36px,
padding `SPACE_MD`, hairline 0.06 e label de 12px. Nenhuma biblioteca trocada.
HTML/PDF/vídeo continuam no host nativo existente; a página HTML mantém seu CSS.

F3. O `~/.orchestrator/SOUL.md` mostrado pelo usuário tinha zero bytes no disco.
O preview agora informa `Empty file` para Markdown sem blocos, em vez de deixar
uma área vazia indistinguível de falha de carregamento. O arquivo original não
foi modificado.

Validação:

- Regressão: `cargo test -p zeron-ui markdown_link_opens_without_a_frame`;
  logs vermelho/verde em `/tmp/comet-first-click-{red,green}.log`.
- Suite: `cargo test -p zeron-ui` — 1.216 passaram, zero falhas.
- Build: `cargo build`; formatação e `git diff --check` passaram.
- Nova build nativa em app de QA isolado, usando o roteamento real dos links:
  um clique abriu Markdown e outro abriu HTML; arquivo vazio mostrou o estado
  explícito, e o Markdown preenchido mostrou a superfície temática sem preto
  opaco. Evidências locais: `/tmp/comet-first-click-markdown.png`,
  `/tmp/comet-first-click-html.png`, `/tmp/comet-empty-file-themed.png`.
- App principal reaberto com o binário corrigido após conferir que não havia
  sessões executando. No Chat da captura, um único clique abriu o relatório
  HTML real. `AGENTS.md` renderizou preenchido e `SOUL.md` mostrou `Empty file`;
  Working tree carregou as 101 mudanças para comparação da superfície. O diff
  aguardou carregamento após o restart; não foi medida sua latência.
  Evidências: `/tmp/comet-main-report-one-click.png`,
  `/tmp/comet-main-markdown-themed.png`, `/tmp/comet-main-empty-file-themed.png`
  e `/tmp/comet-main-diff-themed.png`.

O teste de abertura anterior verificava o caminho funcional e o custo de
repaint; ele não forçava press/release no mesmo quadro. A nova regressão cobre
essa sequência. Teste visual realizado no macOS; o ganho de latência não foi
recalculado, pois esta correção trata perda do gesto e aparência.

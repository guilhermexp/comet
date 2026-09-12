# DESIGN.md — Linguagem Visual do Comet

Referência canônica de design language visual para os clientes desktop do Comet (`apps/zeron`, `crates/ui`). Este documento define os contratos visuais, tokens, tipografia, geometria, componentes, motion, acessibilidade e validação estética consumidos na implementação de interfaces em GPUI.

---

## Índice

1. [Princípios](#1-princípios)
2. [Cor e roles semânticos](#2-cor-e-roles-semânticos)
3. [Tipografia](#3-tipografia)
4. [Espaço, raio, borda, elevação](#4-espaço-raio-borda-elevação)
5. [Layout e estrutura](#5-layout-e-estrutura)
6. [Inventário de componentes](#6-inventário-de-componentes)
7. [Iconografia](#7-iconografia)
8. [Motion](#8-motion)
9. [Estados e feedback](#9-estados-e-feedback)
10. [Acessibilidade](#10-acessibilidade)
11. [QA visual](#11-qa-visual)
12. [Não-negociáveis / anti-padrões](#12-não-negociáveis--anti-padrões)

---

## 1. Princípios

Os princípios fundamentais do Comet governam toda decisão visual do app e se apoiam em regras estritas de renderização:

- **Números dirigem layout, cores são paint**: o layout (larguras, alturas, paddings, bounds) é estritamente derivado de constantes numéricas e medições estáticas. Cores, gradientes e opacidades operam unicamente na camada de pintura (`paint`) e jamais recalculam geometria (`ARCHITECTURE.md` §4, `crates/ui/AGENTS.md:22`).
- **Animação é camada de paint**: transformações temporais (`with_animation`, transições de opacidade) rodam sobre atributos de desenho. Transições nunca alteram dimensões de caixas irmãs ou layout do flex container. `translateY` é projetado como `top` relativo pós-layout (`crates/ui/src/motion.rs:23-24`, `crates/ui/AGENTS.md:22`).
- **Altura de code block analítica**: a altura do bloco de código é invariante e equivale a $\text{linhas} \times \text{line-height} + \text{padding} + \text{header}$, independentemente do highlight de sintaxe. O tokenizer de sintaxe roda em background de forma time-sliced e aplica cores como text runs sem invalidar medição (`ARCHITECTURE.md` §4, `crates/ui/src/markdown/render.rs:34-36`, `crates/ui/AGENTS.md:23`).
- **Semântica do tema é intocável**: syntax highlighting, cores ANSI do terminal, diff semantics (`diff_add`, `diff_del`) e status indicators pertencem à variante de tema ativa e não sofrem recolorização por presets de acento (`accent_tokens`). O accent customizado personaliza exclusivamente controles interativos, seleção, foco, cursor e o glifo tri-tonal (`docs/theme-system.md:23-25`, `crates/ui/src/theme.rs:50-115`).
- **Respeito a `prefers-reduced-motion`**: animações respeitam a preferência do sistema operacional (`cx.reduce_motion()`). Elementos `with_animation` saltam instantaneamente para o estado final em animações únicas ou para o estado inicial em repetições, prevenindo agendamento de frames desnecessários (`crates/ui/src/motion.rs:17-21`, `crates/ui/AGENTS.md:22`).

---

## 2. Cor e roles semânticos

O sistema de cores do Comet utiliza variantes resolvidas desacopladas de fontes externas através do modelo `zeron-theme` (`crates/theme/src/lib.rs` e [`docs/theme-system.md`](docs/theme-system.md)). Componentes de UI consomem exclusivamente roles semânticos expostos em `crates/ui/src/theme.rs`.

### Modelo de tema (`zeron-theme`)

- `ThemeFamily`: agrupa variantes relacionadas (`crates/theme/src/lib.rs:434`).
- `ThemeVariant`: paleta completamente resolvida para aparência clara ou escura (`crates/theme/src/lib.rs:460`).
- `ThemeSelection`: armazena seleções independentes de IDs de variantes para os modos claro e escuro (`crates/theme/src/lib.rs:518`).
- `AccentSelection`: determina se a interface adota o acento autoral (`ThemeDefault`) ou um preset contrastado (`Preset`) (`crates/theme/src/lib.rs:541`).
- `SurfaceTreatment` vs `SurfacePreference`: `SurfaceTreatment` (`Frosted`, `Opaque`) é a recomendação da variante (`crates/theme/src/lib.rs:388`). `SurfacePreference` (`ThemeDefault`, `Frosted`, `Opaque`) é a preferência do usuário gravada localmente no dispositivo (`crates/theme/src/lib.rs:408`).
- **Limites por plataforma**: o efeito de vidro/frost na janela principal (`Theme::glass`, `crates/ui/src/theme.rs:734`) opera com translucidez no macOS (`Theme::GLASS_ALPHA = 0.80`, `crates/ui/src/theme.rs:686`), enquanto Linux e Windows forçam opacidade (`1.0`) por ausência de garantia de compositor blur. Superfícies flutuantes (popovers e composer pill) habilitam frost em macOS e Linux via `Theme::is_frost()` (`crates/ui/src/theme.rs:794-797`).

### Taxonomia de roles semânticos (`crates/ui/src/theme.rs:492-646`)

Os papéis semânticos da struct `Theme` são agrupados nas seguintes categorias de paint:

| Grupo de Role | Tokens Principais | O que pinta na UI |
|---|---|---|
| **Superfícies Neutras** | `bg`, `surface`, `surface_raised` | Fundo do painel de conteúdo principal, fundo do shell/sidebar e elementos elevados opacos (pills e chips). |
| **Escada de Elevação** | `surface_card`, `surface_dialog`, `surface_overlay`, `element_hover`, `element_active`, `border`, `border_strong` | Cards inline, modais flutuantes, popovers/menus suspensos, washes de hover/active e bordas hairlines/fortes. |
| **Tipografia** | `text`, `text_muted`, `text_faint`, `text_dim` | Texto primário (~17.5:1), labels secundários/timestamps, placeholders/desabilitados e caminhos de arquivo em diffs. |
| **Sólidos de Alto Contraste**| `solid`, `on_solid` | Placas de botões primários (máximo contraste: quase branco no dark, quase preto no light) e respectivos textos/ícones. |
| **Accent & Status** | `accent`, `accent_strong`, `accent_wash`, `on_accent`, `danger`, `danger_muted`, `warning`, `warning_muted`, `success`, `success_muted`, `busy`, `glyph` | Identidade interativa, badges de estado (erro, aviso, sucesso), working indicator e paleta tri-tonal do glifo animado. |
| **Componentes Dedicados** | `surface_raised_hover`, `band`, `input_bg`, `selection`, `cursor`, `caret`, `danger_strong` | Hover de pills, faixas de header/footer de popovers, fundo da pill do composer, seleção de texto, cursor de bloco e botões destrutivos. |
| **Código e Diff** | `code_text`, `code_wash`, `syntax`, `diff_add`, `diff_del`, `diff_hunk_bg` | Texto e wash de inline code, paleta de tokens de sintaxe, linhas adicionadas/removidas de diff e cabeçalhos de hunk. |
| **Terminal** | `terminal` (`TerminalColors`: `background`, `foreground`, `selection`, `ansi`) | Fundo do terminal, texto principal, highlight de seleção e as 16 cores padrão ANSI. |
| **Tipografia** | `font_sans`, `font_mono`, `font_sans_fallback`, `font_mono_fallback` | Nomes resolvidos das famílias de fonte para interface e superfícies monoespaçadas. |

### Escopo do accent (`accent_tokens`)

Definido em `crates/ui/src/theme.rs:50-115`, a função `accent_tokens(accent, appearance)` gera tokens ajustados em OKLCH para cada `AccentColor` (`Zeron`, `Orange`, `Amber`, `Green`, `Cyan`, `Blue`, `Pink`):
- Produz `primary`, `strong`, `wash`, `selection`, `caret`, `code_text`, `code_wash`, `activity` e `glyph`.
- Não afeta sintaxe de código, cores ANSI de terminal, nem identidade semântica de warning, error ou diff (`docs/theme-system.md:23-25`).

### Práticas proibidas em cores

- Proibido o uso de cores literais/hardcoded em elementos de UI fora da estrutura do `Theme`.
- Proibido recriar localmente tokens que foram depreciados no upstream (ex.: `white_alpha`); deve-se usar o equivalente semântico (`crates/ui/AGENTS.md:19`).
- Proibido redeclarar enums locais para `AccentColor` ou `Appearance`; devem ser reutilizados diretamente os tipos exportados por `zeron-theme` (`crates/ui/AGENTS.md:21`).

---

## 3. Tipografia

A tipografia do Comet equilibra fontes empacotadas nativas com flexibilidade de configuração local do usuário, garantindo rigidez métrica nas áreas analíticas (`crates/ui/src/typography.rs`).

### Famílias e empacotamento

- **Fontes embutidas**: Geist Sans (`GEIST`, 8 faces) e Geist Mono (`GEIST_MONO`, 8 faces) embutidas nos binários (`crates/ui/src/typography.rs:203-223`).
- **Fontes de interface configuráveis**: `UiFontFamily` suporta `Geist` (padrão), `GeistMono`, `System` (`.SystemUIFont`) e `Installed(String)` (`crates/ui/src/typography.rs:13-39`).
- **Superfícies de código estritamente mono**: o transcript de código, terminal e visualizador de diff operam obrigatoriamente com família monoespaçada (`theme.font_mono`), mantendo integridade colunar independente da preferência de interface (`crates/ui/AGENTS.md:20`).
- **Aliases virtuais do renderizador SVG**: `icons::Assets` redireciona chamadas virtuais de fontes SVG (`fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf` e `fonts/lilex/Lilex-Regular.ttf`) diretamente para os bytes de `GEIST[0]` e `GEIST_MONO[0]` (`crates/ui/src/icons.rs:23-24, 46-50`). É terminantemente proibido adicionar novos arquivos de fontes duplicados no bundle.
- **Invalidação de medição**: a alteração de família ou tamanho de fonte incrementa a geração do subsistema (`typography::generation(cx)`), invalidando caches de medição de texto e de linhas do transcript (`crates/ui/src/typography.rs:343-350`, `crates/ui/AGENTS.md:20`).

### Escala de tamanhos e métricas de texto

A escala de tamanho base de interface (`UiFontSize`) define 7 tamanhos numéricos: `12`, `13`, `14`, `15`, `16` (padrão), `18` e `20` px (`crates/ui/src/typography.rs:84-92, 112`). A macro `ui_rems(px)` mapeia dimensões com base no baseline de 16px (`pixels / 16.0`, `crates/ui/src/typography.rs:118-120`).

Escalas fixas em pixels empregadas pelo aplicativo:

| Papel / Superfície | Tamanho da Fonte | Altura de Linha (`line-height`) | Referência no Código |
|---|---|---|---|
| **Markdown Body** | 14.0 px | 22.0 px | `crates/ui/src/markdown/render.rs:32-33` |
| **Markdown Heading 1** | 19.0 px | 27.0 px | `crates/ui/src/markdown/render.rs:357` |
| **Markdown Heading 2** | 16.0 px | 24.0 px | `crates/ui/src/markdown/render.rs:358` |
| **Markdown Heading 3** | 15.0 px | 22.0 px | `crates/ui/src/markdown/render.rs:359` |
| **Markdown Heading 4+**| 14.0 px | 22.0 px | `crates/ui/src/markdown/render.rs:360` |
| **Markdown Code Block**| 12.5 px | 18.0 px (pad X: 12px, Y: 10px) | `crates/ui/src/markdown/render.rs:35-38` |
| **Composer Input** | 14.0 px | 22.75 px | `crates/ui/src/composer.rs:76-77` |
| **Terminal Grid** | 13.0 px | 18.0 px (padding: 12px) | `crates/ui/src/terminal/view.rs:26-29` |
| **Diff Line** | 12.0 px | 21.0 px | `crates/ui/src/changes.rs:67, 81` |
| **Badges / Tooltips** | 12.0 px | 16.0 px | `crates/ui/src/badges.rs:62, 159` |
| **Sidebar Labels** | 13.0 px | Não especificado no código (`crates/ui/src/workers/presentation.rs`) | `crates/ui/src/workers/presentation.rs:76` |

Pesos de fonte (`FontWeight`): utiliza diretamente as definições nativas do GPUI (`NORMAL`, `MEDIUM`, `SEMIBOLD`, `BOLD`). Tokens proprietários de peso: *Não especificado no código (`crates/ui/src/typography.rs`)*.

---

## 4. Espaço, raio, borda, elevação

Todas as medidas dimensionais derivam de constantes explícitas.

### Espaçamento

Constantes base definidas em `crates/ui/src/theme.rs:721-728`:
- `SPACE_XS`: `4.0 px`
- `SPACE_SM`: `8.0 px`
- `SPACE_MD`: `12.0 px`
- `SPACE_LG`: `16.0 px`
- `TEXT_STACK_GAP`: `1.0 px` (ajuste óptico entre título e descrição empilhados)

Gutters de coluna:
- `COLUMN_GUTTER`: `48.0 px` (gutter largo em volta da coluna de chat de 46rem; `crates/ui/src/transcript.rs:102`)
- `RESPONSIVE_COLUMN_GUTTER`: `10.0 px` (gutter de separação responsiva entre colunas; `crates/ui/src/shell.rs:521`)

### Raios de curvatura (`border-radius`)

- `Theme::BUBBLE_RADIUS`: `16.0 px` (balões de mensagem; `crates/ui/src/theme.rs:715`)
- `Theme::PANEL_RADIUS`: `10.0 px` (painéis e cards principais; `crates/ui/src/theme.rs:717`)
- `Theme::CONTROL_RADIUS`: `6.0 px` (botões e chips pequenos; `crates/ui/src/theme.rs:719`)
- `COMPOSER_CORNER_RADIUS`: `12.0 px` (pill do composer e user message card; `crates/ui/src/composer.rs:70`)
- `CARD_RADIUS`: `12.0 px` (cards de popover; `crates/ui/src/popover.rs:306`)
- `QuestionPanel Pill`: `26.0 px` (`crates/ui/src/composer.rs:6426`)
- `Modal Dialog`: `16.0 px` (`crates/ui/src/popover.rs:958`)
- `Sidebar Row`: `9.0 px` (`crates/ui/src/workers/presentation.rs:72`)
- `Badge / Chip Pill`: `8.0 px` (`crates/ui/src/badges.rs:60`)

### Borda e espessura

- Borda hairline padrão: `1.0 px` (`border_1()`, `Theme::border`, `crates/ui/src/theme.rs:544`)
- Borda forte de foco/elevação: `Theme::border_strong` (`crates/ui/src/theme.rs:546`)
- Separador de split no diff: `SPLIT_DIVIDER_WIDTH = 1.0 px` (`crates/ui/src/changes.rs:80`)
- Barra de destaque de adição/remoção: `ACCENT_BAR_WIDTH = 3.0 px` (`crates/ui/src/changes.rs:75`)

### Profundidade e elevação

A profundidade no Comet é expressa pela composição de três elementos coordenados:
1. **Escada de luminosidade de superfícies**: no dark mode, as camadas ganham separação por passos sutis de claridade (`bg` `#060606` $\to$ `surface` `#0d0d0d` $\to$ `surface_card` $\to$ `surface_dialog` $\to$ `surface_overlay`). No light mode, o fundo base é branco puro e a profundidade é conferida por bordas e scrims (`crates/ui/src/theme.rs:510-532`).
2. **Bordas estruturais**: hairlines (`border` e `border_strong`) delimitam rigorosamente as fronteiras de cada plano.
3. **Frost (Backdrop Blur)**: camadas flutuantes aplicam `frosted(corner_radius, blur_radius, ...)` (`crates/ui/src/frost.rs:27`) com desfoque de fundo em plataformas suportadas. Escalas arbitrárias de drop-shadow: *Não especificado no código (`crates/ui/src/theme.rs`)*; o app emprega unicamente as funções nativas de sombra do GPUI (`shadow_md()`, `shadow_lg()`).

---

## 5. Layout e estrutura

A casca do aplicativo (`Shell`) estrutura a interface em três regiões principais: sidebar à esquerda, coluna central de Chat (`ChatPanel`) e painéis utilitários à direita (`crates/ui/src/shell.rs`).

```
+------------------+------------------------------+--------------------+------------------+
| Sidebar          | Chat Column (Chat Panel)     | Right Pane (Tabs)  | Details / Files  |
| 208..400px       | Min: 300px (Default: 768px)  | 360..Max px        | 300..700px       |
| (Default: 256px) |                              | (Default: 520px)   | (Default: 500px) |
+------------------+------------------------------+--------------------+------------------+
```

### Grade do Shell e larguras de coluna

- **Sidebar**:
  - Modos: `SidebarMode::Orchestrator` (lista de Chats e Spaces) e `SidebarMode::Workers` (projetos e sessões CLI).
  - Switcher de modos: altura `36.0 px`, raio `10.0 px`, raio do botão `8.0 px` (`crates/ui/src/shell.rs:460-462`).
  - Larguras: mínima `208.0 px`, máxima `400.0 px`, padrão `256.0 px` (`crates/ui/src/settings.rs:29-31`).
- **Coluna de Chat (`ChatPanel`)**:
  - Piso mínimo preservado: `CHAT_PANEL_MIN = 300.0 px` (`crates/ui/src/settings.rs:39`).
  - Reserva mínima responsiva: `RESPONSIVE_MAIN_PANE_MIN = 320.0 px` (`crates/ui/src/shell.rs:520`).
  - Largura máxima do conteúdo do composer: `COMPOSER_MAX_WIDTH = 768.0 px` (`crates/ui/src/composer.rs:68`).
- **Host único do Pane Direito (`right_tabs` / `right_active`)**:
  - Centraliza unificadamente todas as abas técnicas em um único host: `Terminal`, `Diff`, `Preview` (arquivos), `Subagent` e `Worker` (`crates/ui/src/shell.rs:1561, 2758`).
  - **Regra `right_pane_open`**: a coluna lateral direita só é montada se houver contexto disponível **e** ao menos uma aba na coleção (`crates/ui/src/shell.rs:2471-2479`, `crates/ui/AGENTS.md:53`). Sem abas abertas, a coluna colapsa totalmente.
  - Larguras: mínima `360.0 px`, padrão `520.0 px` (`crates/ui/src/settings.rs:36-37`).
  - Teto dinâmico: $(\text{viewport} - \text{sidebar} - \text{CHAT\_PANEL\_MIN}).\text{max}(0.0)$ (`crates/ui/src/shell.rs:436`).
  - Modo expandido (`takeover`): ocupa todo o espaço da janela descontando a sidebar (`crates/ui/src/shell.rs:442`).
- **Coluna Details / Files**:
  - Exibe metadados de workspace, telemetria de Workers e árvore de arquivos.
  - Larguras: mínima `300.0 px`, máxima `700.0 px`, padrão `500.0 px` (`crates/ui/src/settings.rs:42-44`).
- **Transições de Painéis**:
  - Alterações de largura e colapso de sidebar e panes realizam interpolação por tween de `200 ms` com curva `EASE_OUT` (`motion::RESIZE`, `crates/ui/src/motion.rs:296`, `WidthTween` em `crates/ui/src/shell.rs:4536`).

---

## 6. Inventário de componentes

Tabela consolidada com os componentes fundamentais de `crates/ui`:

| Componente | Arquivo-fonte | Anatomia Resumida | Estados Suportados | Roles e Tokens de Tema |
|---|---|---|---|---|
| **Composer** | `composer.rs` | Pill arredondado, input de texto multi-linha, rail de anexos superior, ações de envio/parada | Compacto, expandido, typing, disabled, streaming (Send/Steer/Stop) | `input_bg`, `border`, `text`, `caret`, `selection`, `accent`, `danger_strong` |
| **Question Panel** | `composer.rs` | Overlay no composer, lista de opções (1-9), campo de texto livre, botões Submit/Skip | Paged, single-select, multi-select, submitting | `surface_card`, `border`, `text`, `accent`, `element_hover` |
| **Popover / Picker** | `popover.rs`, `pickers.rs` | Card flutuante com blur, search input, lista de itens com teclado, key caps | Closed, opening (`MENU_IN`), item hover, active | `surface_overlay`, `glass_overlay`, `border`, `element_hover`, `text_muted` |
| **Badges e Chips** | `badges.rs`, `url_chips.rs` | Pill de altura fixa (24px) com ícone (12px), rótulo e popover hover opcional | Normal, hover, clicked | `surface_raised`, `text_muted`, `text`, `border`, `accent` |
| **Stream de Tool** | `transcript.rs`, `turn_steps.rs` | Timeline 28px: slot de ícone 18px, verbo+objeto numa linha, sem card | Running, resolved, error, expanded (invocação + resultado em painel de código com scroll) | `text`, `text_muted`, `danger`, `ink(0.10)` |
| **Turn Steps** | `turn_steps.rs` | Cabeçalho resumido com contagem de atividades de ferramentas e chevron de dobra | Collapsed, expanded | `surface_card`, `text_muted`, `text_faint`, `border` |
| **Transcript Rows** | `transcript.rs` | Lista virtualizada baseada em blocos estáveis (`msgId#blockId`) com follow-tail | Normal, hovering, selected | `bg`, `text`, `text_muted`, `code_text`, `syntax` |
| **Sticky User Message** | `transcript.rs` | Clone do cabeçalho do turno do usuário fixado no topo do runway durante scroll | Docked, sliding, unpinned | `surface_card`, `surface_raised`, `border`, `text` |
| **Mídia Inline** | `inline_media.rs`, `transcript.rs` | Imagem sem moldura com altura fixa (260px); diagrama Mermaid ajustado à coluna | Loading, rendered, error (code fallback) | `surface_card`, `border`, `text_muted` |
| **Mermaid Lightbox** | `mermaid_preview.rs` | Modal tela-cheia com pan/zoom contínuo, toolbar e botões de exportação/cópia | Open, dragging, zoomed, fit | `surface_dialog`, `solid`, `on_solid`, `text_muted` |
| **Loaders e Shimmer** | `loaders.rs` | Matriz 3x3, barras pulsantes escalonadas e faixa de atividade na aba | Active, leasing, reduced-motion | `accent`, `glyph`, `busy`, `activity_shimmer` |
| **Message Rail** | `rail.rs` | Trilho de micro-marcas na borda esquerda para navegação rápida por prompt | Normal, hover (card preview), active tick | `accent`, `text_muted`, `border`, `surface_card` |
| **Edge Fade** | `edge_fade.rs` | Máscara de shader linear para dissolver texto sob barras fixas de navegação | Fading top/bottom, overflow-x/y | Shader-level alpha mask |
| **Frost** | `frost.rs` | Camada de desfoque de fundo (`backdrop_blur`) encapsulando cantos arredondados | Active (macOS/Linux), bypass (Windows) | `surface_overlay` com opacidade reduzida |
| **Terminal Grid** | `terminal/` | Emulador Alacritty renderizado em grid de células GPUI com scrollback próprio | Focused, unfocused, selecting, cursor blink | `terminal.background`, `terminal.foreground`, `terminal.ansi` |
| **Diff / Changes Rows** | `changes.rs` | Linhas virtualizadas unificadas ou lado a lado com marcadores +/− e headers | Added, deleted, modified, collapsed file | `diff_add`, `diff_del`, `diff_hunk_bg`, `border` |
| **Settings Rows & Cards**| `settings/` | Seções organizadas em cards elevados com campos de entrada, selects e toggles | Normal, focused, saving, error | `surface_card`, `border`, `text`, `element_hover` |
| **Empty States** | `shell.rs`, `settings/` | Ilustração/ícone minimalista, título discreto e texto orientativo centralizado | Idle | `text_muted`, `text_faint`, `surface_card` |
| **Notices & Banners** | `shell.rs` | Notificação in-app na sidebar com tom de sucesso ou falha explícitos | Visible, dismissed | `text`, `danger`, `success`, `surface_card` |
| **Worker Indicator** | `workers/presentation.rs` | Marcador circular de status na linha de sessão do Worker CLI | Busy, Attention, Unread, Idle, Exited | Blue tint (unread), accent (busy), amber (attention) |
| **Menu Bar Status** | `workers/menu_bar.rs` | Item nativo do sistema com spinner monoespaçado (15pt) e contagem (11pt) | Running, idle | Cor herdada nativa do NSButton |
| **Avatares Blobatar** | `icons.rs`, `details_sidebar/` | Conjunto de 28 avatares lúdicos com hash determinístico por subagente | Rendered, loading | Fill adaptado aos tons do tema |
| **Trajectory Surface** | `trajectory/` | Timeline analítica de 3 lanes, ledger virtualizado (26px) e inspector de 5 abas | Live stream, paused, inspect, raw reveal | `accent_wash`, `border`, `text_dim`, `surface_card` |

### Subseções de Componentes

#### Composer (`crates/ui/src/composer.rs`)
Pill flutuante na parte inferior do Chat.
- **Anatomia**: caixa de texto extensível (`TA_MIN = 76px`, `TA_MAX = 260px`, `crates/ui/src/composer.rs:52-53`), rail superior de anexos (`STRIP_THUMB = 56px`, `crates/ui/src/composer.rs:4482`), área de ações com botão dinâmico (Send $\to$ Steer $\to$ Stop).
- **Estados**: Compacto (`COMPACT_TOTAL_HEIGHT = 49px`, `crates/ui/src/composer.rs:66`), Expandido (`COMPOSER_MIN_HEIGHT = 124px`, `COMPOSER_MAX_HEIGHT = 308px`, `crates/ui/src/composer.rs:61, 63`), Disabled.
- **Roles**: `input_bg`, `border`, `text`, `caret`, `selection`, `accent`, `danger_strong`.

#### Question Panel (`crates/ui/src/composer.rs:6358-6590`)
Substitui o composer ativo quando o agente solicita respostas estruturadas ou confirmações.
- **Anatomia**: container no raio de `26.0 px`, listagem vertical com slots numéricos (teclas 1 a 9), campo para "Other (type your own)", botões Skip e Submit.
- **Estados**: Selecionado, Não selecionado, Submetendo. Possui temporizador de avanço automático em seleções únicas (`AUTO_ADVANCE_MS = 220 ms`, `crates/ui/src/composer.rs:79`).
- **Roles**: `surface_card`, `border`, `text`, `accent`, `element_hover`.

#### Popover e Pickers (`crates/ui/src/popover.rs`, `crates/ui/src/pickers.rs`)
Menus de seleção rápida (modelos, branches, ferramentas, menções `@` e `/`).
- **Anatomia**: card elevado com `CARD_RADIUS = 12.0 px` (`crates/ui/src/popover.rs:306`), campo de busca opcional, itens de menu com altura padronizada, scroll com atenuação de borda.
- **Estados**: Aberto (`MENU_IN` 140ms), Fechando (`MENU_OUT` 100ms), Item Hover, Item Focado.
- **Roles**: `surface_overlay`, `glass_overlay`, `border`, `element_hover`, `text_muted`.

#### Badges e Chips (`crates/ui/src/badges.rs`, `crates/ui/src/url_chips.rs`)
Indicadores compactos inline para metadados, links e referências git.
- **Anatomia**: altura de `24.0 px` (`BADGE_HEIGHT`, `crates/ui/src/badges.rs:58`), raio `8.0 px`, ícone `12.0 px`, texto `12.0 px`. Chips de URL projetam GitHub e YouTube no texto de mensagens já persistidas.
- **Estados**: Normal, Hover (com hover card de detalhes após `280 ms`, `crates/ui/src/badges.rs:64`).
- **Roles**: `surface_raised`, `text_muted`, `text`, `border`.

#### Stream de Tool e Turn Steps (`crates/ui/src/transcript.rs`, `crates/ui/src/turn_steps.rs`)
Eventos compactos com geometria única em `stream_event_row`, sem cards nem conectores verticais. Grupos de tools, raciocínio e tarefas iniciam fechados; raciocínio mostra prévia textual em vez de repetir Thought. O clique revela o conteúdo completo.
- **Anatomia**: row de `28.0 px` (`CHIP_HEIGHT`) com ícone de `14.0 px` num slot de `18.0 px` e copy verbo+objeto em sans regular 14px/22px, como o corpo da conversa; gaps entre blocos de 4px. Sem card interno, sem hairline. `TurnSteps` agrupa o prefixo resolvido num fold com chevron junto ao conteúdo e contadores (`Ran 3 commands · read 2 files`).
- **Estados**: Executando (spinner no trailing), pendente, falha (tint `danger` na linha), expandido (stdout/diff alinhados sob o texto, sem conectores verticais).
- **Roles**: `text`, `text_muted`, `text_faint`, `danger`.

#### Rows do Transcript (`crates/ui/src/transcript.rs`)
Lista virtualizada de blocos Markdown e chamadas do agente.
- **Anatomia**: virtualização via `gpui::list` com identificador composto `msgId#blockId`. Margem lateral negativa (`-COLUMN_GUTTER = -48.0 px`) para expansão de figuras.
- **Estados**: Normal, Streaming (com fade veil na cauda do texto), Selecionado.
- **Roles**: `bg`, `text`, `text_muted`, `syntax`.

#### Mensagem de Usuário Sticky (`crates/ui/src/transcript.rs:5660-5710`)
Fixação temporária do cabeçalho da pergunta do usuário no topo do runway durante rolagem longa.
- **Anatomia**: clone puramente visual do cabeçalho com cantos arredondados (`12.0 px`), sem alterar altura da lista virtualizada.
- **Estados**: Docked (ancorado), Sliding (empurrado pelo próximo cabeçalho).
- **Roles**: `surface_card`, `border`, `text`.

#### Mídia Inline (`crates/ui/src/inline_media.rs`, `crates/ui/src/transcript.rs:85-87`)
Visualização de capturas, imagens e diagramas no corpo do chat.
- **Anatomia**: imagens sem moldura com altura analítica de `260.0 px` (`INLINE_IMAGE_HEIGHT`) e largura auto. Diagramas Mermaid renderizados nativamente via QuickJS/beautiful-mermaid com escala `1.25` (`INLINE_MERMAID_SCALE`) e botão de maximizar.
- **Estados**: Loading, Carregado, Fallback para código puro com highlight de sintaxe em caso de erro.
- **Roles**: `surface_card`, `border`, `text_muted`.

#### Mermaid Lightbox (`crates/ui/src/mermaid_preview.rs`)
Modal imersivo para exploração detalhada de diagramas.
- **Anatomia**: viewport flutuante com controles de zoom (Fit, In, Out, 1:1), pan por arraste de mouse/trackpad e botão de fechar. Limite de zoom ajustado entre `0.1` e `2.0` (`MAX_FIT_ZOOM = 2.0`, `crates/ui/src/mermaid_preview.rs:22-23`).
- **Estados**: Normal, Pan ativo, Zooming.
- **Roles**: `surface_dialog`, `solid`, `on_solid`, `text_muted`.

#### Loaders e Shimmer (`crates/ui/src/loaders.rs`, `crates/ui/src/motion.rs:53-58`)
Indicadores de progresso e atividade assíncrona.
- **Anatomia**: spinner 3x3 (`gradient_spinner`), spinner monoespaçado (`mini_mono_spinner`), anéis de progresso circulares e `activity_shimmer` (banda de 34% que varre a aba de abas em 1.8s a 30fps).
- **Estados**: Animando, Pausado (sob reduced-motion).
- **Roles**: `busy`, `accent`, `glyph`.

#### Message Rail (`crates/ui/src/rail.rs`)
Trilho de navegação rápida situado na margem esquerda da coluna de chat.
- **Anatomia**: faixa vertical visível em larguras $\ge 768.0\text{ px}$ (`RAIL_MIN_CONTAINER_WIDTH`), com no máximo 12 marcas (`MAX_RAIL_TICKS = 12`, `crates/ui/src/rail.rs:127`). Altura da marca `10.0 px`, intervalo `3.0 px`. Ao clicar, rola suavemente com tween de 500ms (`SCROLL_GLIDE`).
- **Estados**: Tick inativo, Tick ativo, Tick em hover (revela card com prévia de 160 caracteres do prompt).
- **Roles**: `accent`, `text_muted`, `surface_card`.

#### Edge Fade (`crates/ui/src/edge_fade.rs`)
Transição suave de desaparecimento de conteúdo em áreas de rolagem.
- **Anatomia**: máscara GLSL embutida no pipeline do GPUI para atenuação gradual de texto nas bordas de cabeçalho ou rodapé (`SIDEBAR_GLASS_FADE_BAND = 24.0 px`, `TRANSCRIPT_FADE_BAND = 24.0 px`).
- **Estados**: Ativo no topo, ativo na base, ativo em eixos horizontais de abas.
- **Roles**: Máscara de opacidade sobre o elemento.

#### Frost (`crates/ui/src/frost.rs`)
Componente para composição de materiais translúcidos com desfoque de fundo.
- **Anatomia**: invólucro com `backdrop_blur` que reproduz acabamento fosco em janelas e popovers no macOS e Linux.
- **Estados**: Ativo, Inativo (plataformas sem suporte utilizam superfícies sólidas).
- **Roles**: Tint derivado de `surface_overlay` ou `surface` com opacidade reduzida.

#### Grade do Terminal (`crates/ui/src/terminal/`)
Emulação de terminal integrada para sessões interativas e saída de comandos.
- **Anatomia**: grid de células monoespaçadas baseado no Alacritty (`TERM_FONT_SIZE = 13.0 px`, `TERM_LINE_HEIGHT = 18.0 px`, `crates/ui/src/terminal/view.rs:26-27`), com padding de `12.0 px` e barra de abas com altura de `40.0 px` (`TAB_BAR_HEIGHT`, `crates/ui/src/terminal/panel.rs:44`).
- **Estados**: Focado, Sem foco, Seleção de texto ativa, Rolagem no scrollback de 10.000 linhas (`SCROLLBACK_LINES`, `crates/ui/src/terminal/emulator.rs:44`).
- **Roles**: `terminal.background`, `terminal.foreground`, `terminal.ansi`.

#### Diff e Changes Rows (`crates/ui/src/changes.rs`)
Painel de visualização de alterações de código.
- **Anatomia**: visualização unificada ou side-by-side. Altura de linha `21.0 px` (`DIFF_LINE_HEIGHT`), cabeçalho de arquivo `36.0 px` (`FILE_HEADER_HEIGHT`), cabeçalho de hunk `28.0 px` (`HUNK_HEADER_HEIGHT`), calha de numeração `36.0 px` (`GUTTER_WIDTH`) e barra de destaque `3.0 px` (`ACCENT_BAR_WIDTH`).
- **Estados**: Adição, Remoção, Contexto, Arquivo colapsado (`COLLAPSE` 180ms).
- **Roles**: `diff_add`, `diff_del`, `diff_hunk_bg`, `border`.

#### Settings (Cards e Rows) (`crates/ui/src/settings/`)
Configurações da aplicação e preferências locais.
- **Anatomia**: formulários organizados em cards elevados (`surface_card`) com espaçamento vertical padronizado, seletores em pills e campos de texto com foco delimitado.
- **Estados**: Normal, Hover, Focado, Modificado/Salvando.
- **Roles**: `surface_card`, `border`, `text`, `text_muted`, `element_hover`.

#### Empty States (`crates/ui/src/shell.rs`, `crates/ui/src/settings/`)
Telas de ausência de dados (sem sessões arquivadas, sem chats, sem workers).
- **Anatomia**: layout centralizado vertical e horizontalmente contendo ícone suave (`text_faint`), título informativo e copy explicativo.
- **Estados**: Padrão de repouso.
- **Roles**: `text_muted`, `text_faint`.

#### Avisos e Notificações (`crates/ui/src/shell.rs:1116-1192`)
Canais de sinalização operacional para o usuário:
1. `SidebarNotice`: feedback local imediato de ações ativas (ex.: exportação de chat, cópia de link) com texto e indicador explícito de sucesso ou falha (`crates/ui/src/shell.rs:1122`).
2. Banner in-app: avisos de desconexão e recuperação de conectividade de rede/engine.
3. `notify::post`: canal reservado estritamente para eventos em background do sistema operacional, jamais acionado com a janela em foco ativo (`crates/ui/AGENTS.md:25`).
- **Roles**: `surface_card`, `text`, `success`, `danger`.

#### Indicadores de Sessão de Worker (`crates/ui/src/workers/presentation.rs:54-61, 139-168`)
Pontos visuais coloridos nas linhas da sidebar de Workers:
- **Anatomia**: marcadores circulares representando o estado real da sessão.
- **Estados**:
  - `Busy`: agente executando ativamente (accent).
  - `Attention`: sessão pausada aguardando input ou bloqueada (amber/warning).
  - `Unread`: worker finalizado normalmente sem visualização prévia (azul, nunca accent; `crates/ui/AGENTS.md:83`).
  - `Idle`: worker parado em prontidão.
  - `Exited`: processo morto ou cancelado.
  - `Restarting`: reinicialização pendente.
- **Roles**: `project_folder_tint`, `warning`, `accent`.

#### Status Item da Menu Bar (`crates/ui/src/workers/menu_bar.rs:586-638`)
Item do macOS no System Menu Bar alimentado via Objective-C (`NSStatusItem`).
- **Anatomia**: spinner de 15pt (`SPINNER_POINTS`) acompanhado de contagem de 11pt (`COUNT_POINTS`) separados por espaço fino.
- **Hierarquia**: a diferenciação é exclusivamente por tamanho físico da fonte monoespaçada; a contagem compartilha exatamente a mesma cor do glifo para manter legibilidade e suportar inversão nativa de realce.
- **Roles**: Cores nativas do sistema macOS (`NSButton`).

#### Avatares de Subagente / Blobatar (`crates/ui/src/details_sidebar/subagent_avatars.rs`)
Identidade visual determinística para agentes especializados:
- **Anatomia**: biblioteca de 28 ilustrações embutidas (`icons/subagents/blobatar/00.svg` a `27.svg`), selecionadas por hash do ID estável do subagente.
- **Dimensões**: renderizado em tiles de `22.0 px` com imagem de `18.0 px` no transcript (`crates/ui/src/transcript.rs:8409-8422`).
- **Roles**: Fills e contornos adaptados à variante de tema.

#### Superfície de Trajetória (`crates/ui/src/trajectory/`)
Painel técnico e analítico de auditoria da execução do agente no Chat ativo (`crates/ui/src/trajectory/AGENTS.md`).
- **Anatomia**:
  - Timeline com 3 lanes fixas (`Input`, `Model`, `Tools`).
  - Ledger hierárquico com altura de linha estritamente fixa (`ROW_HEIGHT = 26.0 px`, `crates/ui/src/trajectory/AGENTS.md:19`).
  - Inspector de 5 abas (`Summary`, `Payload`, `Result`, `Schema`, `Timing`, `crates/ui/src/trajectory/inspector.rs:46-52`).
  - Raw Reveal efêmero local: revela dados sanitizados em memória sob demanda, sem persistência nem sincronização remota.
- **Estados**: Live streaming, Pausado (após rolagem para trás), Filtrado por busca (com dimming contextual sem ocultar linhas), Split horizontal ($\ge 600\text{ px}$) e Narrow ($< 600\text{ px}$).
- **Roles**: `accent_wash`, `border`, `text_dim`, `surface_card`.

---

## 7. Iconografia

A iconografia do Comet combina ícones vetoriais embutidos com bibliotecas externas de linguagens e ferramentas (`crates/ui/src/icons.rs`, `crates/ui/src/tool_icons.rs`).

### Origem dos assets

- **Ícones do App**: SVGs embutidos diretamente no binário via `include_bytes!` a partir de `crates/ui/assets/icons/` (`crates/ui/src/icons.rs:60-63`).
- **Material File Icons**: ícones contextuais para linguagens de programação e ferramentas (git, nodejs, python, rust, docker, etc.) gerados em compilação via `material_file_icon_assets` (`crates/ui/src/icons.rs:26-28`).
- **Subagent Avatars**: 28 avatares Blobatar empacotados em `blobatar_subagent_avatar_assets` (`crates/ui/src/icons.rs:30-34`).

### Tamanhos padronizados

- `12.0 px`: Badges inline, chips de documento, status micro (`crates/ui/src/badges.rs:61`).
- `13.0 px`: Ícone de harness ativo na sidebar (`crates/ui/src/shell.rs:959`).
- `14.0 px`: Ícone de harness arquivado na sidebar (`crates/ui/src/shell.rs:961`).
- `16.0 px`: Ações padrão de barra de título, controles e botões de header.
- `18.0 px`: Slot de ícone no stream de tools e avatares de subagente (`crates/ui/src/transcript.rs`).
- `24.0 px`: Botões do cluster de navegação principal da barra de título (`crates/ui/src/shell.rs:232`).

### Ícones contextuais de tool e provedores

- `tool_icons::tool_icon_descriptor`: inspeciona chamadas `ToolCall` (comandos de terminal, leituras, gravações, patches) e mapeia para o asset semântico Material correspondente ou ícone Solar (`crates/ui/src/tool_icons.rs:258-274`).
- `runtime_icon_path`: mapeia comandos e CLIs suportados para os respectivos ícones de provedor (Claude, Codex, Kimi, Cursor, Gemini, Grok, Pi, Antigravity, OMP; `crates/ui/src/workers/presentation.rs:94-117`).

### Regra mandatória de cor em SVG

No GPUI, o método `gpui::Svg::paint` calcula a cor a partir de `self.path.zip(style.text.color)` do elemento SVG individual, partindo de `Style::default()`. A cor definida em um container pai **não cascateia** para o elemento SVG (`crates/ui/AGENTS.md:105`).
- Todo ícone renderizado via `icons::icon(...)` deve obrigatoriamente receber `.text_color(...)` no próprio nó SVG.
- Omitir a cor resulta em um ícone totalmente invisível (zero pixels desenhados).

---

## 8. Motion

O subsistema de animação (`crates/ui/src/motion.rs`) reúne curvas e especificações temporais projetadas para garantir fluidez sem degradar o desempenho do hardware.

### Catálogo de Motion (`crates/ui/src/motion.rs:210-320`)

| Nome do Motion | Duração (`ms`) | Curva de Easing (`CubicBezier`) | Onde se Aplica na UI | Referência no Código |
|---|---|---|---|---|
| `FADE_IN` | 500 ms | `EASE_OUT_EXPO` (0.16, 1.0, 0.3, 1.0) | Entrada padrão de elementos: opacidade 0 $\to$ 1 + translateY 4 $\to$ 0 | `crates/ui/src/motion.rs:283` |
| `FADE_QUICK` | 150 ms | `EASE` (0.25, 0.1, 0.25, 1.0) | Desaparecimento e transições rápidas de opacidade | `crates/ui/src/motion.rs:285` |
| `MENU_IN` | 140 ms | `EASE` (0.25, 0.1, 0.25, 1.0) | Abertura de popovers/menus (escala 0.96 e translateY -2) | `crates/ui/src/motion.rs:287` |
| `MENU_OUT` | 100 ms | `EASE` (0.25, 0.1, 0.25, 1.0) | Fechamento de menus (mais rápido que a entrada) | `crates/ui/src/motion.rs:290` |
| `DIALOG_IN` | 180 ms | `EASE` (0.25, 0.1, 0.25, 1.0) | Abertura de caixas de diálogo modais (escala 0.96 $\to$ 1.0) | `crates/ui/src/motion.rs:292` |
| `SPLASH_OUT` | 500 ms (+150ms delay) | `EASE` (0.25, 0.1, 0.25, 1.0) | Saída da tela de splash inicial: fade e subida de 6px | `crates/ui/src/motion.rs:294` |
| `RESIZE` | 200 ms | `EASE_OUT` (0.0, 0.0, 0.58, 1.0) | Transições de largura e altura da sidebar e painéis laterais | `crates/ui/src/motion.rs:296` |
| `TAB_SLIDE` | 150 ms | `EASE_OUT` (0.0, 0.0, 0.58, 1.0) | Reordenação por arraste de abas do terminal e pane | `crates/ui/src/motion.rs:298` |
| `COLLAPSE` | 180 ms | `EASE_OUT` (0.0, 0.0, 0.58, 1.0) | Colapso e expansão de arquivos no visualizador de diff | `crates/ui/src/motion.rs:300` |
| `CHEVRON` | 200 ms | `EASE` (0.25, 0.1, 0.25, 1.0) | Rotação aproximada de chevrons de expansão | `crates/ui/src/motion.rs:303` |
| `SCROLL_GLIDE` | 500 ms | `EASE_IN_OUT` (0.42, 0.0, 0.58, 1.0) | Rolagem suave via message rail ou saltos programados | `crates/ui/src/motion.rs:307` |
| `HOVER_FADE` | 150 ms | `EASE_TAILWIND` (0.4, 0.0, 0.2, 1.0) | Dissolução temporal de hover wash em botões e linhas | `crates/ui/src/motion.rs:313` |
| `ZERON_PULSE` | 2400 ms | `EASE` (0.25, 0.1, 0.25, 1.0) | Ciclo completo dos loaders e ondas de células | `crates/ui/src/motion.rs:315` |
| `ACTIVITY_SHIMMER`| 1800 ms | `EASE` (0.25, 0.1, 0.25, 1.0) | Varredura de brilho na strip de abas durante atividade | `crates/ui/src/motion.rs:317` |
| `GRADIENT_SPIN`| 750 ms | `EASE` (0.25, 0.1, 0.25, 1.0) | Onda de rotação matricial no WorkingIndicator | `crates/ui/src/motion.rs:319` |
| `RESORT` | 260 ms | `EASE_RESORT` (0.22, 1.0, 0.36, 1.0) | Animação de reordenação FLIP das linhas da sidebar | `crates/ui/src/shell.rs:892` |

### Pulse clock compartilhado (`motion::pulse_delta`)

Para animações cíclicas persistentes (spinners e shimmers de atividade), é **expressamente proibido** utilizar `with_animation(...repeating)`. Animações GPUI repetitivas forçam re-desenho a cada frame da taxa de atualização do display (120 Hz no ProMotion do macOS), causando sobrecarga severa de GPU e consumo de energia.
- O Comet adota o relógio compartilhado `motion::pulse_delta(&spec, view, cx)` (`crates/ui/src/motion.rs:83`), operando a aproximadamente 30 fps (`PULSE_TICK = Duration::from_millis(33)`, `crates/ui/src/motion.rs:53`).
- Utiliza sistema de leasing temporizado (`PULSE_LEASE = Duration::from_millis(300)`, `crates/ui/src/motion.rs:58`): se nenhum elemento solicita frames, o relógio é desativado automaticamente.

### Modo de movimento reduzido

A flag do sistema operacional é verificada em `motion::reduced_motion(cx)`. Quando ativada, desabilita relógios de pulso, cancela interpolações de rolagem e posiciona transições instantaneamente em seu ponto final.

---

## 9. Estados e feedback

A consistência de feedback visual repousa na matriz de estados do design system e na veracidade estrita das indicações emitidas ao usuário.

### Matriz canônica de estados

1. **Normal (Rest)**: estado padrão de repouso com contraste legível.
2. **Hover**: clareamento sutil de fundo (`element_hover`) com interpolação suave de 150ms (`HOVER_FADE`).
3. **Active / Pressed**: reforço visual de clique/pressão (`element_active`).
4. **Focused**: realce de borda (`border_strong`) e anel de foco de acessibilidade.
5. **Selected**: preenchimento com `accent_wash` em listas densas ou `card_selected_bg` em itens elevados.
6. **Disabled**: opacidade reduzida a `0.35` e texto atenuado (`text_faint`), com bloqueio de eventos de ponteiro (`crates/ui/src/pickers.rs:3135`, `crates/ui/src/settings/shortcuts.rs:353`, `crates/ui/src/shell.rs:9147-9148`).
7. **Working**: exibição do spinner matricial (`busy` / `accent`), desabilitando submissões concorrentes.
8. **Warning**: realce em tom âmbar (`warning` / `warning_muted`) para alertas que demandam ação sem bloquear o fluxo.
9. **Error**: realce em tom vermelho (`danger` / `danger_muted`) para interrupções ou falhas ativas.
10. **Success**: realce em verde esmeralda (`success` / `success_muted`) para operações concluídas com integridade.

### Regras de veracidade de indicadores

- **Erro esperado não é falha**: códigos de saída normais de processos (ex.: `409: session has exited` ao encerrar um Worker CLI) são tratados como término natural do ciclo de vida e não devem emitir banners de erro na tela (`crates/ui/AGENTS.md:41`).
- **Falha transitória não fixa banner**: falhas momentâneas de resize ou polling de terminal são redefinidas no próximo ciclo com sucesso, impedindo que avisos fiquem bloqueados em cima de grades ativas (`crates/ui/AGENTS.md:45`).
- **O indicador não mente sobre o estado**: a marcação de não-lido (`unread`) é azul e destina-se estritamente a tarefas concluídas com sucesso. Sessões encerradas por erro ou mortas em `working`/`starting` renderizam `Exited`; tarefas que aguardam interação exibem `Attention` (`crates/ui/AGENTS.md:48, 83`).
- **Falha parcial transparente**: entregas com metadados incompletos (como exportações sem o índice de workers) anunciam categoricamente o estado como `Incomplete` no `SidebarNotice`, evitando passar a falsa impressão de exportação integral (`crates/ui/AGENTS.md:26`).

---

## 10. Acessibilidade

O Comet assegura acessibilidade por meio de alvos de contraste verificados matematicamente e integração direta com o subsistema AccessKit (`crates/ui/AGENTS.md:147`, `crates/theme/src/lib.rs:574-625`).

### Alvos de contraste de cores

O método `ThemeRegistry::validate` valida as paletas contra as seguintes razões mínimas de contraste WCAG:
- **Texto primário (`text` sobre `background`)**: mínimo de `4.5:1` (`crates/theme/src/lib.rs:591`).
- **Texto secundário (`text_muted` sobre `background`)**: mínimo de `4.5:1` em validação estrita e `3.0:1` em fundos com frost adverso (`crates/theme/src/lib.rs:599`, `docs/theme-system.md:32`).
- **Acento interativo (`accent` sobre `background`)**: mínimo de `3.0:1` (`crates/theme/src/lib.rs:607`).
- **Texto em placas de acento (`on_accent` sobre `accent_strong`)**: mínimo de `4.5:1` (`crates/theme/src/lib.rs:615`).
- **Texto do terminal (`terminal.foreground` sobre `terminal.background`)**: mínimo de `4.5:1` (`crates/theme/src/lib.rs:623`).

### Requisitos de árvore AccessKit

- **Identidade e Papel em Foco**: todo elemento interativo que recebe foco (`track_focus`) deve obrigatoriamente declarar `.id(...)` e `.role(...)` no GPUI.
- **Rótulos e Valores**: controles que contêm texto ou valores mutáveis devem associar `aria_label` e `aria_value`.
- **Consequência da omissão**: omitir o papel (`role`) exclui o nó da árvore do AccessKit, gerando avisos repetidos no console (`a11y: focused element … has no accessibility node`) e fazendo leitores de tela anunciarem a janela inteira em vez do elemento em foco (`crates/ui/AGENTS.md:147`).

---

## 11. QA visual

Não existe pipeline automatizado de renderização headless para componentes GPUI. A garantia de qualidade visual segue o fluxo estruturado do repositório.

### Validação por demonstração e screenshot

- O ciclo de validação visual é executado via script de demonstração local offline:
  ```bash
  scripts/dev-demo.sh
  ```
- Nenhuma alteração visual é considerada concluída sem inspeção visual real em execução do aplicativo e registro de screenshot (`crates/ui/AGENTS.md:146`).

### Chaves de captura (`capture::knob`)

Para habilitar rotas, diálogos e cenários específicos sem interação manual em ambientes de teste visual, utilizam-se variáveis de ambiente intermediadas por `capture::knob` (`crates/ui/src/capture.rs:14-19`):
- `ZERON_UI_CAPTURE=1` (chave-mestre obrigatória; se desativada, as demais variáveis são ignoradas).
- `ZERON_OPEN_ROUTE`: seleciona uma rota de visualização direta (ex.: `settings/agents`).
- `ZERON_OPEN_DIALOG`: abre imediatamente um diálogo específico.
- `ZERON_OPEN_PICKER`: inicia o app com um seletor aberto.
- `ZERON_FORCE_GATE`: força a exibição de telas de bloqueio de autenticação/organização.
- `ZERON_DEMO_UPLOAD`: injeta estado simulado de upload.
- `ZERON_DEMO_TRAJECTORY`: injeta fixtures visuais da Trajectory (`multi-run`, `error`, `narrow`, etc.).
- Proibido ler variáveis de ambiente de captura diretamente com `std::env::var`: o acesso deve passar unicamente por `capture::knob` para evitar estados retidos no terminal do desenvolvedor (`crates/ui/AGENTS.md:18`).

### Cenas de verificação (`VisualFixture`)

Antes de validar qualquer tema ou componente estrutural, devem ser revisadas as duas aparências (Light e Dark) através das 10 cenas padronizadas em [`docs/theme-system.md:159-168`](docs/theme-system.md):
1. Sidebar (navegação, espaços, sessões)
2. Transcript Markdown (títulos, listas, links)
3. Transcript Code (blocos de código, syntax highlighting)
4. Composer (pill de entrada, anexos, botões)
5. Picker / Popover (menus suspensos e paletas de comando)
6. Appearance Settings (seletor de variantes e acentos)
7. Diff (linhas adicionadas, removidas e cabeçalhos de hunk)
8. Terminal (grade de texto, seleção e paleta ANSI)
9. Empty State (telas de estado vazio)
10. Dialog (modais de confirmação e alerta)

---

## 12. Não-negociáveis / anti-padrões

Diretrizes rígidas que não admitem exceções na construção de interfaces do Comet:

1. **Dependências GPL do Zed são proibidas**: não importar as crates `markdown`, `ui`, `theme` ou `editor` do Zed. O ecossistema de markdown, tema e componentes do Comet é de autoria própria sob MIT/Apache-2.0 (`crates/ui/AGENTS.md:15`).
2. **Proibido cor fora de role**: nenhuma cor literal (`rgb`, `hsl`, `hex`) deve ser declarada inline em componentes; todo elemento deve usar os tokens providos por `Theme`.
3. **Proibido animação alterar layout**: animações são puramente camada de pintura (`paint`). É proibido usar transições que modifiquem padding, flex-basis ou dimensões que desloquem elementos adjacentes (`crates/ui/AGENTS.md:22`).
4. **Proibido altura dinâmica em listas virtualizadas**: em listas baseadas em `uniform_list` (como o ledger da Trajectory com `ROW_HEIGHT = 26.0 px`), nenhuma variação de conteúdo ou estado de dobra pode alterar a altura física das linhas (`crates/ui/src/trajectory/AGENTS.md:19`).
5. **Proibido `std::env::var` direto para knobs**: todas as chaves de teste ou captura devem consultar exclusivamente `capture::knob` sob a proteção da chave-mestre `ZERON_UI_CAPTURE` (`crates/ui/AGENTS.md:18`).
6. **Proibido ícone sem cor própria**: chamadas a `icons::icon(...)` devem receber `.text_color(...)` no próprio nó SVG; a cor do container pai não cascateia no GPUI (`crates/ui/AGENTS.md:105`).
7. **Proibido segundo dono de painel/tabs**: o painel lateral direito possui um único dono registrado (`right_tabs` / `right_active`). É vedado introduzir gerenciadores de abas concorrentes (`crates/ui/AGENTS.md:53`).
8. **Proibido duplicar assets de fonte**: referências a fontes SVG virtuais (IBM Plex Sans, Lilex) devem utilizar os aliases de `icons::Assets` apontando para os bytes embutidos de Geist e Geist Mono, sem adicionar novos arquivos de fonte ao repositório (`crates/ui/AGENTS.md:17`).

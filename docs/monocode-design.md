# Especificação de Port: Linguagem Visual MonoCode -> Comet Built-in Theme

## 1. Escopo e Proveniência

Este documento é a especificação técnica port-ready da linguagem visual do **MonoCode** para o modelo de temas built-in do **Comet** (`zeron-theme`). Ele define a tradução determinística de parâmetros visuais, superfícies, paletas de terminal e realce de sintaxe em sementes de configuração prontas para consumo pela função `variant()` em `crates/theme/src/builtins.rs:97-169`.

Este documento é a especificação; a implementação vive em `crates/theme/src/builtins.rs` (`monocode_dark()` + a família `monocode` no `builtin_registry()`) e nos campos de frost lidos por `crates/ui`. **Somente a variante escura foi implementada**, por decisão explícita do dono do produto: a família é single-variant, como `dracula` e `nord`. A tabela `Seeds` light da seção 4.2, os slots ANSI light da 5.2 e a coluna "Hex Light" da 6.1 permanecem aqui como referência de port, sem código correspondente — o seletor de tema do Comet é por aparência (`crates/ui/src/settings/appearance.rs:700-760`), então MonoCode aparece apenas na lista Dark.

Papéis de interação, `terminal_background` com alpha e frost por variante (0.85 / 24 / shell plano) já são expressáveis. Tipografia, geometria e tinting em runtime continuam abertos na seção 11 e no apêndice 12.

### 1.1 Metadados do Repositório de Origem
- **Repositório:** `hardbeat920/monocode` (clone local read-only em `/Users/guilhermevarela/Documents/Projetos/SelfHosting/monocode`)
- **Revisão:** Commit `85a5d03`
- **Versão:** `v0.1.34` (`package.json:4`)
- **Licença:** MIT Copyright (c) 2026 Nick (`LICENSE:1-3`)

### 1.2 Estrutura `ThemeSource`
O modelo do Comet (`crates/theme/src/lib.rs:436-444`) exige proveniência auditável para todo tema registrado. A declaração do built-in na chamada `source()` (`crates/theme/src/builtins.rs:215-223`) é definida da seguinte forma:

```rust
source(
    "monocode-dark",
    "builtin",
    "https://github.com/hardbeat920/monocode",
    "85a5d03",
    "MIT",
)
```

O campo `asset_hash` é gerado dinamicamente em runtime por `variant()` (`crates/theme/src/builtins.rs:165-167`), calculando o digest SHA-256 sobre a estrutura serializada em JSON da variante após a resolução das cores e do mapa de sintaxe. Na semeadura inicial via `source()`, seu valor é um marcador `"pending:monocode-dark"` ou `"pending:monocode-light"`, sendo substituído no retorno de `variant()`.

---

## 2. Mecanismo Visual do MonoCode

O MonoCode não utiliza uma paleta de cores estática tradicional de dezenas de tons isolados. Sua identidade visual é governada por quatro parâmetros fundamentais no arquivo de estilos central (`src/index.css:6-34`, `src/index.css:40-48`):

1. `--theme-hue`: matiz base (default: `240`, configurável entre `0` e `360` em `src/lib/appearance.ts:44-46`).
2. `--theme-saturation`: saturação base (default: `0%`, configurável entre `0` e `100` em `src/lib/appearance.ts:48-50`).
3. `--background-lightness`: luminosidade de fundo (`9%` no modo dark em `src/index.css:28`; `97%` no modo light em `src/index.css:41`).
4. `--content-lightness`: luminosidade de conteúdo e texto (`92%` no modo dark em `src/index.css:29`; `18%` no modo light em `src/index.css:42`).

### 2.1 As Duas Cores Fundamentais
A partir desses quatro parâmetros, o motor de estilo deriva as duas cores soberanas do app (`src/index.css:6-12`):
- `--color-background-base`: `hsl(var(--theme-hue) var(--theme-saturation) var(--background-lightness))`
- `--color-content`: `hsl(var(--theme-hue) var(--theme-saturation) var(--content-lightness))`

No padrão de fábrica (`--theme-hue: 240`, `--theme-saturation: 0%`), ambas as cores são totalmente desprovidas de croma (cinzas puros neutros), determinadas apenas pelas suas respectivas porcentagens de luminosidade.

### 2.2 Escada de Alpha sobre `content`
Quase todos os níveis de superfície, bordas, divisores e hierarquias tipográficas do MonoCode são projeções de opacidade de `--color-content` sobre o fundo, materializadas via classes utilitárias Tailwind (`bg-content/N`, `border-content/N`, `text-content/N`) ou expressões `color-mix(in srgb, var(--color-content) N%, transparent)`:
- `3%` (`bg-content/3`): fundos de caixas de entrada e composer (`src/chrome/Composer.tsx:262`, `src/chrome/Composer.tsx:1143`).
- `5%` (`bg-content/5`): popovers, pickers de arquivos, menubar e hovers padrão (`src/chrome/FilePicker.tsx:167`, `src/chrome/MenuBar.tsx:225`, `src/chrome/CwdPicker.tsx:209`).
- `6%` (`bg-content/6`): mini-cards informativos e previews (`src/chrome/FilePreview.tsx:84`, `src/chrome/HandoffMiniCard.tsx:26`, `src/chrome/InboxMiniCard.tsx:18`).
- `8%` (`bg-content/8`): chips de metadados e badges (`src/chrome/InboxMiniCard.tsx:81`).
- `10%` (`border-content/10`, `bg-content/10`): espessura de borda onipresente em toda a aplicação, botões secundários e itens ativos de listas (`src/chrome/ExplorerMenu.tsx:106`, `src/chrome/ApprovalToasts.tsx:64`, `src/chrome/CwdPicker.tsx:285`).
- `15%` (`bg-content/15`): destaques em menus suspensos e seleção de ramos (`src/chrome/BranchPicker.tsx:441`, `src/chrome/MenuBar.tsx:249`).
- `20%` (`bg-content/20`, `border-content/20`): botões selecionados e bordas enfatizadas (`src/chrome/Composer.tsx:202`, `src/chrome/ApprovalToasts.tsx:64`).
- `35% - 40%` (`text-content/35`, `text-content/40`): textos esmaecidos, placeholders e dicas desativadas (`src/chrome/GitChangesPanel.tsx:467`, `src/chrome/GitChangesPanel.tsx:935`).
- `50%` (`text-content/50`): labels secundários, contadores e legendas (`src/chrome/ProjectRail.tsx:613`, `src/chrome/FileTree.tsx:684`).
- `70%` (`text-content/70`): botões secundários ativos e texto atenuado de leitura (`src/chrome/ApprovalToasts.tsx:98`, `src/chrome/InboxFiltersMenu.tsx:197`).
- `100%` (`text-content`): texto principal da interface (`src/index.css:59`).

---

## 3. Colapso Paramétrico -> Literal

Como o modelo do Comet (`crates/theme/src/builtins.rs:74-95`) exige hexadecimais estáticos (`&str`), todo parâmetro `hsl()` e toda composição `color-mix()` devem ser projetados nos seus equivalentes hexadecimais no padrão do MonoCode (matiz `240`, saturação `0%`).

### 3.1 Resolução dos HSL Base
Com saturação $S = 0\%$, o matiz $H = 240$ é neutro e a conversão HSL para RGB reduz-se diretamente à escala linear $R = G = B = \text{round}(L \times 255)$:
- **Dark Background** ($L = 9\%$): $\text{round}(0.09 \times 255) = \text{round}(22.95) = 23 = 0\text{x}17 \rightarrow \mathbf{\#171717}$.
- **Dark Content** ($L = 92\%$): $\text{round}(0.92 \times 255) = \text{round}(234.60) = 235 = 0\text{x}eb \rightarrow \mathbf{\#ebebeb}$.
- **Light Background** ($L = 97\%$): $\text{round}(0.97 \times 255) = \text{round}(247.35) = 247 = 0\text{x}f7 \rightarrow \mathbf{\#f7f7f7}$.
- **Light Content** ($L = 18\%$): $\text{round}(0.18 \times 255) = \text{round}(45.90) = 46 = 0\text{x}2e \rightarrow \mathbf{\#2e2e2e}$.

### 3.2 Contas de Conversão de `color-mix` sobre Superfície Opaca
Para fins de auditoria e reprodutibilidade, as fórmulas matemáticas exatas aplicadas sobre as superfícies são detalhadas a seguir:

#### Caso 1: Shell Dark (`.sidebar-glass`)
Definido em `src/index.css:105`: `background: color-mix(in srgb, var(--color-background-base) 90%, black)`.
O canal RGB base é $23$ (`#171717`) e o preto é $0$:
$$\text{canal} = \text{round}(23 \times 0.90 + 0 \times 0.10) = \text{round}(20.70) = 21 = 0\text{x}15 \rightarrow \mathbf{\#151515}$$

#### Caso 2: Shell Light (`.sidebar-glass`)
Definido em `src/index.css:109`: `background: color-mix(in srgb, var(--color-background-base) 93%, black 7%)`.
O canal RGB base é $247$ (`#f7f7f7`) e o preto é $0$:
$$\text{canal} = \text{round}(247 \times 0.93 + 0 \times 0.07) = \text{round}(229.71) = 230 = 0\text{x}e6 \rightarrow \mathbf{\#e6e6e6}$$

#### Caso 3: Superfície de Card Dark
No MonoCode, cards elevados utilizam `bg-content/5` sobre o fundo (`src/chrome/FilePicker.tsx:167`).
O canal do conteúdo é $235$ (`#ebebeb`) com $\alpha = 0.05$ sobre fundo $23$ (`#171717`):
$$\text{canal} = \text{round}(235 \times 0.05 + 23 \times 0.95) = \text{round}(11.75 + 21.85) = \text{round}(33.60) = 34 = 0\text{x}22 \rightarrow \mathbf{\#222222}$$

#### Caso 4: Superfície Elevada (Raised) Dark
Mini-cards e badges de elevação utilizam `bg-content/8` (`src/chrome/InboxMiniCard.tsx:81`).
O canal do conteúdo é $235$ com $\alpha = 0.08$ sobre fundo $23$:
$$\text{canal} = \text{round}(235 \times 0.08 + 23 \times 0.92) = \text{round}(18.80 + 21.16) = \text{round}(39.96) = 40 = 0\text{x}28 \rightarrow \mathbf{\#282828}$$

#### Caso 5: Texto Secundário (Muted) Dark
O texto secundário padrão no MonoCode é `text-content/50` (`src/chrome/ProjectRail.tsx:613`).
Composição de $235$ com $\alpha = 0.50$ sobre fundo $23$:
$$\text{canal} = \text{round}(235 \times 0.50 + 23 \times 0.50) = \text{round}(117.50 + 11.50) = 129 = 0\text{x}81 \rightarrow \mathbf{\#818181}$$

#### Caso 6: Superfície de Card Light
No MonoCode, cards no modo light utilizam `bg-content/5` sobre o fundo `#f7f7f7` (`src/chrome/FilePicker.tsx:167`).
O canal do conteúdo é $46$ (`#2e2e2e`) com $\alpha = 0.05$ sobre fundo $247$ (`#f7f7f7`):
$$\text{canal} = \text{round}(46 \times 0.05 + 247 \times 0.95) = \text{round}(2.30 + 234.65) = \text{round}(236.95) = 237 = 0\text{x}ed \rightarrow \mathbf{\#ededed}$$

#### Caso 7: Superfície Elevada (Raised) Light
Mini-cards e badges no modo light utilizam `bg-content/8` sobre o fundo `#f7f7f7` (`src/chrome/InboxMiniCard.tsx:81`).
O canal do conteúdo é $46$ com $\alpha = 0.08$ sobre fundo $247$:
$$\text{canal} = \text{round}(46 \times 0.08 + 247 \times 0.92) = \text{round}(3.68 + 227.24) = \text{round}(230.92) = 231 = 0\text{x}e7 \rightarrow \mathbf{\#e7e7e7}$$

#### Caso 8: Texto Secundário (Muted) Light
O texto secundário no MonoCode utiliza `text-content/50` (`src/chrome/ProjectRail.tsx:613`).
Composição de $46$ com $\alpha = 0.50$ sobre fundo $247$:
$$\text{canal} = \text{round}(46 \times 0.50 + 247 \times 0.50) = \text{round}(23.00 + 123.50) = 146.50 = 146 = 0\text{x}92 \rightarrow \mathbf{\#929292}$$

#### Caso 9: Texto Esmaecido (Faint) Dark
Textos terciários e placeholders utilizam `text-content/35` (`src/chrome/GitChangesPanel.tsx:467`).
Composição de $235$ com $\alpha = 0.35$ sobre fundo $23$:
$$\text{canal} = \text{round}(235 \times 0.35 + 23 \times 0.65) = \text{round}(82.25 + 14.95) = \text{round}(97.20) = 97 = 0\text{x}61 \rightarrow \mathbf{\#616161}$$

#### Caso 10: Texto Esmaecido (Faint) Light
Textos terciários e secundários de arquivo utilizam `text-content/40` (`src/chrome/GitChangesPanel.tsx:935`).
Composição de $46$ com $\alpha = 0.40$ sobre fundo $247$:
$$\text{canal} = \text{round}(46 \times 0.40 + 247 \times 0.60) = \text{round}(18.40 + 148.20) = \text{round}(166.60) = 167 = 0\text{x}a7 \rightarrow \mathbf{\#a7a7a7}$$

---

## 4. Tabela `Seeds` — Dark e Light

O contrato de semeadura do Comet (`crates/theme/src/builtins.rs:74-95`) requer exatamente 12 campos de cor por variante. A tabela a seguir documenta o valor semeado, a justificativa rastreável no código do MonoCode, o valor efetivo após a passagem por `variant()` (`crates/theme/src/builtins.rs:97-169`) e o delta decorrente dos filtros de contraste da engine.

### 4.1 Variante Dark (`monocode-dark`)

| Campo | Hex Semeado | Origem / Justificativa no MonoCode | Efetivo pós-`variant()` | Delta de Endurecimento |
|---|---|---|---|---|
| `background` | `#171717` | `src/index.css:28` (`--background-lightness: 9%`) | `#171717` | Nenhum |
| `shell` | `#171717` | `src/index.css:98-102`, `:114-119` (`html.has-native-glass .sidebar-glass`) | `#171717` | Nenhum |
| `raised` | `#282828` | `src/chrome/InboxMiniCard.tsx:81` (`bg-content/8` sobre `#171717`) | `#282828` | Nenhum |
| `card` | `#222222` | `src/chrome/FilePicker.tsx:167` (`bg-content/5` sobre `#171717`) | `#222222` | Nenhum |
| `text` | `#ebebeb` | `src/index.css:29` (`--content-lightness: 92%`) | `#ebebeb` | Nenhum |
| `muted` | `#818181` | `src/chrome/ProjectRail.tsx:613` (`text-content/50` sobre `#171717`) | `#818181` | Nenhum (contraste 4.60:1 $\ge$ 4.5) |
| `faint` | `#616161` | `src/chrome/GitChangesPanel.tsx:467` (`text-content/35` sobre `#171717`) | `#616161` | Nenhum |
| `accent` | `#459bf7` | `src/index.css:13` (`--color-accent` base) | `#459bf7` | Nenhum (contraste 6.22:1 $\ge$ 3.0) |
| `danger` | `#f87171` | `src/surfaces/TerminalView.tsx:45` (`ANSI_DARK.red` / `red-400`) | `#f87171` | Nenhum |
| `warning` | `#fbbf24` | `src/surfaces/TerminalView.tsx:47` (`ANSI_DARK.yellow` / `amber-400`) | `#fbbf24` | Nenhum |
| `success` | `#4ade80` | `src/surfaces/TerminalView.tsx:46` (`ANSI_DARK.green` / `green-400`) | `#4ade80` | Nenhum |
| `terminal_background` | `#17171700` | `src/surfaces/TerminalView.tsx:100-102` (`#00000000` no canvas; seed na hue do canvas para flatten honesto) | `#17171700` (alpha 0) | Nenhum — `terminal.foreground` endurece contra o canvas achatado |

*Correção de proveniência do `shell` (medida na tela, macOS):* a regra opaca `color-mix(in srgb, var(--color-background-base) 90%, black)` de `src/index.css:104-106` é **fallback de plataforma sem vidro nativo**. No macOS quem pinta é `html.has-native-glass .sidebar-glass` (`:114-119`), que usa `background` a `var(--sidebar-opacity)` = 85% sobre uma janela cujo fundo foi zerado (`:98-102`) — a sidebar do MonoCode nunca fica **mais escura** que o canvas; medida em `#191919` sobre desktop escuro. Semear `#151515` invertia o sinal. Como alpha por superfície não existe no modelo do Comet, o flatten determinístico e independente do papel de parede é o próprio `background`, `#171717`.

### 4.2 Variante Light (`monocode-light`)

| Campo | Hex Semeado | Origem / Justificativa no MonoCode | Efetivo pós-`variant()` | Delta de Endurecimento |
|---|---|---|---|---|
| `background` | `#f7f7f7` | `src/index.css:41` (`--background-lightness: 97%`) | `#f7f7f7` | Nenhum |
| `shell` | `#e6e6e6` | `src/index.css:109` (`color-mix` 93% bg, 7% black na sidebar) | `#e6e6e6` | Nenhum |
| `raised` | `#e7e7e7` | `src/chrome/InboxMiniCard.tsx:81` (`bg-content/8` sobre `#f7f7f7`) | `#e7e7e7` | Nenhum |
| `card` | `#ededed` | `src/chrome/FilePicker.tsx:167` (`bg-content/5` sobre `#f7f7f7`) | `#ededed` | Nenhum |
| `text` | `#2e2e2e` | `src/index.css:42` (`--content-lightness: 18%`) | `#2e2e2e` | Nenhum |
| `muted` | `#929292` | `src/chrome/ProjectRail.tsx:613` (`text-content/50` sobre `#f7f7f7`) | `#6e6e6e` | Ajustado de `#929292` (2.90:1) para `#6e6e6e` (4.76:1) por `ensure_contrast` |
| `faint` | `#a7a7a7` | `src/chrome/GitChangesPanel.tsx:935` (`text-content/40` sobre `#f7f7f7`) | `#a7a7a7` | Nenhum |
| `accent` | `#4078f2` | `src/surfaces/TerminalView.tsx:86` (cursor light e `ANSI_LIGHT.blue`) | `#4078f2` | Nenhum (contraste 3.78:1 $\ge$ 3.0) |
| `danger` | `#e45649` | `src/surfaces/TerminalView.tsx:65` (`ANSI_LIGHT.red`) | `#e45649` | Nenhum |
| `warning` | `#c18401` | `src/surfaces/TerminalView.tsx:67` (`ANSI_LIGHT.yellow`) | `#c18401` | Nenhum |
| `success` | `#50a14f` | `src/surfaces/TerminalView.tsx:66` (`ANSI_LIGHT.green`) | `#50a14f` | Nenhum |
| `terminal_background` | `#fafafa` | `src/surfaces/TerminalView.tsx:105` (`OSC_LIGHT.bg`) | `#fafafa` | Nenhum |

*Nota sobre superfícies sem mapeamento 1:1:* O MonoCode não possui conceitos fixos chamados `shell`, `raised` ou `card`. O `shell` foi ancorado na barra lateral (`.sidebar-glass` em `src/index.css:104-110`), que delimita a casca estrutural externa. O `card` foi mapeado para a elevação de popovers e diálogos flutuantes (`bg-content/5` em `src/chrome/FilePicker.tsx:167`). O `raised` foi mapeado para badges, chips e cartões secundários de destaque (`bg-content/8` em `src/chrome/InboxMiniCard.tsx:81`).

---

## 5. ANSI 16

O MonoCode implementa sua emulação de terminal via `@xterm/xterm` no arquivo `src/surfaces/TerminalView.tsx:43-80`. As tabelas a seguir trazem os 16 slots exatos de cada modo, além do fundo oficial retornado nas sequências OSC 10/11 (`src/surfaces/TerminalView.tsx:103-109`).

### 5.1 Slots ANSI Dark (`ANSI_DARK` em `src/surfaces/TerminalView.tsx:43-60`)

| Slot | Nome do Slot | Hex | Origem no MonoCode |
|---|---|---|---|
| 0 | Normal Black | `#1d2428` | `src/surfaces/TerminalView.tsx:44` |
| 1 | Normal Red | `#f87171` | `src/surfaces/TerminalView.tsx:45` |
| 2 | Normal Green | `#4ade80` | `src/surfaces/TerminalView.tsx:46` |
| 3 | Normal Yellow | `#fbbf24` | `src/surfaces/TerminalView.tsx:47` |
| 4 | Normal Blue | `#60a5fa` | `src/surfaces/TerminalView.tsx:48` |
| 5 | Normal Magenta | `#c084fc` | `src/surfaces/TerminalView.tsx:49` |
| 6 | Normal Cyan | `#22d3ee` | `src/surfaces/TerminalView.tsx:50` |
| 7 | Normal White | `#e8eef2` | `src/surfaces/TerminalView.tsx:51` |
| 8 | Bright Black | `#64748b` | `src/surfaces/TerminalView.tsx:52` |
| 9 | Bright Red | `#fca5a5` | `src/surfaces/TerminalView.tsx:53` |
| 10 | Bright Green | `#86efac` | `src/surfaces/TerminalView.tsx:54` |
| 11 | Bright Yellow | `#fde68a` | `src/surfaces/TerminalView.tsx:55` |
| 12 | Bright Blue | `#93c5fd` | `src/surfaces/TerminalView.tsx:56` |
| 13 | Bright Magenta | `#d8b4fe` | `src/surfaces/TerminalView.tsx:57` |
| 14 | Bright Cyan | `#67e8f9` | `src/surfaces/TerminalView.tsx:58` |
| 15 | Bright White | `#f8fafc` | `src/surfaces/TerminalView.tsx:59` |
| - | `terminal_background` | `#17171700` | canvas hue at zero alpha; the OSC `#00000000` flattened onto `#171717` |

### 5.2 Slots ANSI Light (`ANSI_LIGHT` em `src/surfaces/TerminalView.tsx:63-80`)

| Slot | Nome do Slot | Hex | Origem no MonoCode |
|---|---|---|---|
| 0 | Normal Black | `#383a42` | `src/surfaces/TerminalView.tsx:64` |
| 1 | Normal Red | `#e45649` | `src/surfaces/TerminalView.tsx:65` |
| 2 | Normal Green | `#50a14f` | `src/surfaces/TerminalView.tsx:66` |
| 3 | Normal Yellow | `#c18401` | `src/surfaces/TerminalView.tsx:67` |
| 4 | Normal Blue | `#4078f2` | `src/surfaces/TerminalView.tsx:68` |
| 5 | Normal Magenta | `#a626a4` | `src/surfaces/TerminalView.tsx:69` |
| 6 | Normal Cyan | `#0184bc` | `src/surfaces/TerminalView.tsx:70` |
| 7 | Normal White | `#fafafa` | `src/surfaces/TerminalView.tsx:71` |
| 8 | Bright Black | `#7c8591` | `src/surfaces/TerminalView.tsx:72` |
| 9 | Bright Red | `#df6b60` | `src/surfaces/TerminalView.tsx:73` |
| 10 | Bright Green | `#68b567` | `src/surfaces/TerminalView.tsx:74` |
| 11 | Bright Yellow | `#d19a2f` | `src/surfaces/TerminalView.tsx:75` |
| 12 | Bright Blue | `#5c89f5` | `src/surfaces/TerminalView.tsx:76` |
| 13 | Bright Magenta | `#b54bb3` | `src/surfaces/TerminalView.tsx:77` |
| 14 | Bright Cyan | `#1f9cc9` | `src/surfaces/TerminalView.tsx:78` |
| 15 | Bright White | `#ffffff` | `src/surfaces/TerminalView.tsx:79` |
| - | `terminal_background` | `#fafafa` | `src/surfaces/TerminalView.tsx:105` (`OSC_LIGHT.bg`) |

---

## 6. Syntax 12

O destructuring da função `syntax()` em `crates/theme/src/builtins.rs:171-185` consome exatamente 12 cores de entrada para popular a coleção `BTreeMap<String, Color>` com 24 chaves sintáticas. No MonoCode, os estilos de código são orquestrados em `src/surfaces/editorLanguage.ts:10-99` e importados em `src/surfaces/syntaxTokens.ts:10-12`.

### 6.1 Os 12 Slots de Entrada na Ordem Exata

| Índice | Parâmetro em `builtins.rs` | Hex Dark | Hex Light | Origem no MonoCode |
|---|---|---|---|---|
| 0 | `comment` | `#fefdc2` | `#8a9199` | `src/surfaces/editorLanguage.ts:82,94` (`HIGHLIGHT_PALETTE.comment`) |
| 1 | `keyword` | `#ff8ffd` | `#a626a4` | `src/surfaces/editorLanguage.ts:76,88` (`HIGHLIGHT_PALETTE.keyword`) |
| 2 | `string` | `#b4fa72` | `#50a14f` | `src/surfaces/editorLanguage.ts:79,91` (`HIGHLIGHT_PALETTE.string`) |
| 3 | `number` | `#b4fa72` | `#986801` | `src/surfaces/editorLanguage.ts:81,93` (`HIGHLIGHT_PALETTE.number`) |
| 4 | `type_name` | `#ff8272` | `#c18401` | `src/surfaces/editorLanguage.ts:80,92` (`HIGHLIGHT_PALETTE.type`) |
| 5 | `function` | `#a5d5fe` | `#4078f2` | `src/surfaces/editorLanguage.ts:78,90` (`HIGHLIGHT_PALETTE.callable`) |
| 6 | `property` | `#d0d1fe` | `#e45649` | `src/surfaces/editorLanguage.ts:83,95` (`HIGHLIGHT_PALETTE.property`) |
| 7 | `variable` | `#ebebeb` | `#2e2e2e` | `src/surfaces/editorChrome.ts:19` (variáveis não estilizas herdam `--color-content`) |
| 8 | `punctuation` | `#ebebeb` | `#2e2e2e` | `src/surfaces/editorChrome.ts:19` (pontuação herda `--color-content`) |
| 9 | `tag` | `#ff8272` | `#c18401` | `src/surfaces/editorLanguage.ts:43` (`tags.tagName` mapeado no grupo `type`) |
| 10 | `attribute` | `#d0d1fe` | `#e45649` | `src/surfaces/editorLanguage.ts:48` (`tags.attributeName` mapeado no grupo `property`) |
| 11 | `invalid` | `#ffc4bd` | `#cf222e` | `src/surfaces/editorLanguage.ts:85,97` (`HIGHLIGHT_PALETTE.invalid`) |

### 6.2 Mapeamento Final das 24 Chaves do `BTreeMap`
Em `crates/theme/src/builtins.rs:186-212`, as 12 cores semeadas são distribuídas nas 24 chaves da gramática de realce. Note que várias chaves compartilham a mesma cor original:
- `keyword` reutilizado para: `macro`, `variableSpecial`, `operator`.
- `string` isolado para literais de texto.
- `attribute` reutilizado para: `stringSpecial`, `escape`.
- `number` reutilizado para: `boolean`, `constant`.
- `type_name` reutilizado para: `typeBuiltin`, `constructor`.
- `function` reutilizado para: `functionBuiltin`, `label`.
- `property` isolado para propriedades de objetos.
- `variable` reutilizado para: `parameter`.
- `punctuation` reutilizado para: `embedded`.
- `tag`, `attribute` e `invalid` mantêm papéis únicos.

---

## 7. Accent e seus 8 Papéis

O MonoCode define `--color-accent: hsl(211 92% 62%)` (`src/index.css:13`), que colapsa para `#459bf7`. No modo light, enquanto o CSS não define um `--color-accent` exclusivo, o runtime do terminal (`src/surfaces/TerminalView.tsx:86`) e o slot azul ANSI utilizam `#4078f2` para garantir legibilidade contra fundos claros.

### 7.1 Derivação de `AccentRoles`
No modelo do Comet (`crates/theme/src/lib.rs:353-400`), os 8 papéis de destaque nascem da função `AccentRoles::derive(primary, appearance, background)`. A tabela abaixo apresenta os valores exatos produzidos:

| Papel de Accent | Fórmula / Implementação no Comet | Hex Dark (`primary`: `#459bf7`) | Hex Light (`primary`: `#4078f2`) |
|---|---|---|---|
| `primary` | `primary.ensure_contrast(bg, 3.0)` | `#459bf7` | `#4078f2` |
| `strong` | `primary.ensure_contrast(on, 4.5)` | `#459bf7` | `#4078f2` |
| `wash` | `primary.with_alpha(dark ? 0.22 : 0.12)` | `#459bf738` | `#4078f21f` |
| `on` | `primary.best_on_color()` | `#000000` | `#000000` |
| `selection` | `primary.with_alpha(dark ? 0.35 : 0.24)` | `#459bf759` | `#4078f23d` |
| `caret` | `primary` | `#459bf7` | `#4078f2` |
| `activity` | `primary` | `#459bf7` | `#4078f2` |
| `glyph[0]` (light) | `primary.mix(white/bg, 0.28/0.18)` | `#79b7f9` | `#618ff3` |
| `glyph[1]` (prim) | `primary` | `#459bf7` | `#4078f2` |
| `glyph[2]` (deep) | `primary.mix(black, 0.18/0.26)` | `#397fcb` | `#2f59b3` |

*Comportamento de `on`:* Como a luminância de `#459bf7` ($L \approx 0.31$) e `#4078f2` ($L \approx 0.23$) é suficientemente clara, o contraste contra o preto `#000000` (7.28:1 e 5.19:1) supera o contraste contra o branco `#ffffff` (2.88:1 e 4.04:1). Portanto, `best_on_color()` seleciona corretamente `#000000` para ambos.

---

## 8. Campos Derivados: MonoCode × `variant()`

A função `variant()` (`crates/theme/src/builtins.rs:97-169`) deriva 17 campos estruturais que não podem ser semeados. Esta seção compara o comportamento nativo do MonoCode com o que o Comet sintetizará, documentando as convergências e divergências visuais.

### 8.1 Comparativo de Campos Derivados

| Campo Derivado | Fórmula de `variant()` no Comet | Valor Dark | Valor Light | Comportamento no MonoCode | Diagnóstico de Paridade |
|---|---|---|---|---|---|
| `dialog` | `card.mix(raised, dark ? 0.18 : 0.04)` | `#232323` | `#ededed` | Diálogos e popovers usam `bg-content/5` (`#222222`) com `backdrop-blur-xl` (`src/chrome/FilePicker.tsx:167`) | **Quase idêntico** ($\Delta \approx 1$ unidade RGB no dark; idêntico no light). |
| `overlay` | `card.mix(raised, dark ? 0.34 : 0.02)` | `#242424` | `#ededed` | Backdrops usam animação `modal-backdrop-in` com opacidade total esmaecida (`src/index.css:1028`) | **Divergência conceitual**: Comet trata overlay como cor de superfície sólida intermediária. |
| `hover` | `border_tone.with_alpha(dark ? 0.11 : 0.06)` | `#ffffff1c` | `#0000000f` | MonoCode aplica `hover:bg-content/5` (`#ffffff0d` dark, `#0000000d` light em `src/chrome/CwdPicker.tsx:209`) | **Divergência sutil**: o hover do Comet é ligeiramente mais nítido no dark (11% vs 5%). |
| `active` | `accent.primary.with_alpha(dark ? 0.18 : 0.10)` | `#459bf72e` | `#4078f21a` | MonoCode usa `bg-content/10` para itens ativos (`src/chrome/CwdPicker.tsx:285`) ou `bg-accent/25` no autocomplete | **Divergência de intenção**: Comet colore o estado ativo com o matiz do accent; MonoCode usa o tom neutro de `content`. |
| `border` | `border_tone.with_alpha(dark ? 0.10 : 0.12)` | `#ffffff1a` | `#0000001f` | MonoCode usa `border-content/10` universal (`#ffffff1a` dark, `#0000001a` light em `src/chrome/FilePicker.tsx:167`) | **Paridade exata** no Dark (`#ffffff1a` = 10% alpha); divergência mínima de 2% no Light. |
| `border_strong` | `border_tone.with_alpha(dark ? 0.18 : 0.22)` | `#ffffff2e` | `#00000038` | MonoCode usa `border-content/20` (`#ffffff33` dark, `#00000033` light em `src/chrome/ApprovalToasts.tsx:64`) | **Paridade visual próxima** (18% vs 20% no dark; 22% vs 20% no light). |
| `solid` | Cor fixa `#ebebef` (dark) / `#232328` (light) | `#ebebef` | `#232328` | Botões primários usam `bg-content` (`#ebebeb`) e texto `text-background-base` (`src/chrome/ApprovalToasts.tsx:91`) | **Paridade excelente**: a cor fixa do Comet replica com precisão a inversão preto-no-branco do MonoCode. |
| `on_solid` | `solid.best_on_color()` | `#000000` | `#ffffff` | Texto dos botões primários: `#171717` (dark) e `#ffffff` (light) (`src/chrome/GitChangesPanel.tsx:489`) | **Paridade quase total**: preto puro `#000000` vs cinza escuro `#171717`. |
| `danger_muted` | `danger.mix(text, 0.28)` | `#f49393` | `#b14b41` | Badges e toques de erro usam `bg-red-500/20` com `text-red-300` (`src/chrome/ExplorerMenu.tsx:137`) | **Divergência estética**: Comet mistura tinta com o texto; MonoCode usa camadas alfa translúcidas. |
| `warning_muted` | `warning.mix(text, 0.25)` | `#f7ca56` | `#9c6e0c` | MonoCode usa `bg-yellow-300/12` com `text-yellow-200/90` (`src/chrome/Composer.tsx:1336`) | **Divergência estética**: tonalidade sólida no Comet vs wash transparente no MonoCode. |
| `success_muted` | `success.mix(text, 0.25)` | `#72e19b` | `#488447` | MonoCode usa `bg-emerald-400/20` com `text-emerald-300` (`src/chrome/TaskListPreview.tsx:69`) | **Divergência estética**: tonalidade sólida no Comet vs wash transparente no MonoCode. |
| `input` | `dark ? raised.with_alpha(0.72) : card` | `#282828b8` | `#ededed` | Inputs usam `bg-content/5` ou `bg-content/10` (`src/chrome/GitChangesPanel.tsx:467`) | **Divergência moderada**: Comet usa superfície elevada semiopaca; MonoCode usa wash do conteúdo. |
| `cursor` | `text.with_alpha(dark ? 0.40 : 0.55)` | `#ebebeb66` | `#2e2e2e8c` | MonoCode usa `caretColor: var(--color-content)` no editor (`src/surfaces/editorChrome.ts:35`) e accent no terminal | **Divergência visual**: cursor semitransparente no Comet vs sólido no MonoCode. |
| `diff_add` | `success` | `#4ade80` | `#50a14f` | MonoCode usa `text-emerald-400` em diffs (`src/chrome/FilePreview.tsx:107`) | **Paridade funcional completa**. |
| `diff_delete` | `danger` | `#f87171` | `#e45649` | MonoCode usa `text-red-400` em diffs (`src/chrome/FilePreview.tsx:111`) | **Paridade funcional completa**. |
| `diff_hunk` | `accent.primary.with_alpha(dark ? 0.08 : 0.07)` | `#459bf714` | `#4078f212` | Botões de hunk no CodeMirror usam outline de accent (`src/surfaces/editorGit.ts:1142`) | **Paridade estética alinhada**. |
| `terminal.foreground`| `text.ensure_contrast(terminal_bg, 4.5)` | `#ebebeb` | `#2e2e2e` | `src/surfaces/TerminalView.tsx:85` usa `var(--color-content)` (`#ebebeb` e `#2e2e2e`) | **Paridade exata**. |
| `terminal.selection` | `border_tone.with_alpha(dark ? 0.22 : 0.16)` | `#ffffff38` | `#00000029` | MonoCode usa `rgba(255,255,255,0.18)` e `rgba(0,0,0,0.18)` (`src/surfaces/TerminalView.tsx:88`) | **Paridade muito próxima** (22% vs 18% no dark; 16% vs 18% no light). |

---

## 9. Contraste Medido

A validação do tema no Comet é executada por `ThemeRegistry::validate()` (`crates/theme/src/lib.rs:574-652`), que afere os 5 gates de contraste WCAG 2.1 (luminância relativa calculada segundo a especificação W3C) e os 14 slots cromáticos de terminal.

### 9.1 Os 5 Gates de Validação Obrigatórios

| Gate de Validação | Papéis Comparados | Dark Ratio | Dark Veredito | Light Ratio | Light Veredito |
|---|---|---|---|---|---|
| Gate 1 | `text` vs `background` ($\ge 4.5$) | 15.04:1 | **PASS** | 12.68:1 | **PASS** |
| Gate 2 | `text_muted` vs `background` ($\ge 4.5$) | 4.60:1 | **PASS** | 4.76:1 | **PASS** |
| Gate 3 | `accent.primary` vs `background` ($\ge 3.0$) | 6.22:1 | **PASS** | 3.78:1 | **PASS** |
| Gate 4 | `accent.on` vs `accent.strong` ($\ge 4.5$) | 7.28:1 | **PASS** | 5.19:1 | **PASS** |
| Gate 5 | `terminal.foreground` vs `terminal.background` ($\ge 4.5$) | 14.60:1 | **PASS** | 13.01:1 | **PASS** |

### 9.2 Os 14 Slots Cromáticos de Terminal ANSI contra `terminal_background`

| Slot ANSI | Cor Dark | Dark Ratio | Dark Veredito | Cor Light | Light Ratio | Light Veredito |
|---|---|---|---|---|---|---|
| Slot 1 (Red) | `#f87171` | 6.29:1 | **PASS** | `#e45649` | 3.51:1 | **PASS** |
| Slot 2 (Green) | `#4ade80` | 9.99:1 | **PASS** | `#50a14f` | 3.07:1 | **PASS** |
| Slot 3 (Yellow) | `#fbbf24` | 10.43:1 | **PASS** | `#c18401` | 3.06:1 | **PASS** |
| Slot 4 (Blue) | `#60a5fa` | 6.85:1 | **PASS** | `#4078f2` | 3.88:1 | **PASS** |
| Slot 5 (Magenta) | `#c084fc` | 6.59:1 | **PASS** | `#a626a4` | 5.86:1 | **PASS** |
| Slot 6 (Cyan) | `#22d3ee` | 9.63:1 | **PASS** | `#0184bc` | 4.00:1 | **PASS** |
| Slot 7 (White) | `#e8eef2` | 14.87:1 | **PASS** | `#fafafa` | 1.00:1 | **WARNING (< 3.0)** |
| Slot 9 (Bright Red) | `#fca5a5` | 9.17:1 | **PASS** | `#df6b60` | 3.14:1 | **PASS** |
| Slot 10 (Bright Green) | `#86efac` | 12.40:1 | **PASS** | `#68b567` | 2.40:1 | **WARNING (< 3.0)** |
| Slot 11 (Bright Yellow) | `#fde68a` | 13.98:1 | **PASS** | `#d19a2f` | 2.40:1 | **WARNING (< 3.0)** |
| Slot 12 (Bright Blue) | `#93c5fd` | 9.65:1 | **PASS** | `#5c89f5` | 3.17:1 | **PASS** |
| Slot 13 (Bright Magenta) | `#d8b4fe` | 9.85:1 | **PASS** | `#b54bb3` | 4.35:1 | **PASS** |
| Slot 14 (Bright Cyan) | `#67e8f9` | 12.01:1 | **PASS** | `#1f9cc9` | 3.02:1 | **PASS** |
| Slot 15 (Bright White) | `#f8fafc` | 16.64:1 | **PASS** | `#ffffff` | 1.04:1 | **WARNING (< 3.0)** |

*Nota dos Warnings ANSI:* Em `monocode-light`, os slots 7, 10, 11 e 15 geram advertências (`ValidationIssue::contrast_warning`) por ficarem abaixo de 3.0:1 contra `#fafafa`. Esse comportamento reflete fielmente a paleta One Light herdada pelo MonoCode (`src/surfaces/TerminalView.tsx:62-80`), onde o branco e os tons claros pastel têm baixo contraste sobre fundo quase branco. No Comet, advertências não impedem o registro nem violam os requisitos estruturais.

### 9.3 Output Literal do Script de Cálculo

```text
==================== VALIDATION GATES: Dark ====================
1. text (#ebebeb) vs bg (#171717): 15.04:1 -> PASS
2. muted seeded (#818181 ratio 4.60:1) -> effective (#818181 ratio 4.60:1) -> PASS
3. accent.primary seeded (#459bf7 ratio 6.22:1) -> effective (#459bf7 ratio 6.22:1) vs bg (#171717) -> PASS
4. accent.on (#000000) vs accent.strong (#459bf7): 7.28:1 -> PASS
5. terminal.foreground (#ebebeb) vs terminal.background (#141b1f): 14.60:1 -> PASS

--- 14 Chromatic ANSI Slots vs Terminal Background ---
Slot  1 (1: red - #f87171): 6.29:1 -> PASS
Slot  2 (2: green - #4ade80): 9.99:1 -> PASS
Slot  3 (3: yellow - #fbbf24): 10.43:1 -> PASS
Slot  4 (4: blue - #60a5fa): 6.85:1 -> PASS
Slot  5 (5: magenta - #c084fc): 6.59:1 -> PASS
Slot  6 (6: cyan - #22d3ee): 9.63:1 -> PASS
Slot  7 (7: white - #e8eef2): 14.87:1 -> PASS
Slot  9 (9: brightRed - #fca5a5): 9.17:1 -> PASS
Slot 10 (10: brightGreen - #86efac): 12.40:1 -> PASS
Slot 11 (11: brightYellow - #fde68a): 13.98:1 -> PASS
Slot 12 (12: brightBlue - #93c5fd): 9.65:1 -> PASS
Slot 13 (13: brightMagenta - #d8b4fe): 9.85:1 -> PASS
Slot 14 (14: brightCyan - #67e8f9): 12.01:1 -> PASS
Slot 15 (15: brightWhite - #f8fafc): 16.64:1 -> PASS

==================== VALIDATION GATES: Light ====================
1. text (#2e2e2e) vs bg (#f7f7f7): 12.68:1 -> PASS
2. muted seeded (#929292 ratio 2.90:1) -> effective (#6e6e6e ratio 4.76:1) -> PASS
   [HARDENING DELTA] Muted was adjusted from #929292 to #6e6e6e!
3. accent.primary seeded (#4078f2 ratio 3.78:1) -> effective (#4078f2 ratio 3.78:1) vs bg (#f7f7f7) -> PASS
4. accent.on (#000000) vs accent.strong (#4078f2): 5.19:1 -> PASS
5. terminal.foreground (#2e2e2e) vs terminal.background (#fafafa): 13.01:1 -> PASS

--- 14 Chromatic ANSI Slots vs Terminal Background ---
Slot  1 (1: red - #e45649): 3.51:1 -> PASS
Slot  2 (2: green - #50a14f): 3.07:1 -> PASS
Slot  3 (3: yellow - #c18401): 3.06:1 -> PASS
Slot  4 (4: blue - #4078f2): 3.88:1 -> PASS
Slot  5 (5: magenta - #a626a4): 5.86:1 -> PASS
Slot  6 (6: cyan - #0184bc): 4.00:1 -> PASS
Slot  7 (7: white - #fafafa): 1.00:1 -> WARNING (< 3.0)
Slot  9 (9: brightRed - #df6b60): 3.14:1 -> PASS
Slot 10 (10: brightGreen - #68b567): 2.40:1 -> WARNING (< 3.0)
Slot 11 (11: brightYellow - #d19a2f): 2.40:1 -> WARNING (< 3.0)
Slot 12 (12: brightBlue - #5c89f5): 3.17:1 -> PASS
Slot 13 (13: brightMagenta - #b54bb3): 4.35:1 -> PASS
Slot 14 (14: brightCyan - #1f9cc9): 3.02:1 -> PASS
Slot 15 (15: brightWhite - #ffffff): 1.04:1 -> WARNING (< 3.0)
```

---

## 10. `recommended_surface_treatment`

A escolha do campo `recommended_surface_treatment` em `Seeds` define a preferência visual sugerida ao runtime (`crates/theme/src/builtins.rs:79`, `crates/theme/src/lib.rs:59-65`).

### 10.1 Decisão: `SurfaceTreatment::Frosted`
A recomendação mandatória para o port do MonoCode é **`SurfaceTreatment::Frosted`**.

**Evidência no MonoCode:**
1. A identidade estética central do MonoCode é o vidro translúcido com desfoque nativo (`.sidebar-glass` e `.body-glass` em `src/index.css:104-121`).
2. A configuração de fábrica ativa vidro por padrão: `BODY_GLASS_DEFAULT = true` (`src/lib/appearance.ts:64`), `SIDEBAR_OPACITY_DEFAULT = 0.85` (`src/lib/appearance.ts:54`) e `SIDEBAR_BLUR_DEFAULT = 24` (`src/lib/appearance.ts:58`).
3. Componentes centrais contêm desfoque explícito no compositor web: `backdrop-blur-xl` em `src/chrome/ApprovalToasts.tsx:64`, `src/chrome/FilePicker.tsx:167` e `src/chrome/FileMentionPicker.tsx:62`.

### 10.2 Elegibilidade por Plataforma no Comet
No Comet, a materialização de superfícies foscas depende do suporte do compositor do sistema operacional. Em `crates/ui/src/theme.rs:794-797`, o método `is_frost()` restringe o efeito:

```rust
pub fn is_frost(&self) -> bool {
    self.surface_treatment == SurfaceTreatment::Frosted
        && cfg!(any(target_os = "macos", target_os = "linux"))
}
```

Ou seja, em macOS e Linux, o Comet ativa as camadas de desfoque e translucidez do GPUI. No Windows, o compositor GPUI não possui backend para frost e retrocede silenciosamente para renderização opaca através de `glass()` (`crates/ui/src/theme.rs:734-737`), garantindo que não ocorram artefatos gráficos.

### 10.3 Frost por variante
`ThemeVariant` declara `frost_alpha = 0.85`, `frost_blur_radius = 24.0` e `flat_shell = true` em `monocode-dark`. O renderer lê esses campos com `GLASS_ALPHA` / `MENU_BLUR` / `wash(0.05)` como fallback para variantes que não declaram. A janela pede `Window::set_background_blur_radius(Some(px(24.0)))` só quando o variante declara e a plataforma tem frost; sem declaração o gpui permanece no material AppKit. Off-macOS o frost continua opaco, independente do que a variante peça. Usuários que preferirem casca sólida continuam respaldados pela política local `SurfacePreference::Opaque` (`crates/theme/src/lib.rs:71-87`).

---

## 11. GAPs

Fechados nesta change: papéis de interação (hover/active/border/input/cursor/diff_hunk/terminal.selection) como overrides com alpha; `terminal_background` translúcido; frost alpha 0.85, blur 24 e shell plano sem o `wash(0.05)` extra da sidebar; o vidro da **janela** em `monocode-dark`, que deixa de usar o material AppKit `UnderWindowBackground` (downsample do backdrop) e pede raio WindowServer 24. Sobre o mesmo backdrop a média de luminância é praticamente a mesma (92,4 vs 90,4), mas o WindowServer deixa passar **18,1×** mais energia de alta frequência (0,547 vs 0,030). Variante sem raio declarado continua no material — o comportamento dos outros temas não muda.

Ainda abertos:

1. **Sliders de Matiz e Saturação em Tempo Real:** Os seletores de personalização em `src/lib/appearance.ts:107-153` alteram dinamicamente `--theme-hue` e `--theme-saturation` no elemento raiz. O port para o Comet assume estritamente o ponto de calibração padrão (`240` / `0%`).
2. **Biblioteca de Ícones Externa:** O MonoCode depende de ícones vetoriais com pesos de traço próprios via `@hugeicons/core-free-icons` (`src/chrome/icons.tsx:29-33`) e do tema de ícones de arquivos `react-material-icon-theme` (`package.json:51`). O Comet possui seu próprio repositório de glifos no GPUI.
3. **Animações de Transição de Camada:** O MonoCode define curvas Bézier cúbicas customizadas para a abertura de popovers e diálogos (`src/index.css:966-1033`: `popover-open` em 170ms com `cubic-bezier(0.16, 1, 0.3, 1)` e `modal-panel-in` em 200ms). O modelo `zeron-theme` não armazena timings nem interpolações de movimento.
4. **Tipografia e geometria por tema:** escala 13px/1.6, titlebar 28px, raios 6/8/12px e densidade — apêndice 12. Fora desta change.

---

## 12. Apêndice: Fora do Tema

A identidade visual percebida do MonoCode é fruto tanto de suas cores quanto de sua arquitetura de layout e densidade espacial. Registra-se para histórico os aspectos estruturais que o modelo `zeron-theme` **não expressa**, e que dependeriam de intervenção na camada de renderização (`crates/ui`):

1. **Escala Tipográfica e Densidade:**
   - O editor do MonoCode é fixado em `13px` com `lineHeight: 1.6` (`src/surfaces/editorChrome.ts:20,30`).
   - A barra de menus opera em `12px` com altura condensada de `28px` (`src/chrome/MenuBar.tsx:225`).
   - Chips, contadores e badges utilizam tipografia de `10px` e `11px` com fontes tabulares monoespaçadas (`src/chrome/UsageFooter.tsx:247`, `src/chrome/ApprovalToasts.tsx:77`).
2. **Arquitetura de Navegação Dual-Sidebar:**
   - O MonoCode emprega uma coluna estreita de alternância de projeto chamada *Project Rail* (`src/lib/appearance.ts:60-62`), com largura de `200px` configurável (`PROJECT_RAIL_WIDTH_DEFAULT`), à esquerda da árvore de arquivos tradicional.
   - O Comet organiza seus espaços e sessões através do modelo GPUI em `crates/ui/src/workspace/`, com hierarquia e painéis próprios.
3. **Bordas Arredondadas (Radius):**
   - O MonoCode adota raios suaves e consistentes: `rounded-md` (`6px`) em botões e inputs (`src/chrome/ApprovalToasts.tsx:91`), `rounded-lg` (`8px`) em caixas de diálogo (`src/chrome/FilePicker.tsx:167`), e `rounded-xl` (`12px`) em toasts suspensos (`src/chrome/ApprovalToasts.tsx:64`).
4. **Barras de Rolagem:**
   - Em Linux e Windows, o MonoCode impõe barras de rolagem finas de `6px` com polegar translúcido arredondado (`src/index.css:76-88`), enquanto no macOS mantém a barra nativa do sistema.

Todos esses comportamentos pertencem à árvore de componentes da interface de usuário (`crates/ui`), estando formalmente fora do escopo de um tema registrado em `crates/theme`.

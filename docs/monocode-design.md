# Especificação de Design e Port do Tema MonoCode para o Comet

Referência canônica de design e especificação de port para a criação da família de temas built-in `MonoCode` no subsistema `zeron-theme` do Comet (`crates/theme/src/lib.rs:488-502`, `crates/theme/src/builtins.rs:74-213`). Este documento traduz a arquitetura visual paramétrica do MonoCode em valores literais estáticos exigidos pelo modelo de temas do Comet, detalhando sementes, derivações, conformidade WCAG 2.1 e fronteiras arquiteturais.

---

## 1. Escopo e proveniência

Este documento é estritamente uma **especificação de port** destinada a ser consumida de forma direta por uma change de implementação subsequente em `crates/theme/src/builtins.rs`. Ele não implementa código Rust, não altera estruturas de UI em `crates/ui`, não expressa juízos estéticos comparativos e não propõe mudanças arquiteturais no runtime GPUI.

### 1.1 Repositório-fonte e licença

A proveniência autoral e a licença de distribuição do tema são preservadas conforme exigido pelo contrato de temas embutidos (`crates/theme/AGENTS.md:25`):

- **Repositório de origem:** `hardbeat920/monocode`
- **Revisão:** `65fbe78`
- **Versão:** `0.1.34` (`package.json:4`)
- **Licença:** MIT License © 2026 Nick (`LICENSE:1-3`)

### 1.2 Contrato `ThemeSource` no Comet

No subsistema `zeron-theme`, toda variante de tema declara sua proveniência na struct `ThemeSource` (`crates/theme/src/lib.rs:436-444`), preenchida via função auxiliar `source()` (`crates/theme/src/builtins.rs:215-223`):

```rust
// Declaração canônica de ThemeSource para o MonoCode
source(
    "monocode-dark", // ou "monocode-light"
    "monocode",
    "https://github.com/hardbeat920/monocode",
    "65fbe78",
    "MIT",
)
```

O campo `asset_hash` não é preenchido estaticamente pelo desenvolvedor: a função `variant()` (`crates/theme/src/builtins.rs:165-167`) limpa o campo, serializa a struct `ThemeVariant` resolvida em JSON determinístico e calcula em runtime o digest criptográfico `sha256:{digest}`. Isso garante rastreabilidade e sensibilidade tanto a edições de curadoria quanto a atualizações de upstream.

---

## 2. Mecanismo visual do MonoCode

O MonoCode não utiliza uma paleta de cores estática pré-definida. Sua identidade visual é governada por quatro parâmetros de configuração injetados no elemento raiz do documento (`src/index.css:24-34`):

1. `--theme-hue`: `240` (ângulo de matiz no espaço polar HSL, default centrado em azul-frio; `src/index.css:25`, `src/lib/appearance.ts:46`).
2. `--theme-saturation`: `0%` (percentual de saturação; `src/index.css:26`, `src/lib/appearance.ts:50`). Em `0%`, a interface é estritamente acromática/monocromática.
3. `--background-lightness`: `9%` no modo dark (`src/index.css:28`) e `97%` no modo light (`src/index.css:41`).
4. `--content-lightness`: `92%` no modo dark (`src/index.css:29`) e `18%` no modo light (`src/index.css:42`).

### 2.1 Cores geradas

A partir desses parâmetros, a diretiva `@theme` do Tailwind v4 (`src/index.css:6-22`) gera duas variáveis de cor fundamentais:

- `--color-background-base`: gerada por `hsl(var(--theme-hue) var(--theme-saturation) var(--background-lightness))` (`src/index.css:7-9`). Governa o canvas do app (`body`, `#root`, `.body-glass`; `src/index.css:58, 119-121`).
- `--color-content`: gerada por `hsl(var(--theme-hue) var(--theme-saturation) var(--content-lightness))` (`src/index.css:10-12`). Governa a tipografia principal e atua como tinta base para todos os elementos derivados.

### 2.2 Escada de opacidade sobre `content`

Quase todos os componentes da interface do MonoCode são pintados utilizando camadas translúcidas de `content` sobrepostas ao fundo base, via `color-mix(in srgb, var(--color-content) N%, transparent)` ou classes utilitárias `bg-content/N` / `border-content/N` do Tailwind:

- **3% (`bg-content/3`):** container principal do composer e cartões de fila de mensagens (`src/chrome/Composer.tsx:262, 1143`).
- **5% (`bg-content/5`):** painéis de popover, modal picker de arquivos, menção rápida e seletor de cores (`src/chrome/FilePicker.tsx:167`, `src/chrome/FileMentionPicker.tsx:62`, `src/chrome/ColorPickerPopover.tsx:181`).
- **6% (`bg-content/6`):** cartões de preview de arquivos anexados e mini-cards de handoff/inbox (`src/chrome/FilePreview.tsx:84`, `src/chrome/HandoffMiniCard.tsx:26`, `src/chrome/InboxMiniCard.tsx:18`).
- **8% (`bg-content/8`):** chips de status de provedores (`src/chrome/InboxMiniCard.tsx:81`).
- **10% (`bg-content/10`):** painéis de toast de aprovação, botões de alternância de markdown e inputs primários (`src/chrome/ApprovalToasts.tsx:64`, `src/chrome/MarkdownModeToggle.tsx:37`, `src/chrome/GitChangesPanel.tsx:467`).
- **10% (`border-content/10`):** borda estrutural padrão de toda a aplicação (`src/chrome/Composer.tsx:1146`, `src/chrome/FilePreview.tsx:84`).
- **16% e 32%:** corpo e hover do indicador de rolagem em ambientes não-macOS (`src/index.css:85-92`).
- **20% (`border-content/20`):** borda reforçada e tracejada para toasts e foco (`src/chrome/ApprovalToasts.tsx:64`, `src/chrome/Composer.tsx:1146`).
- **35% e 40%:** texto de placeholder e rótulos desabilitados (`src/index.css:236-242`, `src/chrome/GitChangesPanel.tsx:467`).
- **50% e 55%:** citações em markdown e texto secundário atenuado (`src/index.css:200-202`, `src/chrome/CwdPicker.tsx:212`).
- **70% e 80%:** texto atenuado de botões secundários e menus (`src/chrome/ApprovalToasts.tsx:98`, `src/chrome/CwdPicker.tsx:286`).

---

## 3. Colapso paramétrico → literal

O sistema de temas do Comet não aceita funções dinâmicas CSS (`hsl()`, `color-mix()`) nem opacidades relativas não resolvidas. Para portar o tema, os parâmetros padrão do MonoCode (`--theme-hue: 240`, `--theme-saturation: 0%`) são projetados no espaço linear sRGB.

### 3.1 Resolução das cores base

Com saturação $S = 0$, a matiz $H$ não tem efeito cromático. A conversão padrão HSL para sRGB reduz-se à relação:

$$R = G = B = \text{round}(L \times 255)$$

1. **`background-base` (Dark):**
   $$L = 9\% = 0{,}09 \implies R = G = B = \text{round}(0{,}09 \times 255) = \text{round}(22{,}95) = 23 = \text{0x17} \implies \mathbf{\#171717}$$

2. **`background-base` (Light):**
   $$L = 97\% = 0{,}97 \implies R = G = B = \text{round}(0{,}97 \times 255) = \text{round}(247{,}35) = 247 = \text{0xf7} \implies \mathbf{\#f7f7f7}$$

3. **`content` (Dark):**
   $$L = 92\% = 0{,}92 \implies R = G = B = \text{round}(0{,}92 \times 255) = \text{round}(234{,}60) = 235 = \text{0xeb} \implies \mathbf{\#ebebeb}$$

4. **`content` (Light):**
   $$L = 18\% = 0{,}18 \implies R = G = B = \text{round}(0{,}18 \times 255) = \text{round}(45{,}90) = 46 = \text{0x2e} \implies \mathbf{\#2e2e2e}$$

A exatidão da resolução de `content` claro em `#2e2e2e` é confirmada no próprio código do MonoCode pelo fallback em `src/surfaces/TerminalView.tsx:85`.

### 3.2 Resolução de `color-mix` sobre fundo opaco

O cálculo de interpolação linear no espaço sRGB é definido por:

$$\text{Canal}_{\text{resultante}} = \text{round}\Big(C_{\text{fundo}} \times (1 - \alpha) + C_{\text{frente}} \times \alpha\Big)$$

#### Exemplo 1: Shell da Sidebar (Dark)
Conforme `src/index.css:104-106`, a classe `.sidebar-glass` sem compositor nativo aplica:
`color-mix(in srgb, var(--color-background-base) 90%, black)`
- Fundo: `#171717` ($23$)
- Frente: `black` ($0$)
- Proporção: $90\%$ fundo, $10\%$ preto
$$\text{Canal} = \text{round}(23 \times 0{,}90 + 0 \times 0{,}10) = \text{round}(20{,}70) = 21 = \text{0x15} \implies \mathbf{\#151515}$$

#### Exemplo 2: Shell da Sidebar (Light)
Conforme `src/index.css:108-110`, a classe `.sidebar-glass` no tema claro aplica:
`color-mix(in srgb, var(--color-background-base) 93%, black 7%)`
- Fundo: `#f7f7f7` ($247$)
- Frente: `black` ($0$)
- Proporção: $93\%$ fundo, $7\%$ preto
$$\text{Canal} = \text{round}(247 \times 0{,}93 + 0 \times 0{,}07) = \text{round}(229{,}71) = 230 = \text{0xe6} \implies \mathbf{\#e6e6e6}$$

#### Exemplo 3: Superfície de Card (Dark, 5% Content sobre Background Base)
Componentes de painel e popovers utilizam `bg-content/5` (`src/chrome/ColorPickerPopover.tsx:181`):
- Fundo: `#171717` ($23$)
- Frente: `#ebebeb` ($235$)
$$\text{Canal} = \text{round}(23 + (235 - 23) \times 0{,}05) = \text{round}(23 + 10{,}60) = 34 = \text{0x22} \implies \mathbf{\#222222}$$

#### Exemplo 4: Superfície Elevada / Raised (Dark, 10% Content sobre Background Base)
Superfícies de toast e controle utilizam `bg-content/10` (`src/chrome/ApprovalToasts.tsx:64`):
- Fundo: `#171717` ($23$)
- Frente: `#ebebeb` ($235$)
$$\text{Canal} = \text{round}(23 + (235 - 23) \times 0{,}10) = \text{round}(23 + 21{,}20) = 44 = \text{0x2c} \implies \mathbf{\#2c2c2c}$$

---

## 4. Tabela `Seeds` — Dark e Light

O contrato de semeadura de um tema built-in no Comet reside na struct `Seeds` (`crates/theme/src/builtins.rs:74-95`). A função `variant()` (`crates/theme/src/builtins.rs:97-169`) consome exatamente 12 campos de cor em hex literal.

Se uma cor semeada não atingir o contraste mínimo normativo, o Comet aplica **endurecimento silencioso** (`ensure_contrast`, `crates/theme/src/lib.rs:159-176`), alterando o valor final sem emitir erro de validação. Toda linha abaixo documenta o valor semeado, a referência no código do MonoCode, o valor efetivo pós-`variant()` e o delta numérico resultante.

### 4.1 Tabela `Seeds` — MonoCode Dark

| Campo `Seeds` | Hex Semeado | Origem MonoCode (`path:linha`) | Hex Efetivo Pós-`variant()` | Delta de Endurecimento | Justificativa de Mapeamento |
|---|---|---|---|---|---|
| `background` | `#171717` | `src/index.css:7-9, 28` | `#171717` | Nenhum (0) | Base do canvas com lightness 9% (`var(--color-background-base)`). |
| `shell` | `#151515` | `src/index.css:104-106` | `#151515` | Nenhum (0) | Superfície da sidebar `.sidebar-glass` misturada com 10% preto. |
| `raised` | `#2c2c2c` | `src/chrome/ApprovalToasts.tsx:64` | `#2c2c2c` | Nenhum (0) | Superfície elevada com 10% de `content` sobre `background`. |
| `card` | `#222222` | `src/chrome/ColorPickerPopover.tsx:181` | `#222222` | Nenhum (0) | Superfície de card intermediária (5% `content` sobre `background`). |
| `text` | `#ebebeb` | `src/index.css:10-12, 29` | `#ebebeb` | Nenhum (0) | Texto primário `content` com lightness 92% (`var(--color-content)`). |
| `muted` | `#8e8e8e` | `src/surfaces/editorLanguage.ts:84` | `#8e8e8e` | Nenhum (0) | Cor do token `meta`; contraste medido 5.47:1 supera o piso de 4.5:1. |
| `faint` | `#6c6c6c` | `src/index.css:236-242` | `#6c6c6c` | Nenhum (0) | Placeholder do composer (`content` a 40% sobre o fundo base). |
| `accent` | `#459bf7` | `src/index.css:13` | `#459bf7` | Nenhum (0) | Cor de destaque primária; contraste medido 6.22:1 supera 3.0:1. |
| `danger` | `#f87171` | `src/surfaces/TerminalView.tsx:45` | `#f87171` | Nenhum (0) | Vermelho `red-400` do Tailwind e ANSI dark red (`TerminalView`). |
| `warning` | `#fbbf24` | `src/surfaces/TerminalView.tsx:47` | `#fbbf24` | Nenhum (0) | Amarelo `amber-400` e ANSI dark yellow (`TerminalView`). |
| `success` | `#4ade80` | `src/surfaces/TerminalView.tsx:46` | `#4ade80` | Nenhum (0) | Verde `green-400` e ANSI dark green (`TerminalView`). |
| `terminal_background` | `#141b1f` | `src/surfaces/TerminalView.tsx:104` | `#141b1f` | Nenhum (0) | Fundo escuro emitido nas respostas OSC 11 (`OSC_DARK.bg`). |

### 4.2 Tabela `Seeds` — MonoCode Light

| Campo `Seeds` | Hex Semeado | Origem MonoCode (`path:linha`) | Hex Efetivo Pós-`variant()` | Delta de Endurecimento | Justificativa de Mapeamento |
|---|---|---|---|---|---|
| `background` | `#f7f7f7` | `src/index.css:7-9, 41` | `#f7f7f7` | Nenhum (0) | Base do canvas com lightness 97% (`var(--color-background-base)`). |
| `shell` | `#e6e6e6` | `src/index.css:108-110` | `#e6e6e6` | Nenhum (0) | Superfície da sidebar `.sidebar-glass` misturada com 7% preto. |
| `raised` | `#e3e3e3` | `src/chrome/ApprovalToasts.tsx:64` | `#e3e3e3` | Nenhum (0) | Superfície elevada com 10% de `content` claro sobre o fundo. |
| `card` | `#ededed` | `src/chrome/ColorPickerPopover.tsx:181` | `#ededed` | Nenhum (0) | Superfície de card intermediária (5% `content` claro sobre o fundo). |
| `text` | `#2e2e2e` | `src/index.css:10-12, 42` | `#2e2e2e` | Nenhum (0) | Texto primário `content` com lightness 18% (`var(--color-content)`). |
| `muted` | `#5c6370` | `src/surfaces/editorLanguage.ts:96` | `#5c6370` | Nenhum (0) | Cor do token `meta` claro; contraste medido 5.64:1 supera 4.5:1. |
| `faint` | `#a7a7a7` | `src/index.css:236-242` | `#a7a7a7` | Nenhum (0) | Placeholder do composer (`content` a 40% sobre o fundo base claro). |
| `accent` | `#0863c4` | `src/index.css:43` | `#0863c4` | Nenhum (0) | Cor do link no tema claro; contraste 5.46:1 supera 3.0:1. |
| `danger` | `#e45649` | `src/surfaces/TerminalView.tsx:65` | `#e45649` | Nenhum (0) | Vermelho One Light e ANSI light red (`TerminalView`). |
| `warning` | `#c18401` | `src/surfaces/TerminalView.tsx:67` | `#c18401` | Nenhum (0) | Amarelo/âmbar One Light e ANSI light yellow (`TerminalView`). |
| `success` | `#50a14f` | `src/surfaces/TerminalView.tsx:66` | `#50a14f` | Nenhum (0) | Verde One Light e ANSI light green (`TerminalView`). |
| `terminal_background` | `#fafafa` | `src/surfaces/TerminalView.tsx:105` | `#fafafa` | Nenhum (0) | Fundo claro emitido nas respostas OSC 11 (`OSC_LIGHT.bg`). |

*Nota sobre muted:* Se a semente clara adotasse `src/index.css:200-202` (55% de content = `#888888`), o contraste seria de 3.31:1, disparando `ensure_contrast` para endurecer a cor até `#6d6d6d`. A escolha de `meta` (`#5c6370`) preserva o tom pretendido pelo MonoCode sem provocar distorções silenciosas.

---

## 5. ANSI 16

O MonoCode implementa palettes completas de 16 cores ANSI no emulador de terminal (`src/surfaces/TerminalView.tsx:43-80`), inspiradas na família One Dark / One Light ajustada.

| Índice ANSI | Slot Semântico | Dark Hex | Light Hex | Origem MonoCode (`path:linha`) |
|---|---|---|---|---|
| `0` | Black | `#1d2428` | `#383a42` | `src/surfaces/TerminalView.tsx:44, 64` |
| `1` | Red | `#f87171` | `#e45649` | `src/surfaces/TerminalView.tsx:45, 65` |
| `2` | Green | `#4ade80` | `#50a14f` | `src/surfaces/TerminalView.tsx:46, 66` |
| `3` | Yellow | `#fbbf24` | `#c18401` | `src/surfaces/TerminalView.tsx:47, 67` |
| `4` | Blue | `#60a5fa` | `#4078f2` | `src/surfaces/TerminalView.tsx:48, 68` |
| `5` | Magenta | `#c084fc` | `#a626a4` | `src/surfaces/TerminalView.tsx:49, 69` |
| `6` | Cyan | `#22d3ee` | `#0184bc` | `src/surfaces/TerminalView.tsx:50, 70` |
| `7` | White | `#e8eef2` | `#fafafa` | `src/surfaces/TerminalView.tsx:51, 71` |
| `8` | Bright Black | `#64748b` | `#7c8591` | `src/surfaces/TerminalView.tsx:52, 72` |
| `9` | Bright Red | `#fca5a5` | `#df6b60` | `src/surfaces/TerminalView.tsx:53, 73` |
| `10` | Bright Green | `#86efac` | `#68b567` | `src/surfaces/TerminalView.tsx:54, 74` |
| `11` | Bright Yellow | `#fde68a` | `#d19a2f` | `src/surfaces/TerminalView.tsx:55, 75` |
| `12` | Bright Blue | `#93c5fd` | `#5c89f5` | `src/surfaces/TerminalView.tsx:56, 76` |
| `13` | Bright Magenta | `#d8b4fe` | `#b54bb3` | `src/surfaces/TerminalView.tsx:57, 77` |
| `14` | Bright Cyan | `#67e8f9` | `#1f9cc9` | `src/surfaces/TerminalView.tsx:58, 78` |
| `15` | Bright White | `#f8fafc` | `#ffffff` | `src/surfaces/TerminalView.tsx:59, 79` |
| N/A | `terminal_background` | `#141b1f` | `#fafafa` | `src/surfaces/TerminalView.tsx:104, 105` |

---

## 6. Syntax 12

A função `syntax()` do Comet (`crates/theme/src/builtins.rs:171-185`) consome um array posicional fixo de exatamente 12 cores e o desestrutura nesta ordem estrita. A tabela a seguir documenta cada slot e a origem correspondente no MonoCode (`src/surfaces/editorLanguage.ts:74-99`).

| Índice | Slot Posicional de `syntax()` | Dark Hex | Light Hex | Origem MonoCode (`path:linha`) e Papel Semântico |
|---|---|---|---|---|
| `0` | `comment` | `#fefdc2` | `#8a9199` | `src/surfaces/editorLanguage.ts:82, 94` (`comment`) |
| `1` | `keyword` | `#ff8ffd` | `#a626a4` | `src/surfaces/editorLanguage.ts:76, 88` (`keyword`) |
| `2` | `string` | `#b4fa72` | `#50a14f` | `src/surfaces/editorLanguage.ts:79, 91` (`string`) |
| `3` | `number` | `#b4fa72` | `#986801` | `src/surfaces/editorLanguage.ts:81, 93` (`number`) |
| `4` | `type` | `#ff8272` | `#c18401` | `src/surfaces/editorLanguage.ts:80, 92` (`type`) |
| `5` | `function` | `#a5d5fe` | `#4078f2` | `src/surfaces/editorLanguage.ts:78, 90` (`callable`) |
| `6` | `property` | `#d0d1fe` | `#e45649` | `src/surfaces/editorLanguage.ts:83, 95` (`property`) |
| `7` | `variable` | `#ebebeb` | `#2e2e2e` | `src/index.css:10, 29, 42` (`content`, padrão de texto não-estilizado) |
| `8` | `punctuation` | `#8e8e8e` | `#5c6370` | `src/surfaces/editorLanguage.ts:84, 96` (`meta`, estruturação sintática) |
| `9` | `tag` | `#ff8272` | `#c18401` | `src/surfaces/editorLanguage.ts:43, 80, 92` (`tags.tagName` mapeado a `type`) |
| `10` | `attribute` | `#d0d1fe` | `#e45649` | `src/surfaces/editorLanguage.ts:48, 83, 95` (`tags.attributeName` mapeado a `property`) |
| `11` | `invalid` | `#ffc4bd` | `#cf222e` | `src/surfaces/editorLanguage.ts:85, 97` (`invalid`) |

### 6.1 Mapeamento e reuso no `BTreeMap` do Comet

A implementação interna de `syntax()` (`crates/theme/src/builtins.rs:186-212`) projeta esses 12 slots em 24 chaves semânticas TextMate/Tree-sitter. As seguintes chaves reusam cores idênticas:

- `stringSpecial` e `escape` reusam a cor do slot `attribute` (`#d0d1fe` dark / `#e45649` light).
- `boolean` e `constant` reusam a cor do slot `number` (`#b4fa72` dark / `#986801` light).
- `typeBuiltin` e `constructor` reusam a cor do slot `type` (`#ff8272` dark / `#c18401` light).
- `functionBuiltin` e `label` reusam a cor do slot `function` (`#a5d5fe` dark / `#4078f2` light).
- `macro`, `variableSpecial` e `operator` reusam a cor do slot `keyword` (`#ff8ffd` dark / `#a626a4` light).
- `parameter` reusa a cor do slot `variable` (`#ebebeb` dark / `#2e2e2e` light).
- `embedded` reusa a cor do slot `punctuation` (`#8e8e8e` dark / `#5c6370` light).

---

## 7. Accent e seus 8 papéis

O modelo de temas do Comet não permite customizar cada sub-papel interativo de destaque de forma isolada. Uma única cor semeada em `Seeds.accent` é desdobrada pela função `AccentRoles::derive()` (`crates/theme/src/lib.rs:365-400`) nos 8 papéis da struct `AccentRoles` (`crates/theme/src/lib.rs:353-363`).

### 7.1 Seleção da cor de acento

- **MonoCode Dark:** `#459bf7` (`hsl(211 92% 62%)`, `src/index.css:13`).
- **MonoCode Light:** `#0863c4` (`hsl(211 92% 40%)`, `src/index.css:43`).

### 7.2 Papéis derivados calculados

| Papel Semântico | Dark Hex | Light Hex | Fórmula de Derivação em `AccentRoles::derive` |
|---|---|---|---|
| `primary` | `#459bf7` | `#0863c4` | Semente após `ensure_contrast(background, 3.0)` (sem alteração). |
| `strong` | `#459bf7` | `#0863c4` | `primary` endurecido para garantir $\ge 4{,}5:1$ contra `on`. |
| `wash` | `#459bf738` | `#0863c41f` | `primary.with_alpha(0.22)` dark / `0.12` light. |
| `on` | `#000000` | `#ffffff` | `primary.best_on_color()`. |
| `selection` | `#459bf759` | `#0863c43d` | `primary.with_alpha(0.35)` dark / `0.24` light. |
| `caret` | `#459bf7` | `#0863c4` | Idêntico ao `primary`. |
| `activity` | `#459bf7` | `#0863c4` | Idêntico ao `primary`. |
| `glyph[0]` (light) | `#79b7f9` | `#337ecd` | `primary.mix(WHITE, 0.28)` dark / `.mix(background, 0.18)` light. |
| `glyph[1]` (primary) | `#459bf7` | `#0863c4` | Idêntico ao `primary`. |
| `glyph[2]` (deep) | `#397fcb` | `#064991` | `primary.mix(BLACK, 0.18)` dark / `0.26` light. |

O contraste do `on` contra o `strong` é de **7.28:1** no Dark e **5.85:1** no Light, superando com folga a exigência mínima de 4.5:1.

---

## 8. Campos derivados: MonoCode × `variant()`

A struct `ThemeColors` do Comet carrega 26 campos (`crates/theme/src/lib.rs:446-475`). Destes, 14 são derivados matematicamente em `variant()` (`crates/theme/src/builtins.rs:116-143`) sem admissão de semeadura direta. Esta seção compara o comportamento nativo do MonoCode com a projeção estática do Comet, identificando concordâncias e divergências visuais.

### 8.1 Análise campo a campo

1. **`dialog`:**
   - *Fórmula Comet:* `card.mix(raised, 0.18 dark / 0.04 light)` $\to$ `#242424` (dark) / `#ededed` (light).
   - *MonoCode:* Diálogos modais usam `bg-content/5` com `backdrop-blur-xl` e backdrop preto a 40% (`src/chrome/Modal.tsx:112`, `src/chrome/FilePicker.tsx:167`).
   - *Veredito:* **Divergência leve (+2 níveis de cinza no dark).** O Comet aproxima a elevação combinando card e raised, enquanto o MonoCode conta com desfoque de fundo para isolar a superfície.

2. **`overlay`:**
   - *Fórmula Comet:* `card.mix(raised, 0.34 dark / 0.02 light)` $\to$ `#252525` (dark) / `#ededed` (light).
   - *MonoCode:* Popovers e menus usam `bg-content/5` ou `bg-content/10` (`src/chrome/ColorPickerPopover.tsx:181`).
   - *Veredito:* **Coincidência próxima.** Diferença imperceptível em monitores padrão ($\Delta E < 1$).

3. **`hover`:**
   - *Fórmula Comet:* `Color::WHITE.with_alpha(0.11)` $\to$ `#ffffff1c` (dark) / `Color::BLACK.with_alpha(0.06)` $\to$ `#0000000f` (light).
   - *MonoCode:* Hover em listas e botões aplica `hover:bg-content/5` ou `hover:bg-content/10` (`src/chrome/BranchPicker.tsx:442`, `src/chrome/Composer.tsx:274`).
   - *Veredito:* **Coincidência quase exata.** No dark, 10% de `content` (`#ebebeb`) equivale a `#ffffff1a`, alinhando-se a `#ffffff1c`.

4. **`active`:**
   - *Fórmula Comet:* `accent.primary.with_alpha(0.18 dark / 0.10 light)` $\to$ `#459bf72e` (dark) / `#0863c41a` (light).
   - *MonoCode:* Estados ativos no MonoCode são quase sempre neutros (`bg-content/10` a `20%`, `src/chrome/Composer.tsx:202`, `src/chrome/AccessPicker.tsx:92`), reservando o acento azul apenas para seleções de branch ou anéis de foco (`src/chrome/FileTree.tsx:1098`).
   - *Veredito:* **Divergência perceptível.** O Comet tonaliza estados ativos com o azul da marca, enquanto o MonoCode mantém a interação neutra monocromática.

5. **`border` e `border_strong`:**
   - *Fórmula Comet:* `border = border_tone.with_alpha(0.10 dark / 0.12 light)` $\to$ `#ffffff1a` (dark) / `#0000001f` (light). `border_strong = border_tone.with_alpha(0.18 dark / 0.22 light)` $\to$ `#ffffff2e` (dark) / `#00000038` (light).
   - *MonoCode:* Usa `border-content/10` (`#ebebeb1a` dark / `#2e2e2e1a` light) e `border-content/20` (`#ebebeb33` dark / `#2e2e2e33` light) (`src/chrome/Composer.tsx:1146`, `src/chrome/ApprovalToasts.tsx:64`).
   - *Veredito:* **Coincidência exata.** A geometria de borda do MonoCode é reproduzida fielmente.

6. **`solid` e `on_solid`:**
   - *Fórmula Comet:* Fixo em `rgb(235, 235, 239)` (`#ebebef`) dark / `rgb(35, 35, 40)` (`#232328`) light. `on_solid` é `#000000` dark / `#ffffff` light.
   - *MonoCode:* Botões primários utilizam `bg-content` com texto `text-background-base` (`src/chrome/GitChangesPanel.tsx:489`, `src/chrome/ApprovalToasts.tsx:91`) ou `bg-white text-black` (`src/chrome/Composer.tsx:1495`).
   - *Veredito:* **Coincidência exata.**

7. **`input`:**
   - *Fórmula Comet:* `raised.with_alpha(0.72)` $\to$ `#2c2c2cb8` (dark) / `card` $\to$ `#ededed` (light).
   - *MonoCode:* Campos de input utilizam `bg-content/5` ou `bg-content/10` (`src/chrome/Composer.tsx:311`, `src/chrome/GitChangesPanel.tsx:467`).
   - *Veredito:* **Coincidência excelente.** No dark, `#2c2c2c` a 72% de opacidade fundido sobre `#171717` resulta em `#262626`, posicionando-se exatamente entre os 5% (`#222222`) e 10% (`#2c2c2c`) do MonoCode.

8. **`cursor`:**
   - *Fórmula Comet:* `text.with_alpha(0.40 dark / 0.55 light)` $\to$ `#ebebeb66` (dark) / `#2e2e2e8c` (light).
   - *MonoCode:* Caret no composer utiliza opacidade total (`caret-color: var(--color-content)`, `src/index.css:231`), enquanto no terminal utiliza o accent azul (`src/surfaces/TerminalView.tsx:86`).
   - *Veredito:* **Divergência moderada.** O Comet atenua o cursor de texto com translucidez.

9. **`danger_muted`, `warning_muted`, `success_muted`:**
   - *Fórmula Comet:* `color.mix(text, 0.28 / 0.25)` $\to$ Dark: `#f49393`, `#f7ca56`, `#72e19b`; Light: `#b14b41`, `#9c6e0c`, `#488447`.
   - *MonoCode:* Utiliza tintas Tailwind saturadas com opacidades baixas (ex: `bg-red-500/20 text-red-300`, `bg-yellow-300/12 text-yellow-200/90`; `src/chrome/ExplorerMenu.tsx:137`, `src/chrome/Composer.tsx:1336`).
   - *Veredito:* **Divergência tonal.** O Comet desnatura os tons semânticos misturando-os ao texto base, enquanto o MonoCode preserva a saturação aplicando transparência alfa.

---

## 9. Contraste medido

Conforme implementado no validador oficial `ThemeRegistry::validate()` (`crates/theme/src/lib.rs:574-652`), qualquer tema embutido é submetido a 5 checagens contratuais de contraste e à verificação dos 14 slots ANSI cromáticos (`index % 8 != 0`).

O cálculo utiliza a fórmula de luminância relativa WCAG 2.1 (`crates/theme/src/lib.rs:141-149, 178-189`):

### 9.1 Gates de `ThemeRegistry::validate()`

| Gate de Validação | Papel / Comparação | Mínimo Exigido | Dark Medido | Light Medido | Veredito |
|---|---|---|---|---|---|
| Gate 1 | `text` vs `background` | $\ge 4{,}5:1$ | **15.04:1** | **12.68:1** | **PASS** |
| Gate 2 | `text_muted` vs `background` | $\ge 4{,}5:1$ | **5.47:1** | **5.64:1** | **PASS** |
| Gate 3 | `accent.primary` vs `background` | $\ge 3{,}0:1$ | **6.22:1** | **5.46:1** | **PASS** |
| Gate 4 | `accent.on` vs `accent.strong` | $\ge 4{,}5:1$ | **7.28:1** | **5.85:1** | **PASS** |
| Gate 5 | `terminal.foreground` vs `terminal.background` | $\ge 4{,}5:1$ | **14.60:1** | **13.01:1** | **PASS** |

### 9.2 Slots ANSI Cromáticos (14 slots contra `terminal_background`)

Em `crates/theme/src/lib.rs:635-648`, os slots 0 e 8 são ignorados por serem estruturais (preto/cinza dim). Os demais 14 slots que caírem abaixo de $3{,}0:1$ geram avisos não-bloqueantes (`ValidationSeverity::Warning`).

| Slot ANSI | Nome do Slot | Dark Hex | Dark Ratio | Dark Veredito | Light Hex | Light Ratio | Light Veredito |
|---|---|---|---|---|---|---|---|
| Slot 1 | Red | `#f87171` | 6.29:1 | PASS | `#e45649` | 3.51:1 | PASS |
| Slot 2 | Green | `#4ade80` | 9.99:1 | PASS | `#50a14f` | 3.07:1 | PASS |
| Slot 3 | Yellow | `#fbbf24` | 10.43:1 | PASS | `#c18401` | 3.06:1 | PASS |
| Slot 4 | Blue | `#60a5fa` | 6.85:1 | PASS | `#4078f2` | 3.88:1 | PASS |
| Slot 5 | Magenta | `#c084fc` | 6.59:1 | PASS | `#a626a4` | 5.86:1 | PASS |
| Slot 6 | Cyan | `#22d3ee` | 9.63:1 | PASS | `#0184bc` | 4.00:1 | PASS |
| Slot 7 | White | `#e8eef2` | 14.87:1 | PASS | `#fafafa` | 1.00:1 | **WARNING** |
| Slot 9 | Bright Red | `#fca5a5` | 9.17:1 | PASS | `#df6b60` | 3.14:1 | PASS |
| Slot 10 | Bright Green | `#86efac` | 12.40:1 | PASS | `#68b567` | 2.40:1 | **WARNING** |
| Slot 11 | Bright Yellow | `#fde68a` | 13.98:1 | PASS | `#d19a2f` | 2.40:1 | **WARNING** |
| Slot 12 | Bright Blue | `#93c5fd` | 9.65:1 | PASS | `#5c89f5` | 3.17:1 | PASS |
| Slot 13 | Bright Magenta | `#d8b4fe` | 9.85:1 | PASS | `#b54bb3` | 4.35:1 | PASS |
| Slot 14 | Bright Cyan | `#67e8f9` | 12.01:1 | PASS | `#1f9cc9` | 3.02:1 | PASS |
| Slot 15 | Bright White | `#f8fafc` | 16.64:1 | PASS | `#ffffff` | 1.04:1 | **WARNING** |

*Diagnóstico dos avisos no Light:* Os avisos nos slots 7 e 15 decorrem do fato de a paleta One Light do MonoCode definir branco puro sobre fundo terminal quase-branco (`#fafafa`). Os slots 10 e 11 refletem tons claros da paleta upstream que ficam ligeiramente abaixo do limiar de 3:1. Como o validador classifica esses itens como `ValidationSeverity::Warning` (`crates/theme/src/lib.rs:721`), eles não bloqueiam o registro do tema.

### 9.3 Output literal do script de cálculo

```text
=== DARK THEME RESULTS ===
Gate text: 15.04:1
Gate muted: 5.47:1
Gate accent: 6.22:1
Gate on_accent: 7.28:1
Gate term_fg: 14.60:1
Muted seeded: #8e8e8e -> effective: #8e8e8e
Accent seeded: #459bf7 -> effective: #459bf7
ANSI warnings: 0

=== LIGHT THEME RESULTS ===
Gate text: 12.68:1
Gate muted: 5.64:1
Gate accent: 5.46:1
Gate on_accent: 5.85:1
Gate term_fg: 13.01:1
Muted seeded: #5c6370 -> effective: #5c6370
Accent seeded: #0863c4 -> effective: #0863c4
ANSI warnings: 4
  Slot 7 (#fafafa): 1.00:1 (< 3.0:1)
  Slot 10 (#68b567): 2.40:1 (< 3.0:1)
  Slot 11 (#d19a2f): 2.40:1 (< 3.0:1)
  Slot 15 (#ffffff): 1.04:1 (< 3.0:1)
```

---

## 10. `recommended_surface_treatment`

A recomendação oficial para as variantes MonoCode é:

$$\mathbf{SurfaceTreatment::Frosted}$$

### 10.1 Evidência no código do MonoCode

Toda a identidade de design do MonoCode repousa sobre superfícies vítreas translúcidas e desfoque nativo:

- **Efeito de vidro no macOS:** o app ativa a classe `has-native-glass` e torna o fundo de `body` e `#root` transparente (`src/index.css:98-102`), aplicando na sidebar a cor `hsl(var(--theme-hue) var(--theme-saturation) var(--background-lightness) / var(--sidebar-opacity))` com `--sidebar-opacity: 0.85` (`src/index.css:112-117`).
- **Desfoque nativo de janela:** a função `applySidebarBlur` invoca o comando Tauri `set_window_background_blur` com raio padrão de $24\text{px}$ (`src/lib/appearance.ts:56-59, 262-267`).
- **Filtros de backdrop em componentes:** todas as superfícies flutuantes aplicam desfoque de backdrop CSS (ex: `backdrop-blur-xl` em toasts e seletores de arquivos, `src/chrome/ApprovalToasts.tsx:64`, `src/chrome/FilePicker.tsx:167`; `backdrop-blur-md` no seletor de markdown, `src/chrome/MarkdownModeToggle.tsx:37`).
- **Configuração padrão:** a flag `BODY_GLASS_DEFAULT` é fixada em `true` (`src/lib/appearance.ts:64`).

### 10.2 Limitações na tradução para o Comet

O enum `SurfaceTreatment` do Comet (`crates/theme/src/lib.rs:59-65`) é estritamente binário (`Opaque` ou `Frosted`). O modelo não possui suporte a:
- Ajuste contínuo de opacidade (o slider `0.15` a `1.0` do MonoCode não existe no modelo do Comet).
- Raio de desfoque configurável (o parâmetro `1` a `64px` do MonoCode é omitido).
- Desfoque em nível de componente individual (CSS `backdrop-blur`).

No runtime do Comet, a elegibilidade ao tratamento frost abrange tanto macOS quanto Linux (`is_frost()`, `crates/ui/src/theme.rs:794-797`), ficando o Windows de fora; o alpha de vidro translúcido reduzido é exclusivo do macOS (`Theme::GLASS_ALPHA = 0.80`, `crates/ui/src/theme.rs:686`), enquanto as demais plataformas operam com alpha 1.0.

---

## 11. GAPs

Esta seção lista os aspectos da identidade visual do MonoCode que dependem de estado dinâmico de runtime, variáveis do sistema operacional ou efeitos procedurais, não podendo ser capturados como valores hexadecimais estáticos:

1. **Transparência para o papel de parede (Desktop Wallpaper Passthrough):**
   No MonoCode com `glass-body` ativado (`src/index.css:146-152`), a área de conteúdo do transcript torna-se parcialmente transparente, deixando transparecer as cores da imagem de fundo da área de trabalho do usuário. No Comet, o canvas principal recebe cores hex opacas fixas (`#171717` / `#f7f7f7`).
2. **Customização paramétrica contínua em runtime:**
   O MonoCode expõe seletores interativos para alterar `--theme-hue` e `--theme-saturation` em tempo de execução (`src/lib/appearance.ts:143-154`, `src/chrome/ColorPickerPopover.tsx:82-243`). No Comet, as variantes embutidas são estáticas e fixadas na combinação acromática padrão ($H=240, S=0\%$). Variações de matiz exigiriam variantes separadas ou a biblioteca de temas customizados (`crates/theme/src/library.rs`).
3. **Assimetria de desfoque multiplataforma:**
   O MonoCode executa desfoque nativo no macOS e Windows via Tauri (`set_window_background_blur`). No Comet, a infraestrutura GPUI restringe o suporte a vidro translúcido ao macOS, exibindo superfícies sólidas opacas em outras plataformas.
4. **Animação procedural de boas-vindas (Astra Welcome):**
   O arquivo `src/surfaces/AstraWelcome.css:1-63` define uma animação orbital complexa utilizando gradientes radiais dourados (`#f5f0e4`, `#d8c9a2`, `#a89560`). Esses elementos são puramente decorativos e transitórios, não pertencendo aos tokens semânticos do tema.

---

## 12. Apêndice: fora do tema

O escopo do modelo de temas do Comet (`crates/theme/src/lib.rs:488-502`) abrange unicamente a identidade da variante (`id`, `family_id`, `name`, `appearance`), a recomendação de superfície (`recommended_surface_treatment`), a paleta de cores (`ThemeColors`), o acento (`AccentRoles`), os tokens de sintaxe (`syntax`), a paleta de terminal (`TerminalPalette`) e a proveniência (`ThemeSource`). Famílias de fontes tipográficas não integram o modelo de temas.

Os seguintes aspectos do design do MonoCode pertencem à camada de estrutura e renderização de componentes, situando-se fora das capacidades do tema:

- **Densidade e espaçamento de layout:**
  O MonoCode utiliza a escala compacta do Tailwind CSS (ex: paddings de $6\text{px}$ a $10\text{px}$, inputs com altura de $24\text{px}$ a $28\text{px}$; `src/chrome/Composer.tsx:311`, `src/chrome/GitChangesPanel.tsx:489`). O Comet possui constantes geométricas e paddings próprios e invariantes expressos em GPUI (`crates/ui/src/shell.rs`, `crates/ui/src/composer.rs`).
- **Escala tipográfica:**
  O MonoCode adota tamanhos arbitrários Tailwind como `text-[11px]`, `text-[12px]`, `text-[13px]` e entrelinhas finas (`leading-4`, `leading-5.5`). A escala tipográfica do Comet é governada por tokens internos de `crates/ui`.
- **Famílias de fontes:**
  O MonoCode define pilhas genéricas do sistema (`--font-sans: system-ui, -apple-system, ...` e `--font-mono: ui-monospace, Menlo, ...`; `src/index.css:16-21`). No Comet, a tipografia é de responsabilidade da camada de UI, que utiliza Geist e Geist Mono como padrões canônicos (`crates/ui/src/theme.rs:946-947`), situando-se completamente fora do modelo de temas de `crates/theme`.
- **Iconografia:**
  O MonoCode emprega ícones da biblioteca `@hugeicons/react` (`package.json:35`). O Comet utiliza vetores SVG embutidos em código e desenhados nativamente pelo GPUI (`crates/ui/src/icons.rs`).
- **Motion e micro-interações:**
  As animações de entrada de popover e animações interativas do MonoCode utilizam transições CSS e animações no DOM. O Comet controla motion por GPUI paint transforms via `crates/ui/src/motion.rs`.

Qualquer tentativa de aproximar esses aspectos estruturais exigiria alterações em `crates/ui`, o que está expressamente fora do escopo deste port de tema.

# Unpeel Workers sidebar — mapa visual 1:1

Autoridade: `third_party/unpeel` fixado no repositório. Este documento mapeia somente a sidebar usada como base da aba **Workers**; não redefine a aba Orchestrator.

## Estrutura

| Elemento | Contrato Unpeel | Fonte |
| --- | --- | --- |
| Largura | 300 padrão; 220 mínima; 520 máxima | `Theme.swift`, `SidebarView.swift` |
| Lista | `LazyVStack`, spacing 1 | `SidebarView.swift:467` |
| Insets da lista | top 48, horizontal 8, bottom 60 | `SidebarView.swift:477-480` |
| Footer | spacing 2; left/right/bottom 7.5 | `SidebarView.swift:3014-3050` |
| Empty state | VStack spacing 14; pasta 40; botão 13 semibold | `SidebarView.swift:537-559` |

Não existe título “Workers”, sino ou refresh acima da árvore no Unpeel. A lista começa atrás do chrome superior transparente e usa o inset de 48 px. No Comet, as abas Orchestrator/Workers já ocupam essa zona de chrome; portanto o componente hospedado zera somente esse inset superior para não somá-lo duas vezes. As demais medidas permanecem idênticas.

## Linhas de projeto

| Token | Valor |
| --- | --- |
| Altura mínima | 28 px |
| Raio | 9 px |
| Gap interno | 7 px |
| Padding vertical | 2 px |
| Padding direito | 7 px |
| Recuo principal | `7 + depth * 14` px |
| Recuo de pasta filha | `10 + max(0, depth - 1) * 14` px |
| Ícone de pasta | SVG glass aberto/fechado, 16 px em slot 18 px |
| Label | 13 px medium; projeto raiz com foreground 60% |
| Hover | foreground 10% |
| Branch de worktree | ícone 12, gap 3, mono 10, opacidade 55%, máximo 110 |

As ações rápidas são invisíveis fora do hover da linha. No hover, o strip tem 24 px de altura, chips 22×22 com raio 8, ícones de provider 14 e o botão “+” usa SVG de 16 px. Presets favoritos são agrupados por CLI; não há limite arbitrário de três providers.

## Linhas de sessão

| Token | Valor |
| --- | --- |
| Altura mínima | 28 px |
| Raio | 9 px |
| Gap interno | 7 px |
| Padding vertical | 2 px |
| Padding horizontal | `9 + depth * 14` à esquerda; 9 à direita |
| Slot de status | 16×16 px sempre reservado |
| Label | 13 px regular |
| Provider | SVG 12 px em slot 14 px |
| Idade | 10 px muted; atalho só substitui idade enquanto Command está pressionado |
| Selecionada | active row 16% (fallback sem Liquid Glass) |
| Busy | spinner braille em 120 ms, tint do provider |
| Attention | ponto 6 px e halo 14 px a 20% |
| Unread | ponto azul 7 px |
| Pin | SVG 13 px; visível fixo se pinned, ou no hover |

## Catálogo de providers

| Runtime estável | SVG usado |
| --- | --- |
| `com.sourcegraph.amp` | `amp.svg` |
| `com.anthropic.claude-code` | `claude.svg` |
| `bot.cline.cli` | `cline.svg` |
| `com.openai.codex` | `codex.svg` |
| `com.cursor.agent` | `cursor-agent.svg` |
| `com.google.gemini-cli` | `gemini.svg` |
| `com.github.copilot-cli` | `generic-agent.svg` (fallback oficial do Unpeel) |
| `ai.x.grok-cli` | `grok.svg` |
| `com.moonshot.kimi-code` | `kimi.svg` |
| `dev.kiro.cli` | `kiro.svg` |
| `ai.meta.muse-code` | `muse-code.svg` |
| `ai.opencode.cli` | `opencode.svg` |
| `dev.mariozechner.pi` | `pi.svg` |

O resolver também reconhece aliases e o executável inicial do comando (`claude`, `codex`, `cursor-agent`, `copilot`, `ghcs`, `kiro-cli`, etc.). Um comando desconhecido usa o ícone de terminal, como no Unpeel.

## SVGs de chrome portados

- pasta fechada, aberta e simples;
- branch de worktree e git branch;
- pin e push pin;
- settings, add-project, plus e collapse-all;
- drag handle.

Todos são assets embarcados no binário e não dependem do checkout `third_party/unpeel` em runtime.

## Menu bar macOS — somente Workers

O contrato foi portado de
`third_party/unpeel/apps/native/UnpeelNative/Sources/UnpeelNative/MenuBarController.swift`
e do reducer `ActivityMenuSessions` usado pelo mesmo arquivo. O Comet mantém um
único `WorkersModel` compartilhado entre janela e menu bar; fechar ou reabrir a
janela não cria uma segunda fonte de estado.

| Estado | Regra determinística | Apresentação |
| --- | --- | --- |
| Working | existe sessão Workers trabalhando | spinner braille, frame a cada 120 ms |
| Blocked | sessão aguarda atenção; prevalece sobre unread | marca com indicador de atenção |
| Unread | sessão terminou e ainda não foi aberta | marca com indicador unread |
| Idle | nenhum dos estados anteriores | logo template do Comet |

O popover é `NSPopover` transitório, com largura 332 px, margem 12 px, cabeçalho
34 px, linhas 42 px, raio 9 px e footer 28 px. As seções seguem a mesma ordem do
reducer do Unpeel: blockers, jobs ativos e finalizados. Cada linha preserva o
provider SVG e revela exatamente a sessão e o projeto selecionados; não existe
fallback para uma sessão de outro projeto. `All recent` abre a rota Workers
Recent, com sessões ativas primeiro e histórico canônico do ActivityLog agrupado
por Today, Yesterday e data.

A integração AppKit usa `NSStatusItem.variableLength` e retém explicitamente o
status item durante toda a vida do controller. Isso evita a desalocação após o
autorelease pool — causa confirmada do crash observado no primeiro smoke nativo.

### Gates executados

- 26 testes unitários do adapter e todas as suites de integração;
- 45/45 testes de Workers UI, incluindo precedência, deduplicação, geometria,
  tags nativas e seleção entre projetos;
- 9/9 testes do engine para convergência assíncrona de título/branch;
- 689/689 assertions PTY reais do Unpeel;
- `cargo check -p zeron-ui` e `cargo build -p zeron`;
- smoke nativo com o status item permanecendo ativo após a atualização do model;
- `cargo fmt --all -- --check` e `git diff --check`.

## Ledger final de paridade

| Superfície | Fonte Unpeel | Implementação Comet | Evidência |
| --- | --- | --- | --- |
| Árvore/hover/linhas | `SidebarView.swift` | `workers/workspace.rs`, `presentation.rs`, `session_menu.rs`, `project_menu.rs` | `01`, `03`, `09` |
| Providers/SVGs | catálogo e assets nativos | `icons.rs`, adapter de runtime | testes de catálogo + `07`, `08` |
| Spinner/attention/unread | `HookServer.swift`, activity reducer | `activity_bridge.rs`, `notification_policy.rs`, model compartilhado | 45 testes UI + `09`, `10` |
| Terminal/input/resize | controller protocol + terminal viewport | `workers/terminal.rs`, initial-grid patch | 689 PTY + `07`, `08`, `10` |
| Project/worktree/session actions | reducers e menus da sidebar | adapters tipados + menus por capability | suites `project_actions`/`session_actions` |
| Archive/Recent | store e views nativas | `archive.rs`, `recent.rs` | testes de tuple identity + `05`, `09` |
| Settings | painéis nativos | somente Presets, Transcripts, Notifications | `02` + testes de settings |
| Menu bar | `MenuBarController.swift` | `workers/menu_bar.rs` + `NSStatusItem` | `04`, `05` |
| Isolação Orchestrator | contrato Comet | root Workers retido e modelo separado | `11`, `12` |
| Estado compartilhado | `~/.unpeel` + `session.sock` | core canônico, sem store paralelo | `10` + `unpeel ls/send/wait/screen` |

Os números de evidência correspondem aos arquivos em
`.impeccable/review/workers-parity-completion/`. No teste `10`, o CLI oficial do
Unpeel descobriu a sessão criada no Comet, enviou
`echo COMET_UNPEEL_SHARED_OK`, esperou o texto e leu a mesma viewport; o Comet
refletiu o output e o título automaticamente. O primeiro ensaio usou um caminho
temporário cujo `session.sock` tinha 105 caracteres e excedeu o limite Unix do
macOS; o ensaio válido foi repetido em `/tmp/uqa.XcbWqA` com socket presente.

## Proveniência pendente

O checkout local de `third_party/unpeel` contém dois commits isolados sobre
`b02a4b5`: `5f23a30` (initial terminal grid) e `fb6f77d` (estabilização das
provas PTY). Eles passaram nos gates locais, mas não foram publicados. O root
gitlink só deve ser atualizado depois de autorização explícita para criar ou
usar `zeronsh/unpeel` e fazer push desses objetos; até lá, o `+` do submodule é
esperado e evita registrar uma referência impossível de clonar.

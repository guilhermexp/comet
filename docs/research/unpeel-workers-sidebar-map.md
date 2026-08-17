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

- 25 testes unitários de Workers, incluindo precedência, deduplicação, geometria,
  tags nativas e seleção entre projetos;
- teste do adapter preservando os campos do ActivityLog do Unpeel;
- `cargo build -p zeron`;
- smoke nativo com o status item permanecendo ativo após a atualização do model;
- `cargo fmt --all -- --check` e `git diff --check`.

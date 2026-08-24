# comet-ui — o app gpui

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

O viewport: shell (sidebar de spaces + abas), transcript, composer, painel de terminal, pane de diff, settings, markdown próprio e o kit de animação. Renderiza estado do mirror direto, com notificação por entrada alterada.

## Ownership

Dona de tudo que é pixel. **Não** é dona de comportamento que precisa sobreviver à janela fechada — isso é `comet-engine`. Regra derivada compartilhada com a engine mora em `comet-proto::view`, não aqui.

## Local Contracts

- gpui vem do fork `wingleeio/zed` pinado por rev no `Cargo.toml` raiz. **Não usamos as crates GPL do Zed** (`markdown`, `ui`, `theme`, `editor`) — markdown, componentes e tema são nossos. Puxar uma delas é problema de licença, não de gosto.
- Tema (`theme.rs`) suporta light e dark. Token que sumiu do upstream (ex: `white_alpha`) se remapeia pro equivalente que o upstream aplicou no código gêmeo — não se recria localmente, senão o light mode fica com wash.
- Animação é camada de **paint**: `with_animation` sobre opacidade nunca altera layout. `prefers-reduced-motion` é honrado.
- Altura de linha em code block = linhas × line-height, independente do highlight; o highlight roda time-sliced em background e entra como run de texto (paint-only).
- Transcript é por **bloco**, não por mensagem: id estável `msgId#blockId`, turno vivo não splitado, re-split na persistência. Eco otimista compartilha o id cunhado no cliente pra persistência não piscar.
- Cards do usuário são sticky por turno: um clone paint-only do renderer existente ocupa o inset do runway e é empurrado pelo próximo user row. A geometria é per-chat, não altera altura da lista, não substitui o runway e não duplica o original quando ele já ocupa a posição.
- O wrapper externo do sticky é transparente; a oclusão/blur e o bloqueio de mouse/hover subjacente ficam limitados ao card interno arredondado, enquanto wheel/touch continuam chegando ao transcript.
- `TurnSteps` e a projeção de mudanças de arquivo mantêm ids estáveis; previews de Write/Edit renderizam somente o conteúdo limitado que veio do doc.
- Dentro de `TurnSteps` expandido, grupos de tools mostram os cards individuais por padrão; stdout, invocações e diffs internos continuam fechados, e toggle explícito do usuário prevalece.
- Cards inline de arquivo mantêm expansão, lazy fetch e `ScrollHandle` interno no `Transcript`, keyed pelo row id estável, para virtualização e TurnSteps não resetarem o card.
- Input histórico de arquivo é derivado/highlighted fora do render; corpos grandes usam `uniform_list`, e linhas lógicas patológicas são divididas em paint rows completos de até 512 caracteres antes do cache.
- `crates/ui/src/terminal/` é o **painel de terminal dentro do app**. Não confundir com o `crates/tui` deletado (viewport ratatui do upstream, removido).
- O pane direito é **um único host de tabs**: `right_tabs` + `right_active` (terminal, diff, file preview, subagent, worker) numa strip só. Não existe segundo registro de painel — o par `UtilityPane`/`changes_open` com `Changes`/`TerminalPanel` próprios foi removido porque fazia preview abrir *por cima* do diff, com o outro vivo atrás. `right_pane_open` = coluna visível **e** com ao menos uma tab; sem tab, sem coluna. Nada aqui consulta git: preview abre em pasta sem repo.
- A banda de tabs do pane tem altura fixa (`TAB_BAR_HEIGHT`, `flex_none`) e as chips têm largura natural com `min_w_0`; `size_full`/`flex_1` na banda rouba a altura do conteúdo (superfície em branco) ou divide a largura com a drag region (strip cortada no meio). Chips rolam, `+` e chevron ficam fora do scroller, e a tab recém-selecionada é revelada uma vez via `scroll_to_item`.
- Provider autenticado fica expansível quando houver janela remota **ou** linha de usage local. `NoUsage` significa que ambas estão vazias; linhas locais de 24h/7d/30d continuam acessíveis sem quota remota. Providers managed renderam na ordem fixa Claude, Codex, Kimi; Kimi reusa a marca embutida `WORKER_KIMI` e não tem ação de login/add/switch em Accounts.
- Overflow do widget Details: o card Workers mantém o viewport interno compacto de 152px de scroll. Labels de workflow, subagent e progresso precisam ser normalizados para uma linha visual antes de entrar em rows de altura fixa; label multilinha cru causa sobreposição de texto e é proibido.
- Activities do widget Workers começam colapsadas; toggles explícitos permanecem keyed pelo id estável. Linhas de subagent preservam avatar e status lifecycle, incluindo spinner paint-only durante `Running`.
- Linhas do To-dos usam um único slot circular não encolhível, com check/seta
  centralizados nos dois eixos e a mesma geometria `36/12/9` do card inline;
  estados não recebem offsets ópticos próprios.

## Work Guidance

- "Não atualizou na tela" começa em `comet-doc` (mirror), não aqui.
- Não há harness de render: mudança visual se valida rodando `scripts/dev-demo.sh` e olhando. Screenshot antes de dizer pronto.
- Toda raiz de `track_focus` precisa de `.id()` + `.role()` (mais `aria_label` / `aria_value` quando o controle carrega texto). Sem role o gpui deixa o elemento fora da árvore AccessKit, loga `a11y: focused element … has no accessibility node` a cada mudança de foco, e a tecnologia assistiva anuncia a janela inteira em vez do controle focado.

## Verification

- Comandos: `cargo test -p comet-ui` · `scripts/dev-demo.sh`

| Camada / path | Tier exigido | Como rodar |
|---|---|---|
| `src/**` (estado, derivações, parse de markdown) | unit | `cargo test -p comet-ui` |
| `src/markdown/**` | unit — parse e mend têm cobertura própria | `cargo test -p comet-ui` |
| `src/{shell,settings,terminal}/**` (render gpui) | none — sem harness de render; validação é visual | `scripts/dev-demo.sh` |

## Child DOX Index

Subárvores sem doc próprio (ainda não têm regra local além da desta pasta): `shell/` (spaces, tabs), `terminal/` (emulator, panel, view), `settings/`, `markdown/`. Adensar aqui quando alguma ganhar contrato próprio.

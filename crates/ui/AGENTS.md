# comet-ui — o app gpui

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

O viewport: shell (sidebar de spaces + abas), transcript, composer, painel de terminal, pane de diff, settings, markdown próprio e o kit de animação. Renderiza estado do mirror direto, com notificação por entrada alterada.

## Ownership

Dona de tudo que é pixel. **Não** é dona de comportamento que precisa sobreviver à janela fechada — isso é `comet-engine`. Regra derivada compartilhada com a engine mora em `comet-proto::view`, não aqui.

## Local Contracts

- gpui vem do fork `wingleeio/zed` pinado por rev no `Cargo.toml` raiz. **Não usamos as crates GPL do Zed** (`markdown`, `ui`, `theme`, `editor`) — markdown, componentes e tema são nossos. Puxar uma delas é problema de licença, não de gosto.
- Knobs de captura (`ZERON_OPEN_ROUTE`, `ZERON_OPEN_DIALOG`, `ZERON_OPEN_PICKER`, `ZERON_FORCE_GATE`, `ZERON_DEMO_UPLOAD`) só valem com `ZERON_UI_CAPTURE=1|true|yes|on` e passam **sempre** por `capture::knob`. `std::env::var` direto para uma knob é proibido: exportada uma vez num shell, ela seguia todo `cargo run` daquele terminal — o app abriu na página Accounts por dias. Run normal boota no chat, ponto.
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
- O gesto de captura (menu nativo de modo → `screencapture` → cancelamento) mora **uma vez** em `workers::session_gallery::pick_and_capture`; `Ok(None)` é cancelamento, não erro. Orchestrator e Workers só escolhem destino e o que fazer com o arquivo.
- O trailing da titlebar do chat é **um cluster dimensionado pelo conteúdo** (captura, toggle de details, expand/close do pane): o `pr` da row já termina na borda da coluna do chat. Largura explícita (`right_now - pr`) ou overlay absoluto com offset calculado voltam a pintar em cima do título ou dos próprios botões — foi assim duas vezes.
- Provider autenticado fica expansível quando houver janela remota **ou** linha de usage local. `NoUsage` significa que ambas estão vazias; linhas locais de 24h/7d/30d continuam acessíveis sem quota remota. Providers managed renderam na ordem fixa Claude, Codex, Kimi; Kimi reusa a marca embutida `WORKER_KIMI` e não tem ação de login/add/switch em Accounts.
- O número no header do Usage é quota **restante** (`Weekly 72%`), e a ênfase âmbar dele é por **proximidade de reset**, nunca por percentual: badge `Reset 12h 16m` + texto em `warning` aparecem só dentro de `RESET_SOON_HOURS` (48h). Janela em 5% que vira semana que vem não é urgente; 95% que vira hoje à noite é. Os limiares de 10%/25% continuam valendo só para as barras do corpo expandido, então header e barra podem discordar de propósito. A presença do badge É o gate — não existe booleano paralelo.
- Overflow do widget Details: o card Workers rola no próprio body, dimensionado em linhas inteiras (`CHAT_WORKERS_VISIBLE_ROWS` × `CHAT_WORKERS_ROW_HEIGHT` = 6 × 32px); o antigo 152px fixo cortava a quinta linha no meio do glifo. Labels de workflow, subagent e progresso precisam ser normalizados para uma linha visual antes de entrar em rows de altura fixa; label multilinha cru causa sobreposição de texto e é proibido.
- Activities do widget Workers começam colapsadas; toggles explícitos permanecem keyed pelo id estável. Linhas de subagent preservam avatar e status lifecycle, incluindo spinner paint-only durante `Running`.
- Linhas do To-dos usam um único slot circular não encolhível, com check/seta
  centralizados nos dois eixos e a mesma geometria `36/12/9` do card inline;
  estados não recebem offsets ópticos próprios.
- O To-dos mostra no máximo **10 linhas completas** (`TODO_VISIBLE_ROWS`, 10 × 36px); acima disso o body do próprio card rola. A quantidade de tarefas nunca volta a alongar o Details sidebar inteiro.
- Ordem no widget Workers é **ativos primeiro, mais novos primeiro dentro de cada grupo**. Activities/subagents usam a ordem inversa do transcript; CLI Workers ordenam por `created_at_unix_ms` descendente (desempate por update e id). Um launch novo fica no topo da seção ativa sem enterrar trabalho ainda rodando atrás de uma linha encerrada.
- O Files pane do details sidebar **não consulta `.gitignore`**, global excludes nem `.ignore`: a raiz de um workspace é diretório de estado vivo tanto quanto checkout, e honrar as regras de ignore escondia justamente o que o pane existe pra mostrar (`~/.orchestrator` ignora `logs/`, `sessions/`, `data/`, `brain-source/`). Ruído estrutural (`.git`, `node_modules`, `target`, `dist`, `build`, caches) é negado por **nome** em `file_tree::DENIED_DIRECTORIES`/`DENIED_FILES`, e o watcher de recência reusa a mesma regra por `is_denied_relative`.
- Symlink de diretório é **pasta não atravessável**: o `file_type` do link diria "arquivo", então a classificação stata o alvo (`entry_is_dir`) enquanto `follow_links(false)` continua barrando a descida. Link quebrado permanece arquivo.
- Recência de arquivo é **event-sourced**, nunca mtime: o scan não lê timestamp nenhum. Evento do watcher marca o path com o instante em que **chegou**, e o tier decai em 10s/60s/180s (`recency.rs`) com tick de 5s que também poda a última marca — sem poda notificada a linha final ficaria com tint velho. O decaimento é lido no render contra wall clock, não via `with_animation`: gpui reinicia o clock por element id em remount e as rows são reconstruídas em todo rescan.
- Rescan disparado por watcher é **silencioso** (`refresh_files`): manter `LoadState::Loading` piscava o pane vazio a cada save. Debounce de 500ms coalesce a rajada de eventos.
- A faixa de status mostra o **motivo** da falha quando a session row traz um (`view::run_failure_text`), truncado na própria linha; `"Run failed"` é só o fallback de quando não há motivo. A constante crua escondia mensagens acionáveis que a engine já tinha em mãos e journalava.
- **Escape no composer é gesto de duas etapas**: um Esc mantém tudo que já fazia (fecha o popup de menção, volta uma página do painel de pergunta); dois Escs dentro de `DOUBLE_ESCAPE_WINDOW` param o run vivo — o botão Stop pelo teclado. O painel de pergunta **na primeira página não consome** o Esc: ele cobre o composer inteiro, inclusive o Stop, então essa é a única saída de uma pergunta cujo run travou.
- **Paste no composer tem precedência e limites explícitos**: imagem → paths → texto longo (`> 5.000` caracteres) como `TextFile` staged → texto plano. O input limita-se a `10.000` caracteres e qualquer truncagem/rejeição usa `self.failure`; imagens e texto compartilham o mesmo rail de bytes, persistência por chat, restore e prompt `Attached files`. O parser do transcript aceita também o trailer legado `Attached images`.

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

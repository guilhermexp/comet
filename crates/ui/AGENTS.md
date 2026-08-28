# comet-ui — o app gpui

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

O viewport: shell (sidebar de spaces + abas), transcript, composer, painel de terminal, pane de diff, settings, markdown próprio e o kit de animação. Renderiza estado do mirror direto, com notificação por entrada alterada.

## Ownership

Dona de tudo que é pixel. **Não** é dona de comportamento que precisa sobreviver à janela fechada — isso é `comet-engine`. Regra derivada compartilhada com a engine mora em `comet-proto::view`, não aqui.

## Local Contracts

- gpui vem do fork `wingleeio/zed` pinado por rev no `Cargo.toml` raiz. **Não usamos as crates GPL do Zed** (`markdown`, `ui`, `theme`, `editor`) — markdown, componentes e tema são nossos. Puxar uma delas é problema de licença, não de gosto.
- Knobs de captura (`ZERON_OPEN_ROUTE`, `ZERON_OPEN_DIALOG`, `ZERON_OPEN_PICKER`, `ZERON_FORCE_GATE`, `ZERON_DEMO_UPLOAD`) só valem com `ZERON_UI_CAPTURE=1|true|yes|on` e passam **sempre** por `capture::knob`. Toda nova `SettingsSection` entra também no parser de `ZERON_OPEN_ROUTE`, para a própria página continuar alcançável no QA nativo. `std::env::var` direto para uma knob é proibido: exportada uma vez num shell, ela seguia todo `cargo run` daquele terminal — o app abriu na página Accounts por dias. Run normal boota no chat, ponto.
- Tema (`theme.rs`) suporta light e dark. Token que sumiu do upstream (ex: `white_alpha`) se remapeia pro equivalente que o upstream aplicou no código gêmeo — não se recria localmente, senão o light mode fica com wash.
- Animação é camada de **paint**: `with_animation` sobre opacidade nunca altera layout. `prefers-reduced-motion` é honrado.
- Altura de linha em code block = linhas × line-height, independente do highlight; o highlight roda time-sliced em background e entra como run de texto (paint-only).
- Transcript é por **bloco**, não por mensagem: id estável `msgId#blockId`, turno vivo não splitado, re-split na persistência. Eco otimista compartilha o id cunhado no cliente pra persistência não piscar.
- **A entrega do export mora no shell, a formatação não**: as seis ações do menu resolvem as entries e escolhem destino; `chat_export.rs` continua puro. O transcript vem da memória SÓ quando a linha clicada é a selecionada (`export_reads_memory`) — qualquer outra abre um `WatchDocMessages` transitório e consome apenas o frame `Reset`, porque exportar o chat errado é uma falha silenciosa que produz arquivo plausível. Resultado (sucesso e falha) sai pelo `sidebar_notice`, nunca por `notify::post`: banner de desktop é canal de background e é suprimido justamente com o app em foco. `SidebarNotice` carrega o tom junto do texto para que sucesso não herde o vermelho da falha anterior.
- **Chat Transcript Export é uma projeção pura do transcript**: `chat_export.rs` percorre `&[SessionMessageEntry]` uma vez para formar um único `ExportDoc`, e Markdown/Text/JSON derivam somente dele. Nunca lê Run Journal, resolve `output_ref`/`diff_ref` ou importa gpui/I/O; tools usam `zeron_proto::view::tool_chip_content`, e só o `ToolCall` já sanitizado decide quais comando/path aparecem.
  `ExportDoc.messages` usa tipos próprios (`ExportMessage`/`ExportPart`/`ExportTool`),
  nunca `SessionMessageEntry` bruto: output, diff, refs, reasoning, input e
  workflow deixam de ser representáveis antes dos três renderers.
- Chips GitHub/YouTube existem só em mensagens de usuário já enviadas: `url_chips.rs` segmenta e projeta o texto uma vez em `rows_for_entry`, e a row cacheia spans clicáveis. O paint só consome esses spans; input, assistente e outras URLs continuam texto, a pontuação final fica fora do chip e a mensagem persistida nunca muda.
- Cards do usuário são sticky por turno: um clone paint-only do renderer existente ocupa o inset do runway e é empurrado pelo próximo user row. A geometria é per-chat, não altera altura da lista, não substitui o runway e não duplica o original quando ele já ocupa a posição.
- O wrapper externo do sticky é transparente; a oclusão/blur e o bloqueio de mouse/hover subjacente ficam limitados ao card interno arredondado, enquanto wheel/touch continuam chegando ao transcript.
- `TurnSteps` e a projeção de mudanças de arquivo mantêm ids estáveis; previews de Write/Edit renderizam somente o conteúdo limitado que veio do doc.
- Turno assentado nunca renderiza tool viva: `rows_for_entry` força `resolved`/`is_error` e derruba subagente `running` quando `status != Streaming`. A engine assenta as parts no recovery, mas todo doc escrito antes disso ainda carrega `resolved: false` — a guarda é o que cura o histórico.
- `TurnSteps` só existe em turno assentado: enquanto `status == Streaming` o turno inteiro fica aberto, e o fold nasce ao assentar reaproveitando os ids das rows vivas.
- Dentro de `TurnSteps` expandido, grupos de tools mostram os cards individuais por padrão; stdout, invocações e diffs internos continuam fechados, e toggle explícito do usuário prevalece.
- Cards inline de arquivo mantêm expansão, lazy fetch e `ScrollHandle` interno no `Transcript`, keyed pelo row id estável, para virtualização e TurnSteps não resetarem o card.
- Input histórico de arquivo é derivado/highlighted fora do render; corpos grandes usam `uniform_list`, e linhas lógicas patológicas são divididas em paint rows completos de até 512 caracteres antes do cache.
- `crates/ui/src/terminal/` é o **painel de terminal dentro do app**. Não confundir com o `crates/tui` deletado (viewport ratatui do upstream, removido).
- O pane direito é **um único host de tabs**: `right_tabs` + `right_active` (terminal, diff, file preview, subagent, worker) numa strip só. Não existe segundo registro de painel — o par `UtilityPane`/`changes_open` com `Changes`/`TerminalPanel` próprios foi removido porque fazia preview abrir *por cima* do diff, com o outro vivo atrás. `right_pane_open` = coluna visível **e** com ao menos uma tab; sem tab, sem coluna. Nada aqui consulta git: preview abre em pasta sem repo.
- **Vídeo no preview é media document do WebKit, não elemento nosso.** `mp4/mov/m4v/webm` viram `PreviewKind::Video` e vão pro mesmo host nativo do PDF (`loadFileURL:` no `WKWebView` isolado), com uma diferença medida em 2026-08-27: os controles de mídia rodam no switch **legado** `javaScriptEnabled`, não em `allowsContentJavaScript`. Com o legado desligado o player pinta uma barra morta (spinner, `--:--`, sem play) e o vídeo colapsa num canto; com ele ligado e o de conteúdo **desligado** o player nativo funciona inteiro (play, ±15s, timeline, volume, AirPlay, PiP) e a página segue sem poder rodar script próprio — é essa a combinação, e é só o vídeo que a recebe. Vídeo também **não passa pelo teto de bytes**: nada é lido pra memória, e uma gravação de tela de centenas de MB é o caso comum, não a exceção.
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
- **Intake do composer tem precedência e falha visível**: paste segue imagem → paths → texto longo (`> 5.000` caracteres) como `TextFile` staged → texto plano; o input limita-se a `10.000` caracteres e qualquer truncagem/rejeição usa `self.failure`. No drop, imagem usa o upload existente, arquivo de texto dentro do space vira mention relativa e outro arquivo vira `TextFile` staged por path, sem cachear o conteúdo integral; path ilegível nomeia o arquivo em `self.failure`. Imagens e texto compartilham persistência por chat, restore e prompt `Attached files`; o parser do transcript aceita também o trailer legado `Attached images`.
- **A volta do anexo no transcript não é só imagem**: o trailer de refs carrega arquivo de texto/binário igual carrega imagem (`source_path` vai literal, sem upload), então só a extensão distingue — `attachment_ref_is_image`. Ref não-imagem vira chip de documento e **nunca** dispara load: mandá-la pelo caminho de imagem pedia ao device uma figura que não decodifica e a faixa virava uma fileira de molduras tracejadas vazias. A faixa tem altura fixa (flip de load não pode mexer no virtualizer) mas rola no eixo X; `overflow_hidden` comia a cauda de um envio com muitos arquivos sem nenhum sinal.
- **O rail staged vive ACIMA do pill, não dentro dele**: thumbnails têm `56×56`; `TextFile` renderiza chip de `120..200×52` com título de primeira linha, subtítulo de origem/tamanho e remoção no hover. O strip é irmão do pill (`container.children(strip)`) e `target_height = base_height + comment_strip_h` não soma altura de anexo nenhuma — a altura do pill é independente da contagem de anexos. Voltar a medir o strip dentro do pill reintroduz feedback na histerese da altura.
- **Markdown no composer é decoração paint-only**: `markdown_decor::scan` produz ranges por byte sem mutar o texto; o shaping projeta esses ranges sobre o display, mas mention chips e o marked range do IME têm prioridade. Markers permanecem visíveis, inputs acima de 10.000 caracteres pulam o scanner, e somente métricas do shaping base dirigem o flip compacto↔expandido.
- **`RenderCache` só é invalidado por prefixo de row id**: uma row renderiza árvores secundárias sob chaves derivadas (`"{row}-reasoning"`, `"{row}#mermaid-code"`), mas quem invalida só conhece o id da row. Com match exato essas entradas ficavam cacheadas pra sempre e todo bloco de thinking congelava no comprimento do primeiro paint, cortado no meio da palavra. `flats`/`code` não guardam hash de conteúdo — a chave é a única validade, então chave derivada nova exige que o prefixo continue sendo o id da row.
- O painel de pergunta colhe o texto livre em `wizard_advance`, **nunca no call site**: Submit, Enter com input desfocado e o timer de auto-advance do single-select caem todos ali, e quando só um lembrava de gravar, escolher "Other (type your own)" + digitar + Submit mandava a label literal pro agente, que só podia perguntar de novo. `Wizard::advance` recebe o texto como **parâmetro** pra tornar esse esquecimento inexpressável.
- Declinar é primeira classe: o botão **Skip** submete labels vazias e o bridge do OMP traduz isso em `{"cancelled": true, "timedOut": false}` (`omp/mod.rs` `spawn_interactive_answer`) — coisa distinta de timeout e de `confirmed: false`, que seria um "não" de verdade. Skip limpa **todas** as páginas (pick velho iria como resposta real) e nunca é gated por `can_advance`: é a única saída de uma pergunta que o usuário não quer responder.

## Work Guidance

- "Não atualizou na tela" começa em `comet-doc` (mirror), não aqui.
- Não há harness de render: mudança visual se valida rodando `scripts/dev-demo.sh` e olhando. Screenshot antes de dizer pronto.
- Toda raiz de `track_focus` precisa de `.id()` + `.role()` (mais `aria_label` / `aria_value` quando o controle carrega texto). Sem role o gpui deixa o elemento fora da árvore AccessKit, loga `a11y: focused element … has no accessibility node` a cada mudança de foco, e a tecnologia assistiva anuncia a janela inteira em vez do controle focado.

## Verification

- Comandos: `cargo test -p zeron-ui` · `scripts/dev-demo.sh`

| Camada / path | Tier exigido | Como rodar |
|---|---|---|
| `src/**` (estado, derivações, parse de markdown) | unit | `cargo test -p zeron-ui` |
| `src/chat_export.rs` (ExportDoc, artifacts, Markdown/Text/JSON e filename) | unit | `cargo test -p zeron-ui --lib chat_export` |
| `src/shell.rs` (menu EXPORT, resolução do transcript, notice) | unit — decisões puras; render gpui é visual | `cargo test -p zeron-ui --lib export` |
| `src/url_chips.rs` + projeção da row de usuário | unit | `cargo test -p zeron-ui url_chips` · `cargo test -p zeron-ui transcript` |
| `src/markdown/**` | unit — parse e mend têm cobertura própria | `cargo test -p zeron-ui` |
| `src/markdown/render.rs` (invalidação do `RenderCache`) | unit — chaves derivadas caem junto com a row | `cargo test -p zeron-ui invalidate_row` |
| `src/transcript.rs` (render gpui) | none — sem harness de render; validação é visual | `scripts/dev-demo.sh` |
| `src/markdown_decor.rs` + mapping em `src/composer.rs` | unit — scanner, exact-cover/projeção, IME/cap e estabilidade do flip | `cargo test -p zeron-ui markdown_decor && cargo test -p zeron-ui composer` |
| `src/{attachments,composer}.rs` (paste/drop e rail staged) | unit — precedência/cap, classificação de path, persistência e restore | `cargo test -p zeron-ui attachments && cargo test -p zeron-ui composer` |
| `src/settings/projects.rs` (filtro, git remoto, editor/config e decisões de ícone) | unit; render gpui continua visual | `cargo test -p zeron-ui projects` · `scripts/dev-demo.sh` |
| `src/{shell,settings,terminal}/**` (render gpui) | none — sem harness de render; validação é visual | `scripts/dev-demo.sh` |

## Child DOX Index

Subárvores sem doc próprio (ainda não têm regra local além da desta pasta): `shell/` (spaces, tabs), `terminal/` (emulator, panel, view), `settings/`, `markdown/`. Os módulos-raiz `chat_export.rs`, `composer.rs` e `markdown_decor.rs` também permanecem governados por este doc. Adensar aqui quando alguma subárvore ganhar contrato próprio.

- **Settings → Projects mostra o LEDGER, nao o working set.** A sidebar de
  Workers lista `projects[]`, que `remove_project` poda junto com as sessoes;
  esta pagina lista `zeron_workers_unpeel::project_ledger`, que sobrevive a
  poda. Uma linha so do ledger nao tem `project_id`: renomear, "Fill with AI" e
  "Run Auto Doc" ficam indisponiveis nela porque nao ha o que lancar. O
  "Forget" do Danger Zone apaga metadado e NAO toca em sessao — e o verbo
  oposto do "Remove project" do menu de contexto da sidebar. Grupos
  organizacionais compartilham o path do pai e ficam fora; worktrees de path
  próprio continuam. A lista rola no próprio pane.
- **Git so roda para o projeto SELECIONADO.** `status` e os dois commits
  ancora custam processos; a listagem nunca os chama. E `RepositoryState` tem
  um quarto estado que o reference nao tem — `FolderMissing` — porque o ledger
  guarda projetos cuja pasta o usuario pode ter movido: sem ele, uma pasta
  apagada leria como "nao e repo" e ganharia um Initialize Git que falharia.
  Link de repository preserva `GitRemote.host`; nunca reconstrói tudo em
  `github.com`.
- **Config/Worktree são editor, não resumo.** Target suportado + listas shared,
  Unix e Windows normalizam linhas vazias/comentários e só gravam quando o
  conteúdo ou target mudou. Saves são serializados; enquanto um está em voo,
  a fila preserva o draft mais novo por projeto e só avança o baseline depois
  da persistência correspondente. Ícone usa SHA-256 do path, é carregado fora
  do render e reset/forget só podem remover filho direto do diretório app-owned.
- **Falha de setup e sucesso parcial visível.** O worktree criado continua
  selecionável, mas comando + motivo entram em `WorkersModel.error`, sobrevivem
  ao refresh seguinte e nenhum Worker é lançado automaticamente nele.

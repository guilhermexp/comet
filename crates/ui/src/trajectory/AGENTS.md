# trajectory — surface de preview e inspeção da trajetória de execução do Chat

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

Superfície analítica e técnica para visualização, navegação cronológica e inspeção detalhada da trajetória de execução do agente no Chat ativo (`TrajectoryView`). Combina Timeline de 3 lanes fixas, Ledger hierárquico virtualizado, Inspector de 5 abas com Raw Reveal efêmero sob demanda local e Toolbar de controles.

## Ownership

Dona da projeção e apresentação visual do fluxo de trajetória (`TrajectoryViewModel`, `TrajectoryView`, timeline, ledger, inspector, toolbar). **Não** é dona do armazenamento persistente (mora em `zeron-engine::trajectory_store`), da captura de eventos (mora em `zeron-engine::sessions`), nem do protocolo RPC (mora em `zeron-rpc`).

## Local Contracts

- **A surface é dona do próprio watch task e usa `subscribe_checked`**: `TrajectoryView` gerencia sua própria tarefa assíncrona de streaming; o drop da entidade cancela imediatamente a assinatura, evitando tarefas órfãs e vazamentos de recursos.
- **A retomada de stream é baseada em watermark (`TrajectoryCursor`)**: reconexões retomam a partir do último cursor recebido (`after_cursor`), evitando retransmissão de histórico completo. Estados terminais (`ChatDeleted`, `StoreUnavailable`) selam o stream e impedem reaberturas.
- **`AppState` só fornece acesso à engine**: `state.read(cx).engine()` é acessado exclusivamente para invocar RPCs (`watch_trajectory`, `reveal_trajectory_raw`); o `AppState` nunca retém estado, histórico ou dados da surface fechada.
- **Raw Reveal é estado efêmero de apresentação (`RevealState`)**: textos e payloads brutos obtidos sob demanda do dispositivo local nunca são persistidos em disco nem sincronizados entre chats. Troca de registro selecionado, fechamento da aba ou resync limpa o reveal imediatamente para `RevealState::Hidden`.
- **`ROW_HEIGHT` fixo (`px(26.0)`) mantém o virtualizador analítico**: nenhum estado de execução (loading, erro, partial, fold ou reveal) pode alterar a altura de linha do ledger (`uniform_list`), garantindo estabilidade geométrica de scroll.
- **Dado ausente renderiza como `Unavailable` ou `Unsettled`, nunca como zero ou string vazia**: registros sem timestamp físico são categorizados como `SequenceOnly` / `Unavailable`, operações em andamento são marcadas como `Unsettled`, e campos sanitizados ausentes preservam sua semântica explícita.
- **3 lanes fixas na Timeline (`Input`, `Model`, `Tools`)**: classificação determinística por `TrajectoryLane`. A geometria suporta modo `Sequence` (largura uniforme normalizada) e modo `Recorded` (larguras proporcionais ao tempo decorrido, com fallback seguro para sequence quando timings reais não existem).
- **Layout responsivo com breakpoint `TRAJECTORY_SPLIT_THRESHOLD` (`px(600.0)`)**: contêineres $\ge 600\text{px}$ renderizam `TrajectoryLayout::Split` (ledger e inspector em colunas lado a lado); contêineres $< 600\text{px}$ alternam para `TrajectoryLayout::NarrowDetail` (visão exclusiva do inspector com botão de retorno).
- **Busca por texto aplica dimming sem filtrar linhas**: itens não correspondentes recebem `dimmed: true`, mantendo a continuidade e o contexto estrutural/cronológico intactos no ledger.
- **Folds independentes para Turns e Calls**: colapso de turnos e chamadas de ferramentas operam de forma desacoplada; overrides manuais do usuário por linha (`fold_overrides`) têm precedência sobre os toggles globais da toolbar.
- **Seguimento de live-edge com contagem de pendências**: quando o usuário rola para trás, o seguimento automático pausa e novos registros incrementam `pending_live`. O rearme é **só** a ação explícita "Follow Live" na toolbar — rolar de volta até o fim não rearma sozinho, porque um rearme implícito voltaria a arrastar o viewport de quem só estava conferindo o fim da lista.
- **A suspensão do live-edge é derivada da posição do scroll, não de um gesto, e decidida no task do watch antes de enfileirar o catch-up**: `keep_following_live` (ledger.rs) roda em `TrajectoryView::follow_live_edge` a cada item recebido, lendo `offset`/`max_offset` do `UniformListScrollHandle` (tolerância de 2 linhas), o que cobre roda, trackpad, arraste e teclado num único ponto. Decidir no render não funciona: com deltas em taxa maior que o frame rate há sempre um `scroll_to_item` pendente e a checagem nunca rodaria. Enquanto um salto nosso ainda não foi aplicado pelo prepaint, só o usuário pode mover o offset — por isso a view guarda `live_jump_from` (offset no momento do enfileiramento) e compara com o atual: igual é o próprio salto pendente (ex.: "Follow Live" clicado de longe), diferente é o usuário rolando, que vence e descarta o salto pendente.
- **A linha selecionada do ledger usa `theme.accent_wash`, não um token neutro**: no tema claro os neutros (`element_active` 1.16:1, `glass_selected_bg` 1.13:1) somem contra o fundo branco numa lista tão densa; o wash de accent carrega deslocamento de matiz além do de luminância e sobrevive aos dois temas.

## Work Guidance

- Toda lógica de projeção, geometria, hit-testing, resolução de scroll e transição de estados deve residir em funções puras com cobertura de testes unitários em seus respectivos módulos (`model.rs`, `timeline.rs`, `ledger.rs`, `inspector.rs`, `toolbar.rs`, `view.rs`).
- Componentes GPUI de render (`render_timeline`, `render_ledger`, `render_inspector`, `render_toolbar`, `TrajectoryView::render`) não possuem harness de teste automatizado headless; sua validação é visual via `scripts/dev-demo.sh` ou execução headed no app real.
- Função pura testada **não** é feature entregue: as duas passagens nativas encontraram cinco defeitos que a suíte não pegava — ledger e timeline sem `on_click`, callback capturando `cx.to_async()` e abortando com `RefCell already borrowed`, id estourando o painel, seleção invisível no tema claro, e `set_following_live(false)` sem nenhum chamador de produção. Ao mexer aqui, rodar o app de verdade e clicar no que mudou.
- Todo elemento interativo com foco deve manter `.id()`, `.role()` e `aria_label` adequados.

## Verification

| Camada / path | Tier exigido | Como rodar |
|---|---|---|
| `src/trajectory/model.rs` (projeção pura, fold, busca, replay, watermark, lifecycle) | unit | `cargo test -p zeron-ui trajectory::model` |
| `src/trajectory/timeline.rs` (lanes fixas, layout puro, duration mode, hit-testing) | unit (geometria e hit-testing); render gpui é visual | `cargo test -p zeron-ui trajectory::timeline` |
| `src/trajectory/ledger.rs` (geometria de scroll, row height fixo, ancoragem, virtualização) | unit (cálculo de scroll e ancoragem); render gpui é visual | `cargo test -p zeron-ui trajectory::ledger` |
| `src/trajectory/inspector.rs` (tabs disponíveis, summary fields, reveal params) | unit (fields, tabs e reveal); render gpui é visual | `cargo test -p zeron-ui trajectory::inspector` |
| `src/trajectory/toolbar.rs` (ações da toolbar, toggles de fold, duration mode, busca) | unit (transições puras de estado); render gpui é visual | `cargo test -p zeron-ui trajectory::toolbar` |
| `src/trajectory/view.rs` (TrajectoryView, watch lifecycle, watermark resumption) | unit (decisões puras de stream e reveal); render gpui é visual | `cargo test -p zeron-ui trajectory::view` |

## Child DOX Index

Nenhuma subárvore filha.

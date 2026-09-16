## Why

A aba ativa do widget Workers segue **despacho**, não **atividade**. `sync_dispatch_with_recency`
só conhece dois sinais: a contagem daquela aba cresceu, ou o `started_at`/`created_at` mais novo
dela avançou. O estado de execução — `WorkerSemantic::is_active` para workers,
`WorkflowTaskStatus::Running` para workflows e subagentes — nunca entra na escolha.

Duas consequências, as duas reportadas em uso:

- **Worker reativado não puxa foco.** Um worker que já está na lista e volta a trabalhar no meio
  da sessão não cresce contagem e tem `created_at` antigo (é a data de criação, não de atividade).
  Nenhum sinal dispara e a aba fica parada onde estava. Não é caso raro: o `app-state.json` deste
  device tem 70 bindings worker→chat, e os chats mais usados carregam dezenas de workers já
  vinculados — "abrir um worker no meio da sessão" quase sempre é reativar um que já estava lá.
- **Aba morta segura o foco.** Subagente que termina continua com a aba selecionada enquanto um
  worker trabalha ao lado, porque "terminar" também não é evento para essa lógica.

A máquina de estado em si está correta para o que ela modela: a sequência worker → subagente →
worker novo foi reproduzida em sonda e dá `Workers → Subagents → Workers`, inclusive com o worker
antigo podado no mesmo tick e com `created_at` chegando zerado. O buraco é o modelo, não o código.

`updated_at_unix_ms` **não** serve como relógio de atividade e não é usado aqui: ele anda com o
heartbeat do host e com repaint idêntico da TUI (contrato em `crates/ui/AGENTS.md`), então um
Worker parado há 24h pareceria recém-ativo.

## What Changes

- A aba passa a seguir a **atividade viva**. Uma aba toma foco quando ganha linha nova, quando o
  `started_at` mais novo dela avança, **ou quando alguma linha dela passa a rodar** — o que inclui
  o worker reativado.
- Quando a aba em foco fica sem nada rodando e outra ainda tem trabalho vivo, o foco **migra** para
  ela em vez de ficar numa lista encerrada.
- O desempate continua o mesmo: Workflows → Workers → Subagents, do mais grosso para o mais fino,
  porque um worker também gera subagentes embaixo dele.
- A regra continua passando por cima de seleção manual, como hoje.
- A primeira sincronização de um chat continua sendo só baseline, e lista ilegível continua sendo
  ausência, nunca zero.

## Capabilities

### New Capabilities

- `workers-widget-tab-focus`: qual aba do widget Workers fica visível, derivada da atividade viva
  de cada lista.

## Impact

- `crates/ui/src/details_sidebar/widgets.rs`: `sync_dispatch_with_recency` vira `sync_tab_focus`,
  recebendo por aba contagem + ids rodando + `started_at` mais novo; estado ganha o conjunto ativo
  anterior.
- `crates/ui/src/details_sidebar/view.rs`: `render_chat_workers` passa os ids rodando de cada lista.
- DOX: `crates/ui/AGENTS.md` (regra da aba ativa e matriz de cobertura).

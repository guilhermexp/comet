# Agent Elements: catálogo e adoção gradual no streaming

Análise de **09/09/2026**, ligada ao [guia do streaming](streaming-guide.md). Referência: [21st-dev/agent-elements](https://github.com/21st-dev/agent-elements), commit `b04b36cb6381a1dd1a0e86cc7c90564ddcd56d37`. P1–P6 implementados nos renderers nativos; o catálogo abaixo preserva os IDs e o escopo aprovado.

## Compatibilidade e escopo

A biblioteca fornece componentes **React 19/Tailwind/shadcn**, com integração ao AI SDK. O Comet renderiza em **Rust/gpui**. A adoção neste app significa portar a apresentação e as interações selecionadas para os renderers nativos existentes. Instalar o registro shadcn não transforma componentes React em gpui. Uma WebView seria uma mudança de arquitetura separada, fora desta proposta.

O [registro no commit analisado](https://github.com/21st-dev/agent-elements/blob/b04b36cb6381a1dd1a0e86cc7c90564ddcd56d37/public/r/index.json) contém **25 componentes**. O README anuncia 26; usamos o inventário do código como referência. A licença é [MIT](https://github.com/21st-dev/agent-elements/blob/b04b36cb6381a1dd1a0e86cc7c90564ddcd56d37/LICENSE); adaptações de código devem preservar os avisos aplicáveis.

IDs S01–S12, T01–T12 e F01–F06 abaixo pertencem ao guia do streaming. IDs C01–C25 identificam o catálogo externo e permanecem estáveis nas próximas etapas.

## Catálogo completo

“Portar” significa adaptar ao gpui. “Depois” significa candidato fora da primeira troca. “Condicional” exige decidir o contrato de produto ou dispor dos dados indicados.

| ID | Componente / entrada do registro | Correspondência no Comet | Decisão e limite |
|---|---|---|---|
| C01 | `AgentChat` / `agent-chat` | Contêiner do Chat, transcript e composer | **Referência posterior.** Não substituir a arquitetura nativa de Chat, Session e sincronização. |
| C02 | `MessageList` / `message-list` | Projeção S01–S12 e lista virtualizada | **Referência posterior.** Aproveitar organização; manter virtualização, IDs, folds e rolagem nativos. |
| C03 | `InputBar` / `input-bar` | Composer | **Depois.** Layout, anexos e ações; preservar envio, steer, cancelamento e estado do runtime. |
| C04 | `Suggestions` / `suggestions` | Sugestões no Chat vazio | **Condicional.** Só se houver sugestões reais e decisão de exibi-las. |
| C05 | `ModelPicker` / `model-picker` | Seletor de provider/modelo | **Depois.** Apresentação sobre opções e modelo realmente configurados. |
| C06 | `ModeSelector` / `mode-selector` | Controles de modo do composer | **Condicional.** Expor apenas modos suportados pelo harness; não inventar equivalências. |
| C07 | `UserMessage` / `user-message` | S01 User | **Portar depois das tools.** Bolha, tipografia e anexos; verificar largura e relação com mensagem fixada. |
| C08 | `ErrorMessage` / `error-message` | S11 ErrorChip | **Portar.** Tratamento visual de erro; deduplicação F01 continua sendo correção de dados/projeção. |
| C09 | `Markdown` / `markdown` | S02 Markdown, S03 LiveMarkdown e blocos de código | **Portar apresentação.** Manter parser, linguagens, syntax, cache e comportamento incremental nativos. |
| C10 | `SendButton` / `send-button` | Botão enviar/parar | **Depois.** Aparência e estados devem seguir os comandos reais do composer. |
| C11 | `AttachmentButton` / `attachment-button` | Adicionar anexo | **Depois.** Preservar seleção, permissões e upload nativos. |
| C12 | `FileAttachment` / `file-attachment` | Anexos do composer, S01 e S05 InlineImages | **Portar depois das tools.** Chips/preview; não resolve por si a imagem duplicada F02. |
| C13 | `TextShimmer` / `text-shimmer` | Label de atividade pendente | **Opcional.** Efeito visual discreto; não representa conclusão ou progresso real. |
| C14 | `SpiralLoader` / `spiral-loader` | Indicador de atividade | **Adotado no Thinking ativo**, em 24px; desaparece ao concluir, sem spinner redundante. |
| C15 | `BashTool` / `bash-tool` | T01 Exec, detalhe de comando em S06/S08 | **Primeira leva.** Comando e output em painel de código, com rolagem e estado real. |
| C16 | `EditTool` / `edit-tool` | S07 FileChange; T03 WriteFile/T04 EditFile; T05 ApplyPatch conforme dados | **Segunda leva.** Cabeçalho, +/− e diff; sem fabricar conteúdo anterior/novo quando só existe resumo. |
| C17 | `SearchTool` / `search-tool` | T06 Search, T07 Glob, T08 WebFetch, T09 WebSearch | **Portar seletivamente.** Resultados ricos só quando existem fontes e dados estruturados; manter fallback textual. |
| C18 | `TodoTool` / `todo-tool` | T10 Todo, S09 TaskSnapshot | **Portar.** Lista compacta e estados; resolver no-op F06 e identidade Q01 separadamente. |
| C19 | `PlanTool` / `plan-tool` | Sem row nativa própria de plano com aprovação | **Condicional.** Todo e texto Markdown não são automaticamente um plano aprovável. Exige contrato explícito. |
| C20 | `ToolGroup` / `tool-group` | S06 ToolGroup e agrupamento de S08 TurnSteps | **Portar.** Resumo compacto e expansão; unificar contagens/classificação F05 no código nativo. |
| C21 | `SubagentTool` / `subagent-tool` | Spawn em S06/S08 e acesso ao Chat filho | **Portar cabeçalho/status primeiro.** Manter doc filho e lifecycle próprios; não embutir toda a conversa do filho nesta etapa. |
| C22 | `QuestionTool` / `question-tool` | Fluxo de perguntas aguardando resposta, junto ao composer | **Depois.** Mapear pergunta/seleção/resposta ao protocolo; não confundir com simples S10 InputChip. |
| C23 | `McpTool` / `mcp-tool` | T11 Mcp; apresentação estruturada reutilizável para T12 | **Primeira leva.** Nome da operação, argumentos e resultado legíveis, usando apenas payload disponível e sanitizado. |
| C24 | `ThinkingTool` / `thinking-tool` | S04 Reasoning e pensamento em S08 | **Portar.** Uma linha compacta expansível; não repetir título como corpo nem inferir término por timer. |
| C25 | `GenericTool` / `generic-tool` | T02 ReadFile e fallback T12 Unknown | **Primeira leva, adaptado.** Base para tools sem renderer próprio; acrescentar detalhe e estado de erro que o wrapper externo não cobre. |

Não há entradas independentes chamadas `ReadTool`, `WriteTool`, `CodeBlock` ou `TerminalTool` nesse registro. A leitura usa a apresentação genérica; escrita/edição usam `EditTool`; código está em `Markdown`; o cartão de terminal pertence a `BashTool`.

## Base compartilhada antes de trocar cada saída

**B01 · `ToolRowBase`** é um componente interno da biblioteca, não uma 26ª entrada instalável. É a referência mais útil para alinhar ícone, label, detalhe, estado e expansão. Portar seu padrão estendendo `transcript.rs::stream_event_row`; conectar também os cabeçalhos especializados que hoje o contornam, especialmente `render_file_change` (F03).

Contrato de apresentação derivado das solicitações do usuário:

- Ícone, texto e seta próximos, na mesma linha; seta imediatamente após o texto, nunca empurrada para a borda da coluna.
- Seta invisível em repouso e visível ao passar o mouse, sem deslocar o texto ao aparecer.
- Ícone de linguagem suficiente: retirar `js`/`py` redundante do label quando o ícone já informa a linguagem.
- Tipografia e espaçamento consistentes entre tools, grupos, tarefas e reasoning; texto de código em monoespaçada dentro do painel.
- Comando, JSON, output e diff dentro de blocos delimitados, com altura limitada e rolagem quando necessário.
- Estado pendente, concluído ou falho vem dos eventos reais. Erro precisa continuar acessível mesmo em renderer especializado.

O código atual usa 14/22 px para texto compacto e headers de 28 px; esses valores são uma base de comparação, não prova de correspondência visual com as screenshots. Cada troca precisa conferir a imagem nativa, o cálculo de altura e a invalidation da lista.

Outras peças internas úteis são `ToolRenderer`/`routeToolCall` (referência de dispatch), `QuestionPrompt`, `ImageLightbox` e `ToolApprovalFooter`. Não criar uma segunda camada de roteamento em paralelo à atual; não introduzir aprovação apenas porque o componente externo possui esse rodapé.

## Diferenças que impedem copiar o comportamento diretamente

| ID | Evidência no código externo | Adaptação necessária |
|---|---|---|
| A01 | [`tool-row-base.tsx`](https://github.com/21st-dev/agent-elements/blob/b04b36cb6381a1dd1a0e86cc7c90564ddcd56d37/lib/agent-ui/components/tools/tool-row-base.tsx) mantém chevron visível. | Aplicar a regra de hover do usuário; validar também header de diff, que tem layout próprio. |
| A02 | [`bash-tool.tsx`](https://github.com/21st-dev/agent-elements/blob/b04b36cb6381a1dd1a0e86cc7c90564ddcd56d37/lib/agent-ui/components/tools/bash-tool.tsx) limita output a 80 px com `overflow-hidden`. | Manter rolagem interna e acesso a todo o output disponível. Um componente visual não recupera o que o documento já truncou. |
| A03 | [`generic-tool.tsx`](https://github.com/21st-dev/agent-elements/blob/b04b36cb6381a1dd1a0e86cc7c90564ddcd56d37/lib/agent-ui/components/tools/generic-tool.tsx): o wrapper `GenericTool` não renderiza payload e não consome `isError`. | Preservar erro e detalhe de eval/hub/Unknown; aproveitar a base, não esse wrapper isolado. |
| A04 | [`use-tool-complete.ts`](https://github.com/21st-dev/agent-elements/blob/b04b36cb6381a1dd1a0e86cc7c90564ddcd56d37/lib/agent-ui/hooks/use-tool-complete.ts) aciona callback por duração de animação. | Conclusão nativa exclusivamente por lifecycle de tool/run; timers servem apenas à animação. |
| A05 | [`markdown.tsx`](https://github.com/21st-dev/agent-elements/blob/b04b36cb6381a1dd1a0e86cc7c90564ddcd56d37/lib/agent-ui/components/markdown.tsx) usa Streamdown e uma whitelist de fences que não inclui Rust/Python. | Portar estilo preservando linguagens e parser nativos; evitar regressão de syntax highlighting. |
| A06 | [`edit-tool.tsx`](https://github.com/21st-dev/agent-elements/blob/b04b36cb6381a1dd1a0e86cc7c90564ddcd56d37/lib/agent-ui/components/tools/edit-tool.tsx) usa `@pierre/diffs/react`, DOM e conteúdo antigo/novo. | Usar renderização nativa e `FetchToolInput` existente; fallback honesto quando não há corpo do arquivo/diff completo. |

A biblioteca não resolve automaticamente deduplicação de eventos, repetição de imagens, resumos divergentes, no-op de tarefas ou sincronização de subagentes. Os achados F01–F06/Q01–Q04 continuam no guia e devem acompanhar as trocas relacionadas.

## Ordem proposta de adoção

| Etapa | Escopo concreto | Critério para avançar |
|---|---|---|
| P1 | **B01 + C15 BashTool**: header comum e detalhe Exec | Ícone/texto/seta alinhados; hover sem salto; comando/output enquadrados; pending, sucesso, erro, vazio e output longo; alturas e scroll corretos. |
| P2 | **C23 McpTool + C25 GenericTool** | eval, hub, MCP e tool desconhecida: JSON/output no painel, erro acessível, sem rótulo de linguagem repetido; dados ausentes não viram conteúdo inventado. |
| P3 | **C16 EditTool** | Write/Edit e ApplyPatch conforme payload: +/−, syntax, expansão junto ao texto, corpo/erro e carregamento sob demanda. |
| P4 | **C20 ToolGroup + C24 ThinkingTool** | Contagem somente no resumo final recolhível; chamadas intermediárias diretas e conteúdo expandido sem duplicação de título; preservação de estado de expansão. |
| P5 | **C18 TodoTool + C17 SearchTool + C21 SubagentTool**, uma família por vez | No-op e identidade de tarefas; resultados reais; lifecycle e acesso ao filho; fallback quando faltar payload. |
| P6 | **C08 ErrorMessage + C09 Markdown + C07 UserMessage + C12 FileAttachment**, uma saída por vez | Tipografia/código, falhas e imagens; validar F01/F02 na camada responsável, além do visual. |
| P7 | **C22 e componentes de composer/shell C01–C06/C10–C11/C13/C19**, selecionados depois | Decisão específica de produto e contrato do harness; sem ampliar a refatoração do streaming por conveniência. |

Em cada etapa: reabrir a cadeia DOX, delimitar a mudança/OpenSpec quando aplicável, usar `implement`, testar seams de comportamento e conferir render nativo em repouso/hover/aberto, viewport estreito, streaming/assentado, replay e erro. Não declarar paridade visual apenas com testes unitários.

## Evidência e limite desta etapa

P1–P6 adaptados em `transcript.rs`, `turn_steps.rs`, `file_change.rs` e `markdown/render.rs`. Licença da referência preservada em [agent-elements-LICENSE.txt](agent-elements-LICENSE.txt). Nenhuma dependência React foi adicionada.

- **P1–P2:** headers e disclosures comuns; comandos e argumentos JSON com syntax no painel; erros/outputs disponíveis no fallback.
- **P3:** EditToolDiffCard nativo com cabeçalho integrado, gutters antiga/nova, indicadores +/−, preview até 260px e expansão com scroll até 520px. Numeração relativa ao trecho fornecido; caudas truncadas e linhas fragmentadas não recebem posições inventadas. `FetchToolInput` e virtualização preservados.
- **P4:** contagens por categoria somente no total final recolhível, chamadas intermediárias diretas, reasoning sem repetição do título e espaçamento entre narrativa/atividade mesmo dentro do turno expandido.
- **P5:** tarefas por ocorrência do título, no-ops ocultos, falhas preservadas, estado real do subagente e resultados disponíveis no fallback de busca.
- **P6:** tipografia e superfícies de código consistentes, bolha sem sombra, anexos compactos, deduplicação de erro/imagem apenas dentro de cada entrada.

Validação: `cargo test --workspace` passou; após o ajuste final do Edit, `cargo test -p zeron-ui --lib` passou com 1.200 testes e `cargo build` passou. Revisão visual usa o mock opt-in `ZERON_MOCK_ELEMENTS=1` combinado com code/subagent/error num daemon e app temporários isolados. A aceitação visual final do usuário continua distinta desses testes.

P7 permanece fora desta entrega. Não há equivalência automática de planos aprováveis, perguntas, busca rica sem fontes estruturadas ou lifecycle baseado em timers. O documento durável mantém seus limites de payload; deduplicação de apresentação não reescreve o histórico.

Ajuste solicitado em 09/09, 13:27: removidos os cabeçalhos de contagem dos grupos intermediários em streaming e nos detalhes do turno concluído. O total final de TurnSteps é preservado, assim como invocação/output e ordem de cada chamada.

Ajuste de ThinkingTool/SpiralLoader: header fixo `Thinking`/`Thought`, conteúdo somente na expansão mesmo quando curto; espiral nativa de 24px baseada nos vetores/keyframes da referência, quatro ciclos rápidos e dois lentos, ausente quando concluída e estática quando ativa com movimento reduzido. Ação e detalhe usam tons distintos nos headers.

Thinking/Thought abre o corpo por padrão e mantém a seta visível também em repouso; o usuário pode recolher manualmente. A regra de hover continua nas outras tools e no resumo do turno.

Comandos: `Ran command`/`Running command` com detalhe sans discreto; comandos compostos/longos mostram até quatro executáveis. Expansão integra header e payload numa moldura única, mantendo invocação completa em mono e scroll. Reasoning aberto usa recuo com linha vertical.

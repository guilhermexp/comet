# zeron-harness — adaptadores de coding agent

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

Esconde *qual* agente está rodando atrás de uma interface só. O trait `Harness` mais três implementações: **claude-code** (subprocesso falando stream-json), **codex** (app-server JSON-RPC) e **mock** (determinístico, usado pelo demo e pelos testes). Junto: mailbox de steering, `requestInput` e os catálogos de modelo/reasoning/opções.

## Ownership

Dona de tudo que é específico de vendor. A engine acima só conhece o trait — se um `if provider == "claude"` apareceu fora daqui, a abstração vazou.

## Local Contracts

- Catálogo de modelo/opção é dado do harness, não constante espalhada na UI.
- Steering é mailbox: comando chega enquanto o run está vivo e é entregue no ponto de corte; sem run vivo, vira o próximo turno.
- Resolução de ambiente de shell (`shell_env_resolution.rs`) existe porque o agente herda o ambiente errado quando invocado fora de um shell de login — mudanças aqui quebram o spawn em máquinas reais sem quebrar teste.
- Fixtures de transcript vivem em `tests/fixtures/` — é o que fixa o parse de stream-json contra mudança de formato do vendor.
- Write/Edit progressivos emitem `ToolCallPreview` transitório com o mesmo id: o parser consome cada chunk uma vez, retém tail de 8 KiB e coalesce refresh a cada 16 KiB; só o `ToolCall` autoritativo posterior carrega input completo.
- Anexo de imagem no OMP **nunca falha por tamanho**: o texto do prompt já lista cada arquivo como caminho local (`with_attachments`) e o OMP roda nesta máquina com ferramentas de arquivo próprias, então um conjunto acima do frame de 2 MiB degrada para esses caminhos e a rodada segue. Três screenshots de Retina estouram o orçamento em base64 — recusar o turno inutilizava um envio rotineiro. O preflight roda **antes** do spawn: é trabalho de filesystem que não depende do filho.
- Falha de transporte do OMP carrega as últimas 4 linhas de stderr do filho (`fatal_message`). "OMP RPC exited before ready" sozinho manda o usuário para um log de debug que ninguém abre.
- **O handshake do OMP é prazo, não espera.** Medido em 2026-08-28: o binário (126 MB) emite `{"type":"ready"}` em ~0,9s com a linha de comando exata da produção, overlay `--config` incluído. Os 5s antigos pareciam folgados por isso, mas eram o **único** prazo e não tinham escape — `registry.rs` constrói `OmpHarness::new()` puro, e `with_timeouts` só os testes chamam. Hoje o default é 15s e `ZERON_OMP_HANDSHAKE_MS` sobrepõe; `0` e lixo caem no default de propósito, porque o knob existe para afrouxar sob pressão de máquina, nunca para desligar o OMP por erro de digitação.
- O timeout de handshake **loga o que o `fatal_message` não alcança**: o `omp` não escreve nada em stderr no caminho feliz, então a string voltava nua e a próxima ocorrência ficava tão cega quanto a primeira. O `warn` carrega prazo real, estado do filho (`alive` / `exited(status)`) e quantos frames de stdout saíram antes do prazo — é isso que separa "boot lento" de "processo travado" de "protocolo mudo", que pedem correções diferentes.
- Frames anteriores ao `ready` ficam num buffer local, **não** no canal de eventos: o `event_rx` só é retirado depois que `start` retorna, então mandar para ele durante o handshake podia encher os 256 slots e travar o reader no `send().await` — e o `ready` seguinte nunca seria lido, dando um "handshake timed out" sem nenhuma relação com lentidão. Hoje o `ready` é a primeira linha e o alçapão não morde; continua fechado porque `--no-extensions` **não** silencia os `extension_ui_request`.
- **O OMP corta o JSONL em 1 MiB por linha, e a v1 do protocolo não sobrevive a isso.** Um frame maior é degradado pelo filho para `{"success":false,"error":"RPC response exceeded the transport limit"}` — nada de truncamento visível, a resposta simplesmente vira erro. O catálogo de modelos mede **1,2 MiB em 550 linhas** (medido em 2026-08-28), então `get_available_models` nunca cabia: o picker ficava só com o erro e trocar de modelo virava impossível. `start` pede a **v2** logo depois do `ready` (`negotiate_protocol`, só quando o `ready` anuncia a versão em `supportedProtocolVersions`) e o reader remonta os `rpc_chunk` base64 pelo `ChunkAssembler`. Recusa da negociação é `warn`, não erro: custa os frames grandes, não a sessão. Pedaço perdido **falha alto** em vez de remontar JSON picotado.
- Todo launch do OMP passa `--config` com um overlay que desliga os quatro roots de skill de **nível user** (`~/.claude`, `~/.agents`, `~/.codex`, `~/.pi`); roots de nível **project** ficam ligados. Sem isso o chat herda o catálogo pessoal por cima do do repo — medido: 98 de 120 skills num repo alheio, incluindo skills de um workspace vazando nos outros pelos mirrors sob `~`. O overlay precisa ser YAML **aninhado**: chave pontilhada (`skills.enableClaudeUser`) é ignorada em silêncio, e parece ter funcionado. Skill vinda de plugin não passa por esses toggles e sobrevive de propósito — plugin é assunto de `--no-extensions`, não deste overlay. Falha de escrita do overlay degrada para "sem escopo", nunca mata o turno.
- Ordem de chave de JSON **não é contrato**: `serde_json/preserve_order` chega neste crate por unificação de feature do workspace, então `Value::Object` é ordenado sob `cargo test -p zeron-harness` e é ordem-de-inserção no app real. Teste que compara payload serializado compara `Value` parseado, nunca string; fixture que extrai campo faz parse ciente de profundidade, nunca regex guloso — um `.*"type":"..."` pegava o `type` aninhado do `message` e o dispatch caía sem resposta só na configuração que o usuário roda.
- O system prompt do orquestrador tem **duas metades e dois gates independentes**: a base entra quando o cwd é o workspace do orquestrador (`is_orchestrator_workspace`) e o bloco de delegação só é anexado quando a tool `workers` vai de fato ser registrada (`workers_tool_expected`, o mesmo predicado que guarda o `WorkersBridge`). Compor em `orchestrator_system_prompt` é o que impede o texto de mandar delegar numa sessão que não recebeu worker — e é por isso que o mandato dentro do bloco é incondicional. Mandato hedgeado ("when the `workers` tool is available") o modelo lê como opcional: ele inspeciona `list_projects`/`list_presets` e nunca lança. O bloco também precisa **nomear a outra substância** (`task`), senão a seção de delegação genérica do harness vence por default.
- **Normalização de tools do OMP**: `normalize_tool` mapeia `grep` para `ToolCall::Search` (com `pattern` e `path` opcional) e `glob` para `ToolCall::Glob` (o padrão vem do campo `path` do protocolo OMP; `pattern` é só fallback quando `path` não veio, porque padrão vazio não é erro — é chip vazio no transcript e sinal perdido no matching de Worked Projects).

## Work Guidance

- Vendor mudou o formato de saída? A correção é uma fixture nova + o parse, nunca um `if` no consumidor.
- Adicionar harness novo = implementar o trait + catálogo + fixture de transcript. Nada mais deve precisar mudar.

## Verification

- Comandos: `cargo test -p zeron-harness`

| Camada / path | Tier exigido | Como rodar |
|---|---|---|
| `src/**` (parse, mailbox, catálogos, composição do system prompt) | unit | `cargo test -p zeron-harness` |
| `tests/{claude,codex}.rs` | integration — contra fixtures | `cargo test -p zeron-harness` |
| `tests/shell_env_resolution.rs` | integration | `cargo test -p zeron-harness` |

## Child DOX Index

Sem filhos.

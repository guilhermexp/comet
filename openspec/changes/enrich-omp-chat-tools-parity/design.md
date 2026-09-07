## Context

See proposal.md for motivation. No OMP RPC wire format changes: the CLI already emits standard frames. In `crates/harness/src/omp/normalize.rs`, `normalize_tool` only recognizes 8 tool names, dropping `web_search` and `fetch` to `ToolCall::Unknown`. In `crates/proto/src/view.rs`, `tool_chip_content_raw` treats all `ToolCall::Unknown` generically unless prefixed by `Agent: `. Output extraction in `tool_output` stringifies complex objects into raw JSON without checking inner text fields.

The first implementation enriched `view.rs` for `hub`/`eval` but the transcript never saw those arguments: `sanitize_tool_call` drops `Unknown`/`Mcp` input except a subagent spawn badge. Workers MCP results are also pretty-printed JSON, so the 160-character summary collapses to `{…`.

## Goals / Non-Goals

**Goals:**
- Mapear `web_search` e `fetch` para os tipos nativos existentes de `ToolCall`.
- Permitir que ferramentas integradas do OMP (`hub`, `eval`, `workers`) tenham apresentação informativa (operação, alvos, contexto) e ícones semânticos em vez de `Ran tool` / `Called tool comet-workers · workers`.
- Preservar só identificadores curtos desses tools na política de privacidade do doc, para o chip sobreviver à sanitização.
- Extrair texto limpo de saídas estruturadas, inclusive compactando JSON pretty-printed, para que o resumo de 160 caracteres preserve conteúdo útil.

**Non-Goals:**
- Não alterar o formato durável do CRDT Loro ou adicionar novas variantes ao enum `ToolCall` (reutilizar `ToolCall::Unknown` / `ToolCall::Mcp` enriquecidos).
- Não criar widgets complexos interativos de TUI no transcript para eval/hub/workers.
- Não persistir corpos de `code`, `prompt`, `initial_text` ou mensagens IRC.

## Decisions

1. **Reutilizar `ToolCall::Unknown` / `Mcp` com whitelist de chip no sanitizer**: especializar a renderização em `tool_chip_content_raw` e deixar `sanitize_tool_call` guardar só identificadores curtos (`op`/`name`/`to`/`from`/`application`/`ids` para hub; `language`/`title` para eval; `action`/`session_id`/`project_id`/`name`/`project` para workers). O resto do input continua no run journal local.
2. **Extração de Texto Prioritária em `tool_output`**: inspecionar `"text"`, `"output"`, `"stdout"`, `"message"` (e strings diretas em arrays de blocos). Se o texto extraído for JSON, compactar para uma linha (e preferir campos textuais internos) para o summary de 160 caracteres não virar `{…`.
3. **Mapeamento Direto para Variantes Existentes**: `web_search` mapeia para `ToolCall::WebSearch { query: ... }` e `fetch` mapeia para `ToolCall::WebFetch { url: ..., prompt: None }`.

## Risks / Trade-offs

- **Compatibilidade**: Nenhuma mudança de schema ou quebra de sync; `ToolCall` permanece byte-compatível. Devices antigos ignoram campos extra no `input` sanitizado.
- **Payloads desconhecidos**: Se o input de `hub`, `eval` ou `workers` for malformado ou nulo, o fallback silencioso para o nome da tool é mantido.
- **Whitelist vs tamanho**: a política continua sendo chave allowlisted, não um size cap. Títulos de eval longos entram inteiros; corpos de código continuam fora.

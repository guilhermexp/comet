## Purpose

Define o mapeamento semântico, apresentação visual contextual e extração limpa de saída para ferramentas nativas do runtime OMP no Chat Transcript do Comet.

## ADDED Requirements

### Requirement: Mapeamento de Ferramentas Web do OMP
The OMP event normalizer MUST map search and fetch tool invocations to native typed variants (`ToolCall::WebSearch` and `ToolCall::WebFetch`).

#### Scenario: Normalização de web_search
- **WHEN** OMP emite uma tool invocation com nome `web_search` e argumentos `{ "query": "pesquisa" }`
- **THEN** O evento é normalizado como `ToolCall::WebSearch { query: "pesquisa" }`
- **Test:** `unit` (`crates/harness/tests/omp_rpc.rs`)

#### Scenario: Normalização de fetch
- **WHEN** OMP emite uma tool invocation com nome `fetch` e argumentos `{ "url": "https://example.com" }`
- **THEN** O evento é normalizado como `ToolCall::WebFetch { url: "https://example.com", prompt: None }`
- **Test:** `unit` (`crates/harness/tests/omp_rpc.rs`)

### Requirement: Apresentação Semântica de Ferramentas OMP Conhecidas
The visual projection of `ToolCall::Unknown` for built-in OMP tools such as `hub` and `eval` MUST derive human-readable labels and operational details from their input arguments.

#### Scenario: Detalhe semântico do Hub
- **WHEN** uma ferramenta `hub` é invocada com operação `start` e argumento `name: "servico"`
- **THEN** o detalhe do chip apresenta a operação e o alvo (`start servico`) em vez de ficar vazio ou genérico
- **Test:** `unit` (`crates/proto/src/view.rs`)

#### Scenario: Detalhe semântico do Eval
- **WHEN** uma ferramenta `eval` é invocada com argumento `language: "js"` e título opcional
- **THEN** o detalhe do chip identifica a linguagem e contexto da célula
- **Test:** `unit` (`crates/proto/src/view.rs`)

### Requirement: Identificadores Curtos Sobrevivem à Política de Privacidade
The render-only sanitizer MUST keep a whitelist of short chip identifiers for `hub`, `eval`, and the Workers MCP tool, and MUST drop payloads such as code, prompts, and message bodies. Presentation of those tools MUST still derive operation and target after sanitization.

#### Scenario: Hub após sanitizar
- **WHEN** `hub` é invocado com `{ "op": "start", "name": "servico", "text": "payload secreto" }` e o call passa por `sanitize_tool_call`
- **THEN** o input persistido contém `op` e `name`, não contém `text`, e o detalhe do chip é `start servico`
- **Test:** `unit` (`crates/doc/src/parts.rs`)

#### Scenario: Eval após sanitizar
- **WHEN** `eval` é invocado com `{ "language": "js", "title": "Checking logs", "code": "secret()" }` e o call passa por `sanitize_tool_call`
- **THEN** o input persistido contém `language` e `title`, não contém `code`, e o detalhe do chip identifica linguagem e título
- **Test:** `unit` (`crates/doc/src/parts.rs`)

#### Scenario: Workers após sanitizar
- **WHEN** a tool MCP `workers` é invocada com `{ "action": "wait_for_status", "session_id": "worker-1", "initial_text": "briefing" }` e o call passa por `sanitize_tool_call`
- **THEN** o input persistido contém `action` e `session_id`, não contém `initial_text`, e o detalhe do chip apresenta a ação e o alvo
- **Test:** `unit` (`crates/doc/src/parts.rs`)

### Requirement: Apresentação da Tool Workers
The visual projection of the Comet Workers MCP tool MUST name the dispatched action instead of repeating the server and tool names.

#### Scenario: Chip de wait_for_status
- **WHEN** `ToolCall::Mcp` tem `tool: "workers"` e `action: "wait_for_status"` com `session_id: "worker-1"`
- **THEN** o chip apresenta a ação e o alvo (`wait_for_status worker-1`) em vez de `comet-workers · workers`
- **Test:** `unit` (`crates/proto/src/view.rs`)

### Requirement: Extração de Texto Limpo em Resultados Estruturados
The tool output extractor MUST prioritize extracting readable text from common string fields before falling back to raw JSON object serialization. Pretty-printed JSON content MUST be compacted to a single line so a 160-character transcript summary keeps payload fields instead of `{…`.

#### Scenario: Resultado contendo campo de texto em objeto JSON
- **WHEN** o resultado de uma tool é um objeto JSON contendo `{ "text": "log de saída", "details": { ... } }`
- **THEN** `tool_output` extrai `"log de saída"` como string de saída legível, evitando o truncamento de chaves JSON
- **Test:** `unit` (`crates/harness/src/omp/normalize.rs`)

#### Scenario: Content array com JSON pretty-printed
- **WHEN** o resultado contém `content: [{ "type": "text", "text": "{\n  \"session_id\": \"worker-1\",\n  \"launched\": true\n}" }]`
- **THEN** `tool_output` devolve JSON compacto em uma linha que inclui `session_id` e não começa por uma chave `{` isolada
- **Test:** `unit` (`crates/harness/src/omp/normalize.rs`)

#### Scenario: Strings diretas em arrays de blocos
- **WHEN** o resultado contém `content: ["primeira linha", "segunda linha"]`
- **THEN** `tool_output` junta as strings em texto legível em vez de serializar o array como JSON
- **Test:** `unit` (`crates/harness/src/omp/normalize.rs`)

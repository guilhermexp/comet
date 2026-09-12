## 1. Mapeamento de Ferramentas e Saídas no Harness

- [x] 1.1 Mapear `web_search` e `fetch` para `ToolCall::WebSearch` e `ToolCall::WebFetch` em `normalize_tool`. files: `crates/harness/src/omp/normalize.rs`, `crates/harness/tests/omp_rpc.rs`.
- [x] 1.2 Extrair campos textuais legíveis em `tool_output` de objetos JSON estruturados antes de fallback para JSON bruto. files: `crates/harness/src/omp/normalize.rs`, `crates/harness/tests/omp_rpc.rs`.
- [x] 1.3 Compactar JSON pretty-printed e aceitar strings diretas em `content[]` em `tool_output`. files: `crates/harness/src/omp/normalize.rs`.

## 2. Apresentação Semântica de Ferramentas OMP na UI

- [x] 2.1 Enriquecer rótulo e detalhe de `ToolCall::Unknown` para `hub` e `eval` em `tool_chip_content_raw` e `tool_presentation`. files: `crates/proto/src/view.rs`.
- [x] 2.2 Atribuir ícones semânticos dedicados para ferramentas OMP conhecidas (`eval`, `hub`) em `tool_icons`. files: `crates/ui/src/tool_icons.rs`.
- [x] 2.3 Apresentar a action do Workers MCP no chip em vez de `comet-workers · workers`. files: `crates/proto/src/view.rs`.

## 3. Privacidade do Doc

- [x] 3.1 Whitelist de identificadores curtos de `hub`, `eval` e `workers` em `sanitize_tool_call`, com testes que passam pelo sanitizer antes da apresentação. files: `crates/doc/src/parts.rs`.

## 4. Verificação e Testes

- [x] 4.1 Executar suite de testes unitários nas crates afetadas. verify: `cargo test -p zeron-doc` && `cargo test -p zeron-proto` && `cargo test -p zeron-harness` && `cargo test -p zeron-ui`.
- [x] 4.2 Validar conformidade OpenSpec com `openspec validate enrich-omp-chat-tools-parity --strict`.

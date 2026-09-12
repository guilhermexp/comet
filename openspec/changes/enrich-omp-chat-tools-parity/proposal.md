## Why

O Chat nativo do Comet trata ferramentas centrais do runtime OMP (como `hub`, `eval`, `web_search` e `fetch`) como ferramentas genéricas desconhecidas (`Ran tool <name>`), apresentando payloads e resultados JSON crus truncados em 160 caracteres. Além disso, a intenção declarada da execução (`intent` / `args.i`) é descartada pelo normalizador, deixando o trailer de atividade em segundo plano no chat exibindo apenas palavras aleatórias rotativas (`FLAVOUR_WORDS`, como "Plotting...") em vez da operação real. Esta change eleva a fidelidade visual e semântica do chat para ferramentas nativas do OMP.

## What Changes

- **Mapeamento de Ferramentas Web no OMP**: mapear `web_search` e `fetch` para `ToolCall::WebSearch` e `ToolCall::WebFetch` em `normalize_tool`.
- **Apresentação Semântica de Ferramentas OMP**: enriquecer `ToolCall::Unknown` para `hub` (mostrando operação `op`, alvos e jobs) e `eval` (mostrando linguagem e título da célula) com rótulos semânticos e ícones dedicados em `zeron-proto::view` e `zeron-ui::tool_icons`.
- **Extração Limpa de Saída de Ferramentas**: em `tool_output`, extrair campos textuais diretos (`text`, `output`, `stdout`, `message`) de resultados estruturados em JSON antes de recorrer à serialização de objeto bruto, evitando o corte prematuro de 160 caracteres sobre chaves JSON.
- **Preservação de Intenção de Ferramenta**: capturar o campo `intent` ou `args.i` no início da execução de ferramentas no OmpNormalizer para disponibilizar a atividade real do agente.

## Capabilities

### New Capabilities
- `omp-chat-tools-presentation`: mapeamento semântico, extração de texto legível de saídas e apresentação especializada de ferramentas nativas OMP no Chat Transcript.

### Modified Capabilities
Nenhuma capability existente tem requisitos modificados de forma incompatível.

## Impact

- `crates/harness/src/omp/normalize.rs`: mapeamento de `web_search`, `fetch`, `intent` e sanitização de `tool_output`.
- `crates/proto/src/view.rs`: detalhe e rotulagem para ferramentas `hub` e `eval`.
- `crates/ui/src/tool_icons.rs`: ícones dedicados para ferramentas OMP conhecidas.

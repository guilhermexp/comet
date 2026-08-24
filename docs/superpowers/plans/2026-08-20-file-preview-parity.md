# File Preview Parity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Portar o file preview 1:1 do Orchestrator.dev para o Comet em GPUI nativo.

**Architecture:** A árvore Files emite eventos de abertura; cada arquivo vira uma `RightSurface` no mesmo host de Terminal/Diff; `file_preview` possui classificação, leitura segura e viewers somente leitura, sem faixa de abas própria. Markdown e syntax highlighting reutilizam módulos internos existentes.

**Tech Stack:** Rust, GPUI, zeron-syntax, markdown renderer do zeron-ui, serviços nativos macOS.

## Global Constraints

- Orchestrator.dev é a fonte de verdade visual e comportamental.
- Preview disponível em Orchestrator e Workers.
- Nenhuma WebView React ou frontend duplicado.
- Toda leitura deve permanecer confinada ao checkout e fora da thread de UI.
- TDD obrigatório antes de código de produção.

---

### Task 1: Modelo de abas e classificação

**Files:**
- Create: `crates/ui/src/file_preview/model.rs`
- Create: `crates/ui/src/file_preview/mod.rs`
- Modify: `crates/ui/src/lib.rs`
- Test: `crates/ui/src/file_preview/model.rs`

**Interfaces:**
- Produces: `PreviewKind`, `PreviewTabs`, `PreviewTab`, `classify_preview_kind`.

- [ ] Escrever testes vermelhos para classificação por extensão, deduplicação, troca de aba, fechamento e isolamento por contexto.
- [ ] Rodar `cargo test -p zeron-ui file_preview::model --no-default-features` e confirmar falha pelo módulo ausente.
- [ ] Implementar o modelo mínimo.
- [ ] Rodar novamente e confirmar green.

### Task 2: Leitura segura e preparação de conteúdo

**Files:**
- Create: `crates/ui/src/file_preview/loader.rs`
- Test: `crates/ui/src/file_preview/loader.rs`

**Interfaces:**
- Produces: `load_preview(root, relative_path) -> Result<LoadedPreview, PreviewLoadError>`.

- [ ] Escrever testes vermelhos para path traversal, symlink externo, limite de tamanho, UTF-8, imagem e unsupported.
- [ ] Rodar os testes focais e confirmar os erros esperados.
- [ ] Implementar canonicalização, limites e leitura por tipo.
- [ ] Rodar os testes focais e confirmar green.

### Task 3: Viewers Markdown e código

**Files:**
- Create: `crates/ui/src/file_preview/view.rs`
- Create: `crates/ui/src/file_preview/code_view.rs`
- Create: `crates/ui/src/file_preview/markdown_view.rs`
- Test: módulos acima.

**Interfaces:**
- Consumes: `LoadedPreview`, `PreviewKind`.
- Produces: `FilePreview` Entity e renderização read-only.

- [ ] Escrever testes vermelhos para cabeçalho, abas, linhas e árvore Markdown.
- [ ] Implementar renderização mínima usando `zeron_syntax` e `crate::markdown`.
- [ ] Rodar testes focais e corrigir até green.

### Task 4: HTML, imagem, PDF e dados

**Files:**
- Create: `crates/ui/src/file_preview/media_view.rs`
- Modify: `crates/ui/src/file_preview/view.rs`
- Test: módulos acima.

**Interfaces:**
- Produces: viewers somente leitura e estado unsupported.

- [ ] Escrever testes vermelhos para roteamento e estados de erro.
- [ ] Implementar HTML, imagem, PDF, CSV/planilha e unsupported com limites.
- [ ] Rodar testes focais e confirmar green.

### Task 5: Integração com Files e Shell

**Files:**
- Modify: `crates/ui/src/details_sidebar/view.rs`
- Modify: `crates/ui/src/shell.rs`
- Modify: `crates/ui/src/settings.rs`
- Test: módulos acima.

**Interfaces:**
- Produces: `DetailsSidebarEvent::OpenFile`, painel central, abas por contexto e seleção sincronizada.

- [ ] Escrever testes vermelhos do evento e do ciclo abrir/alternar/fechar.
- [ ] Conectar a árvore ao Shell e persistir o estado relevante.
- [ ] Rodar testes focais e confirmar green.

### Task 6: Gates e QA visual

**Files:**
- Modify somente o necessário a partir dos defeitos observados.

- [ ] Rodar `cargo test -p zeron-ui`.
- [ ] Rodar `cargo fmt --all --check`.
- [ ] Rodar `cargo check --workspace`.
- [ ] Rodar o detector Impeccable nos arquivos de UI alterados.
- [ ] Abrir o Comet e o Orchestrator.dev lado a lado; validar Markdown, código, HTML, troca/fechamento de abas e sidebar sincronizada em no máximo duas rodadas visuais.

## Why

Um Worker parado (ou o histórico de um vivo) aparece embaralhado no terminal: palavras novas por cima de texto velho, tabelas cortadas, `⎿ (timeout 10m)` vazando entre linhas. O Claude Code, o pi e o codex desenham em diferencial: sobem N linhas, reescrevem só as células que mudaram e pulam as iguais com cursor-forward. Esse N só vale no grid em que o TUI rodava. O `output.bin` não guarda esse grid, então o terminal decodifica o log no grid do painel (ou em 300 colunas, para Worker parado) e o cursor cai nas linhas erradas. Com o log real do `sec-estorno`, 120×24 sai limpo e 120×60 reproduz o defeito do print.

## What Changes

- O session host do Unpeel vendorizado grava `pty-geometry.jsonl` no diretório da sessão. É uma linha `{offset, cols, rows}` no launch e outra a cada resize que muda o grid, com o offset lifetime do primeiro byte produzido naquele tamanho.
- `zeron-workers-unpeel` expõe `read_pty_geometry` e, para sessões gravadas antes do diário, `estimate_legacy_geometry`: a largura da régua `─` que ocupa a linha inteira, em 24 linhas.
- No catch-up do histórico, o terminal de Workers decodifica cada trecho no grid gravado e só assume o grid do painel quando o histórico termina. Worker parado reflui a tela final para o painel, como antes.
- Worker parado, com o histórico completo, mantém a largura em que foi desenhado e só a altura segue o painel. Em painel mais estreito, a tela rola na horizontal (gesto horizontal ou Shift+roda) em vez de refluir e quebrar as linhas.
- A aba de Worker aberta pelo chat passa a saber se o Worker parou. Antes ela o tratava como vivo.
- Sem diário e sem régua, fica o comportamento anterior.

## Capabilities

### Modified Capabilities

- `worker-terminal-initial-presentation`: o histórico é decodificado no grid em que foi produzido.

## Impact

- `third_party/unpeel/crates/unpeel-core/src/session_host.rs`, `terminal_viewport.rs`: diário e `TerminalViewportState::size`.
- `crates/workers-unpeel/src/lib.rs`: `WorkersGeometryMark`, leitura e estimativa legada.
- `crates/ui/src/workers/terminal.rs`: `feed_at_recorded_geometry`, catch-up guiado pelas marcas, largura congelada e scroll horizontal.
- `crates/ui/src/terminal/{panel,view}.rs`: `GridSnapshot::first_col` desloca a pintura.
- `crates/ui/src/shell.rs`: `add_worker_surface` repassa `stopped`.

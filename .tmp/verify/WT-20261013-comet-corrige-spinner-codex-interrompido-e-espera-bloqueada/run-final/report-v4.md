# WT-20261013 — report v4

Fonte do código: `17381e4ed24993204a7b093b52c6de112476b99b` em `fix/codex-pause-hook-wait-20261013`.
Pai: `eb9a8e497ced41495eacfe918859205eee5a846f`.
Baseline: `e80142973ec72fcf7ed7297d7a0eb7c453485241`.

Este arquivo é o único conteúdo do commit filho. Não é aceite. Build, gate, `zeron`, MCP e widget continuam bloqueados. O produto não aperta `t` e não confia o hook. `wait_until_matching` não foi alterado: o achado minor do review-v3 fica refutado, como em `review-v3/adjudication.md`.

## Correção

O detalhe novo deixa de ser substring. Uma linha, depois de removidos os espaços ASCII das duas bordas (`trim_ascii`: 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x20), tem de ser exatamente `t trust · esc back` (U+00B7). A comparação é `eq_ignore_ascii_case`, só A–Z. O scan Rust não aloca nessa checagem: `lines()`, `trim_ascii` e a comparação emprestam os bytes, e ela ocorre antes do `to_lowercase` da janela. O Swift percorre os mesmos bytes UTF-8, com o mesmo corte e a mesma dobra. Citação, sufixo e prefixo não casam. A lista `t trust all · enter review · esc close` continua no par antigo.

## RED

Matcher ainda era o de `eb9a8e`. Os três testes novos esperavam `false`. `rustc --test --edition 2021` em `menu_prompt.rs`, filtro `pause_hook_wait`. Saída em `run-final/red-hook-detail-line/stdout.log`. Exit 101. 14 passed, 3 failed:

- `pause_hook_wait_quoted_footer_in_log_is_not_a_menu`: `Log: "t trust · esc back"` deu `true`.
- `pause_hook_wait_unknown_footer_suffix_is_not_a_menu`: `t trust · esc backwards` deu `true`.
- `pause_hook_wait_unicode_prefix_is_not_a_menu`: `ét trust · esc back` deu `true`.

São os mesmos três contracasos de `scan-v3/negative-results.json`. Não são tela real.

## GREEN

O mesmo comando, depois do patch, em `run-final/green-hook-detail-line/stdout.log`. Exit 0. 17 passed, 0 failed, 11 filtered. Os 14 anteriores continuam passando. A sonda no mesmo `menu_prompt.rs`:

```
quoted_footer_in_log menu=false
unknown_footer_suffix menu=false
unicode_word_prefix menu=false
edge_spaces menu=true
ascii_case menu=true
hooks-review-detail.txt menu=true settled=true
hooks-review-overview.txt menu=true settled=true
paused.txt menu=false settled=true
```

A última linha real é `  t trust · esc back` com espaços no fim. O literal Swift é o mesmo byte string do Rust. O Swift não foi compilado.

## vendored_tree

`tasks.md` 3.1 marcava o campo como atualizado; o report v3 dizia que não foi recalculado. As duas frases descreviam o commit `eb9a8e`: o checkbox prometia a atualização e o hash não mudou.

O campo não é snapshot upstream. `base_revision` `f27e61a6e4fa5e7180f0cd28c129a3b110a89bbc` não resolve no histórico reescrito. `vendored_tree` é o tree git de `third_party/unpeel`. O valor antigo `7c917b141d5180d29e29b0c609e6d734ed8704f9` é o tree desse diretório em `49ada659`. A baseline já estava em `3b32b31b0fa0a60242e251651f2735f63800aeec` e deixou o campo parado. Este commit grava `b39530598881bc743c25aea469e4a8eb671a13d1`, que é `HEAD:third_party/unpeel`. `local_modifications_count` permanece 32 porque a ferramenta que produziu esse número não está no repositório; inventar outro valor quebraria o contrato.

## Arquivos no commit de código

`menu_prompt.rs`, `menu_prompt_pause_hook_wait.rs`, `RemoteGhosttyTerminalView.swift`, `spec.md`, `design.md`, `tasks.md`, `fork_changelog.md`, os dois `AGENTS.md`, `unpeel-upstream.toml`. Os quatro arquivos de graft na raiz ficaram sujos e fora do commit.

## notProven

- `cargo fmt`, `cargo build`, `zeron` / `__workers_mcp__`, smoke e widget.
- Compilação do espelho Swift.
- Host aplicando `menu_prompt_active` nessa tela. A prova é o detector sobre a tela real preservada e sobre os três contracasos.

## 1. Parse do payload atual do Kimi

- [x] 1.1 `usage_row` em `kimi_usage.rs` deriva `used` de `limit - remaining` quando `used` está ausente, clampando negativo em zero e rejeitando a row sem nenhum dos dois
- [x] 1.2 Testes de parser para o shape live (`limit`/`remaining`, string e numérico), `remaining > limit`, e row sem contador

## 2. Linha por provider, sempre

- [x] 2.1 `provider_usage_rows` emite uma linha placeholder `NotSignedIn` (sem `account_id`) para todo provider de `PROVIDERS` sem conta visível
- [x] 2.2 Atualizar os testes de `usage.rs` que assumiam membership vazia (`missing_provider_account_yields_no_placeholder_rows`, `fully_hidden_snapshot_has_no_rows`, contagens de linha)
- [x] 2.3 Remover o empty-state "No accounts in Usage" de `view.rs`, agora inalcançável

## 3. Warning do harness na linha

- [x] 3.1 `ProviderUsageRow` ganha `warning: Option<String>` vindo de `snapshot.warnings` do harness, só quando a linha não tem janela nem linha local
- [x] 3.2 `render_usage_row` usa o warning como resumo no lugar de "No usage yet" / "Not signed in"
- [x] 3.3 Testes para probe falho nomeando a causa e para provider saudável ignorando warning

## 4. DOX e verificação

- [x] 4.1 Atualizar `crates/ui/AGENTS.md` (contratos de Usage e do toggle de Accounts) e `crates/engine/AGENTS.md` (parse do Kimi)
- [x] 4.2 `cargo test -p zeron-ui usage` · `cargo test -p zeron-engine kimi` · `cargo fmt --all`
- [x] 4.3 `openspec validate always-show-usage-providers --strict --no-interactive`
- [ ] 4.4 Validação visual do widget (render gpui não tem harness)

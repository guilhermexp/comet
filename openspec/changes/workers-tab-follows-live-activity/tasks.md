## 1. Atividade viva entra na decisão de foco

- [x] 1.1 `sync_dispatch_with_recency` vira `sync_tab_focus`, recebendo por aba contagem, ids rodando e `started_at` mais novo; estado guarda o conjunto ativo anterior como `Option`, zerado por `sync_context`
- [x] 1.2 Ganho (linha nova, início mais novo, ou linha que passou a rodar) toma foco, na ordem Workflows → Workers → Subagents
- [x] 1.3 Sem ganho, aba efetiva sem nada rodando cede foco para a primeira aba com trabalho vivo

## 2. Call site entrega o que está rodando

- [x] 2.1 `render_chat_workers` passa os ids `Running` de workflows/subagentes e os `semantic.is_active()` dos workers; lista com erro continua entrando como ausência

## 3. Testes

- [x] 3.1 Worker reativado sem crescer contagem nem timestamp puxa foco
- [x] 3.2 Foco migra quando a aba em foco esvazia e outra segue rodando; aba que perde uma linha mas mantém trabalho não cede
- [x] 3.3 Ganho vence esvaziamento no mesmo sync; ordem de desempate preservada
- [x] 3.4 Primeiro sync é baseline mesmo com linhas já rodando; lista ausente e sua recuperação não são atividade
- [x] 3.5 Testes existentes de despacho/recência revisados para a nova API

## 4. DOX e verificação

- [x] 4.1 Atualizar `crates/ui/AGENTS.md` (regra da aba ativa, linha 158, e matriz de cobertura)
- [x] 4.2 `cargo test -p zeron-ui` · `cargo fmt --all`
- [x] 4.3 `openspec validate workers-tab-follows-live-activity --strict --no-interactive`
- [ ] 4.4 Validação visual no app (render gpui não tem harness)

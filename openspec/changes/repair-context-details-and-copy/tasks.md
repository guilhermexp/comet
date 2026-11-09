# Tasks

Escopo: `proposal.md`, `design.md` e delta `specs/details-sidebar/spec.md`; ordem em `../../WORK-QUEUE.md`. Cinco tarefas aceitas no código local; a tarefa 3.1 continua pendente do smoke com produtor compatível. O app instalado não foi alterado.

Worker via `/delegate` e `/implement`, com `graft build <checkout>`, cadeia `AGENTS.md` e TDD por grupo. Partir da linha remota do fork após integrar as changes anteriores da fila; o checkout local `28aa8628` diverge da release `8c5898efc` e não deve ser sobrescrito. Nomes de testes novos abaixo são propostos. Evidência em `.tmp/verify/repair-context-details-and-copy/`.

## 1. Conectar fatos nativos reais

- [x] 1.1 Em `crates/engine/src/context_breakdown.rs` e no seam existente de `ContextUsage`/runtime, disponibilizar o agregado validado para a expansão quando a composição for unsupported, mantendo timestamp, zero/ausente e proveniência de host/runtime/modelo conforme D1. Acrescentar testes `context_breakdown_aggregate_` para Codex/Pi, capacidade parcial, refresh falho, evento após troca de modelo e cache de runtime anterior. Provar RED/GREEN com `cargo test --locked -p zeron-engine --lib context_breakdown` e regressões de `context-usage-continuity`; atualizar `crates/engine/AGENTS.md` no grupo. Reutilizar normalização existente, sem cálculo a partir de billing ou comprimento do transcript.
- [x] 1.2 Em `crates/harness/src/omp/mod.rs` e normalização de contexto, distinguir RPC detalhada suportada de ausência da capability na CLI oficial. Preservar schema-v1 no produtor compatível e agregar os totais read-only disponíveis no caso sem RPC; substituir a orientação enganosa de update por informação factual conforme D2. Provar RED/GREEN com fixture `context_aggregate_without_breakdown_` em `tests/omp_rpc.rs`, cobrindo Unknown command, dois refreshes sem novos prompts, dados detalhados válidos e identidade/configuração alteradas. Comandos: `cargo test --locked -p zeron-harness --test omp_rpc context_` e `cargo test --locked -p zeron-harness --lib context_`; atualizar `crates/harness/AGENTS.md` e matriz de builds. Não instalar produtor nem tornar `scripts/omp-dev` implícito.

## 2. Retirar o rodapé e os estados vazios enganosos

- [x] 2.1 Em `crates/ui/src/details_sidebar/context_breakdown.rs`, derivar os estados de agregado/composição e a visibilidade de barra/categorias/listas. Acrescentar fixtures para o relatório da imagem, agregado conhecido sem composição, vazio total, zero medido, lista vazia medida e lista unsupported; provar RED/GREEN com `cargo test --locked -p zeron-ui context_breakdown`. A expectativa é total/capacidade úteis quando disponíveis, cobertura parcial identificada e ausência de barra/listas sem dado. Atualizar `crates/ui/AGENTS.md` no mesmo grupo.
- [x] 2.2 No render Context de `details_sidebar/{context_breakdown,view}.rs`, remover os quatro parágrafos técnicos marcados na imagem e notas repetitivas; conservar estado curto e ajuda/horário/precisão acionáveis sob interação. Seguir `/Users/guilhermevarela/.agents/skills/impeccable/SKILL.md`, `DESIGN.md` e os tokens/geometria existentes. Provar por fixture GPUI segura que esses textos não aparecem no corpo, que os valores permanecem visíveis, a ajuda é acessível por teclado e a expansão mantém escopo. Registrar captura nas larguras estreita/normal e temas existentes no caminho nativo autorizado, ou `notProven` se esse caminho não estiver disponível; não lançar cópia dev sem isolamento aprovado.

## 3. Prova integrada e revisão

- [ ] 3.1 Conferir fixtures engine→RPC→UI com runtime/host correto, respostas tardias, mudança de modelo, memória redigida, estimates e deferred tools. Executar `cargo test --locked -p zeron-engine --test device_routing context_`, testes focados da harness/UI e `cargo fmt --all -- --check`; registrar revisões e resultados. Conferir contrato read-only contra build OMP compatível explicitamente identificado, separando esse recibo da CLI oficial sem RPC. O smoke não envia prompt, cria turno, conta tokens via API paga ou muda executável/configuração global. Validar `openspec validate repair-context-details-and-copy --strict`; suíte pesada, se necessária, via `box-test run -- cargo test --workspace` conforme `/agents-box`.
- [x] 3.2 Antes do commit final, executar `/Users/guilhermevarela/orchestrator.v5/scripts/review-pair.sh --repo <checkout> --base <sha-base> --out <checkout>/.tmp/verify/repair-context-details-and-copy/review-pair/1 --include-untracked --gate`, com saída nova/vazia. Exit 6: tratar cada achado com evidência. Exit 4: usar contingência imediatamente, por revisor autorizado ou revisão manual completa em `.tmp/verify/repair-context-details-and-copy/manual-review/1/summary.md`, identificando base/árvore/untracked, motivo, arquivos/contratos, achados, tratamento, comandos e limitações. Preservar scanner de segredos e bloquear segredo pendente; nunca `--agent` no OpenQodex nem `--use-credits` no CodeRabbit. Registrar recibo e modalidade real aqui.

## Workflow follow-up

- Após implementação aceita, arquivar via CLI, executar `/sync` e `/cleanup`, preservar dados de usuário/trabalho de outras sessões e limpar recursos próprios dos workers. Se usado, executar `box-test clean` nos checkouts envolvidos.
- Atualizar a fila com o que foi efetivamente entregue. Não declarar composição completa em produção apenas porque o fallback parcial ou a fixture/source-built passaram.
- A entrega do produtor OMP no ambiente instalado e ampliação de APIs de outras CLIs permanecem dependências explícitas. Uma futura distribuição precisa de destino e autorização próprios, sem substituir a CLI silenciosamente; até lá, o painel deve comunicar a cobertura parcial.
- Publicação do app, push e release exigem destino autorizado e prova; este pedido registra o problema, a correção planejada e sua posição na fila.

## Evidência do worker — 2026-11-09

Implementação autorizada sobre `8c5898efc5438be60978d8dfd74468df95469e38`, neste checkout isolado. Recibos locais em `.tmp/verify/comet-blaze-corrections-20261109-codex`. Checkboxes, aceite e arquivamento continuam reservados ao orquestrador; nenhum resultado abaixo declara publicação. Os comandos/exits exatos estão em `verified-ledger.json`.

| Tarefa | Implementação/prova entregue para conferência |
|---|---|
| 1.1 | RED `zeron-engine_--lib_context_breakdown-red.log` → GREEN `verified-03.log` (6): Codex/Pi, zero/capacidade parcial, horário de evento, falha stale, modelo e processo anterior. Continuidade: `verified-04.log` (1), `verified-05.log` (3). |
| 1.2 | RED `zeron-harness_--test_omp_rpc_context_-red.log` → GREEN `verified-01.log`/`verified-15.log` (20 unit): ausência cacheada, dois refreshes read-only sem prompt extra, normalização parcial/zero e schema-v1 preservado. Nenhuma alteração do executável OMP. |
| 2.1 | RED `zeron-ui_context_breakdown-red.log` → GREEN `verified-14.log` (5): agregado conhecido sem composição, vazio total, zero medido, capacidade parcial, listas medidas vazias versus unsupported; barras/listas sem fatos ocultadas. |
| 2.2 | Renderer real GPUI: light/dark a 300/500px, valores/bounds e listas de sete rows preservados, rodapé técnico ausente do corpo, Info com foco/Enter/Space e preferência isolada por contexto. Captura nativa `notProven`; motivo exato em `context-external-limits.json`. |
| 3.1 | `verified-06.log` (5, um smoke externo ignorado) e `verified-07.log` (routing, 1): engine/RPC reais, late model/runtime/host, no-spawn, memória/estimates/deferred. `fmt-final.log` e `context-validate-final.log` (strict exit 0). Produtor fonte identificado c13c5f07126fba7afbc9241da8eed43f9ec5eef4; smoke read-only atual `notProven`: launcher privado removido e source sem dependências. Recibo histórico não é prova desta revisão nem da CLI instalada; limites em `context-external-limits.json`. |
| 3.2 | Gate obrigatório e modalidade efetiva em `review-pair/1/summary.md`; contingência autorizada, se necessária, em `manual-review/1/summary.md`. Sem segredo pendente antes do commit. |

### Revisão e prova complementar

Gate inicial `review-pair/1` exit 10: CodeRabbit protegido pelo scanner; dois valores sintéticos já presentes na base foram conferidos por path/rule/SHA256 (`secret-fixture-verification.json`, exceções exatas em `verified-fixture-secret-allowlist.json`). Oito achados próprios do OpenQodex foram corrigidos; `.cursor/hooks.json` é alteração preexistente, preservada e excluída do commit por instrução explícita. Tratamento por achado em `review-findings-addressed.json`.

Prova após correções: `review-confirmed-ledger.json` (15 comandos exit 0), `admission-final-ledger.json` (admissão imediata, harness completa e background real) e `presets-ledger.json` (quarta correção autorizada). A revisão conjunta final, incluindo presets, fica em `review-pair/2/summary.md`; eventual contingência manual autorizada em `manual-review/1/summary.md`. Nenhum checkbox é aceite do worker.

### Fechamento da revisão conjunta

`review-pair/2` concluiu ambos os revisores e o scanner clean (exit 6 por dez achados). Oito achados próprios foram tratados no escopo; os dois sobre hooks são a mesma alteração preexistente excluída do commit. Tratamento individual em `review2-findings-addressed.json`; regressões RED em `review2-*-red.log` e comandos/exits finais em `review2-final-ledger.json`. O helper nativo existente em `harness/src/executable.rs` foi estendido para provar o mesmo ambiente/cwd da execução, sem outro detector ou dependência. Guards Unix têm inspeção explícita, sem alegar Cargo Windows.

Revisão final dessa correção complementar em `review-pair/3/summary.md`; tratamento/identidade final em `final-review/summary.md`, ou contingência de indisponibilidade em `manual-review/1/summary.md` se exit 4. Os recibos locais distinguem execução de comandos, limitações externas e aceite reservado ao orquestrador.

### Disposição final do gate

`review-pair/3` concluiu OpenQodex e CodeRabbit (cada ferramenta exit 0; gate exit 6 por cinco achados), scanner clean. Os dois achados sobre hooks são alteração preexistente excluída do commit. Os três próprios foram corrigidos e confrontados no delta final: watchdog moderno de inatividade com esperas host/input fora do orçamento; cache com um contexto retido por executável; refresh automático da CLI sem orientação contraditória de reconectar. Não se declara gate exit 0 nem revisão automática do código posterior às correções. Tratamento e identidade final em `final-review/summary.md` e `review3-findings-addressed.json`.

REDs comportamentais: `review3-modern-idle-red.log`, `review3-cache-bound-red.log`, `review3-auto-refresh-red.log` (todos exit 101). Tentativas com erros de montagem/compilação das fixtures estão retidas e explicitamente excluídas da prova RED. GREEN final: `review3-final-ledger.json`, sete comandos exit 0 — OMP 99 (+ dois vivos ignorados), unit filtro omp 89, resolvedor 29, background real 1, Usage 20, freshness/renderer 4, HTTP→snapshot→row 1. Context UI 7 permanece válido em `review2-final-06.log`; presets 2+2+12 em `presets-ledger.json`. Formatação/strict/scanner finais e identidade da árvore são registrados no recibo final antes do commit. Checkboxes/aceite continuam do orquestrador.


### Pendência de revisão da árvore comprometida — 2026-11-09

A árvore original `716b4f4df480cc90ac2d2a37b426557d4f75b7ba` do commit `5a8a68f560382d609a1ab39fe658b859745e0b0b` foi revisada integralmente em snapshot Git limpo e completo contra a base original. `review-pair/4` terminou exit 4: OpenQodex concluiu 352/352 hunks e apontou dois defeitos reais; CodeRabbit não conseguiu inferir a branch-base no clone encadeado. A conferência manual de contingência está em `manual-review/1`, sem declarar aprovação nem substituir uma ferramenta disponível. Corrigidos apenas os refs Git privados necessários para tornar a base inferível; a principal e o hook alheio permanecem intactos.

Os dois defeitos recebem correção mínima e regressões novas: `structured_compaction_validation_failure_before_start_keeps_the_diagnostic` cobre validação antes/depois do ACK, sem start estruturado; `context_breakdown_aggregate_queued_reader_does_not_reuse_superseded_stream_usage` cobre leitor enfileirado com agregado mais recente, partindo de zero real ou ausência. REDs comportamentais e GREENs específicos estão em `review4-new-{red,green}-ledger.json`, usando Cargo locked/offline, jobs=2, target privado separado com debug/incremental desativados; provas anteriores são reaproveitadas. O target antigo já removido não foi reconstruído. A nova revisão nativa da fonte corrigida é registrada em `review-pair/5`; resultado, identidade exata e eventual tratamento ficam nos recibos `.tmp/verify/comet-blaze-corrections-20261109-codex/`. Esta nota não presume o resultado desse gate. Nenhuma task é marcada aceita/fechada; aceite e arquivamento continuam do orquestrador.

## Aceite do orquestrador — 2026-11-09

Conferidos os 51 hashes da entrega `f77bb787d3659b9b6b164d20a73bf9b484050d56` (tree `cf4c86a3a07813ec1bb8f65d889f0de446d2f3be`), os logs focados e os dois GREENs complementares. Revisão liberada por contingência manual após exit 4 real por cota do CodeRabbit, não por gate 0. Recibos sob `.tmp/verify/comet-blaze-corrections-20261109-codex/`: `review-pair/5/summary.md`, `manual-review/2/summary.md` e `review5-delivered-identity.json`. Scanner clean com somente as duas exceções sintéticas exatas já verificadas.

**3.1 permanece pendente:** o smoke read-only atual exige build OMP compatível disponível; o launcher histórico aponta para runtime privado removido e a fonte identificada não tem dependências instaladas. Fixtures e recibo histórico não substituem essa prova. Implementação das demais tarefas aceita; esta change não será arquivada.

Destinos: branch local retida; principal local divergente e hook preexistente preservados. Sem push, release, instalação ou recuperação de conta real.

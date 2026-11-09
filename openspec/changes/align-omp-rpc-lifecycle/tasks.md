# Tasks

Escopo e decisões: `proposal.md`, `design.md` e os três deltas em `specs/` desta change. Implementação autorizada em 2026-11-09; aceite e checkboxes oficiais reservados ao orquestrador. Nenhum destino de release/deploy está autorizado nesta etapa.

Execução por worker via `/delegate`, com `/implement`, `graft build <checkout>` no preflight e leitura dos `AGENTS.md` aplicáveis. Aplicar TDD por grupo: acrescentar fixture e teste, provar falha no código anterior, corrigir e registrar sucesso. Os nomes de testes abaixo são sugestões para os novos casos, não afirmações de testes já existentes. Registrar recibos em `.tmp/verify/align-omp-rpc-lifecycle/`.

## 1. Resultado correlacionado do prompt

- [ ] 1.1 Em `crates/harness/src/omp/process.rs` e `tests/omp_rpc.rs`, expor o ID do request antes do envio sem perder correlação/limites do transporte; acrescentar `prompt_request_identity_precedes_result` e variantes com respostas fora de ordem. Prova: RED/GREEN com `cargo test --locked -p zeron-harness --test omp_rpc prompt_request_`, mais a regressão existente `process_correlates_out_of_order_responses`.
- [ ] 1.2 Em `omp/mod.rs`, integrar admissão, resultado e versão verificada do mesmo executável conforme D1; ampliar `tests/fixtures/fake-omp-rpc.sh` para `prompt_outcome_` (extensão local sem `agent_end`, ACK síncrono, erro com messages vazio, aborted, ID alheio/tardio/duplicado, erro legado seguido de resultado). Prova: RED/GREEN com `cargo test --locked -p zeron-harness --test omp_rpc prompt_outcome_`; atualizar o contrato em `crates/harness/AGENTS.md` no mesmo grupo.

## 2. Yield, assentamento e runtime quente

- [ ] 2.1 Em `omp/{mod,normalize}.rs`, distinguir `yielded:false`, yield do prompt e wake autônomo, conservando conteúdo/usage/timing e fronteiras de inputs/host tools. Acrescentar casos `yield_lifecycle_` com resultado antes/depois do consumo do ACK, wake e `session_settled` sem novo run. Prova: RED/GREEN com `cargo test --locked -p zeron-harness --test omp_rpc yield_lifecycle_` e regressões de interrupção/timing existentes; atualizar o contrato de runtime em `crates/harness/AGENTS.md`.
- [ ] 2.2 Reter `TurnControl` e publicar background antes do `Done`; limpar apenas por estado confirmado ou teardown conforme D2. Estender `crates/engine/tests/{omp_warm_runtime,runtime_survival}.rs` com `omp_background_` para reaper/update bloqueados enquanto há entrega pendente, liberação após assentamento, wake, Stop e shutdown. Prova: RED/GREEN com `cargo test --locked -p zeron-engine --test omp_warm_runtime omp_background_` e `cargo test --locked -p zeron-engine --test runtime_survival omp_background_`; atualizar `crates/engine/AGENTS.md` com o contrato efetivamente implementado, sem novo wire/sync.

## 3. Compactação estruturada

- [ ] 3.1 Em `omp/mod.rs`, substituir decisão por prosa no contrato moderno pelo resultado estruturado da operação ativa; estender fixture e `tests/omp_rpc.rs` com `structured_compaction_` (sucesso sem prosa inglesa, skipped, aborted, falha, ACK antecipado, evento fora de escopo e chegada tardia). Prova: RED/GREEN com `cargo test --locked -p zeron-harness --test omp_rpc structured_compaction_`, confirmando usage antes da conclusão, nenhum token inventado e nenhum `Done` duplicado.
- [ ] 3.2 Provar fallback legado, prazo e cancelamento com fixtures nas versões antiga, desconhecida e piso `18.18.0`, preservando os casos existentes de compactação e o ACK local. Prova: `cargo test --locked -p zeron-harness --test omp_rpc compaction` e novos casos de fronteira de versão; atualizar `crates/harness/AGENTS.md` para registrar a prioridade estruturada e o fallback real.

## 4. Startup de extensões

- [ ] 4.1 Em `omp/{process,mod}.rs`, tornar o consumo de input/eventos ativo durante negociação e configuração de Chat conforme D4, sem enviar prompt antes do setup. Estender fixture com diálogo em `session_start` que bloqueia comandos até resposta e testes `startup_extension_` para resposta, cancelamento, EOF e falha. Prova: RED/GREEN com `cargo test --locked -p zeron-harness --test omp_rpc startup_extension_`; verificar resolução única e ausência de filho/bridge restante, mantendo deadlines de transporte separados da espera de usuário.
- [ ] 4.2 Aplicar política headless apenas à descoberta sem consumidor UI, preservando extensões e overlay de skills; acrescentar `discovery_startup_` para catálogo com extensão interativa e argv de Chat inalterado. Prova: `cargo test --locked -p zeron-harness --test omp_rpc discovery_startup_`, `every_omp_launch_scopes_skills_to_the_project` e regressões de startup burst/overflow/backpressure; atualizar a política em `crates/harness/AGENTS.md` no mesmo grupo.

## 5. Integração e gate da entrega

- [ ] 5.1 Executar `cargo fmt --all -- --check`, `cargo test --locked -p zeron-harness --test omp_rpc`, unit tests OMP da harness e testes engine afetados; conferir dois prompts no mesmo processo, host tools antes do resume, steering, desligamento, timing e `get_context_breakdown` opcional. Registrar comandos/saídas da revisão exata. Não executar teste vivo pago nem lançar app por pressuposição. Se necessária suíte pesada, usar `box-test run -- cargo test --workspace` na box conforme `/agents-box`; não repetir suíte já válida sem causa.
- [ ] 5.2 Validar `openspec validate align-omp-rpc-lifecycle --strict` e revisar o diff completo. Antes do commit final, executar `/Users/guilhermevarela/orchestrator.v5/scripts/review-pair.sh --repo <checkout> --base <sha-base> --out <checkout>/.tmp/verify/align-omp-rpc-lifecycle/review-pair/1 --include-untracked --gate`, com saída nova/vazia. Exit 6 exige corrigir ou refutar cada achado com evidência; exit 4 exige contingência imediata por revisor autorizado ou revisão manual completa em `.tmp/verify/align-omp-rpc-lifecycle/manual-review/1/summary.md`, incluindo identidade da árvore/untracked, indisponibilidade, caminhos, contratos, testes, achados e limitações. Preservar scanner de segredos e bloquear segredo pendente; nunca `--agent` no OpenQodex nem `--use-credits` no CodeRabbit. Registrar aqui o recibo e a modalidade real antes de liberar o commit.

## Workflow follow-up

- Esta proposta permanece aberta para revisão; não arquivar uma change com implementação pendente.
- Após autorização e aceite da implementação, conferir cada tarefa e recibo, arquivar via `/openspec-archive-change`, executar `/sync` e então `/cleanup` nos recursos próprios da tarefa. Preservar learning da auditoria e trabalho de outras sessões.
- Caso a execução use box-test, executar `box-test clean` em todos os checkouts envolvidos. Remover somente builds, temporários, branches e worktrees da tarefa comprovadamente descartáveis; reportar retenções e espaço real liberado.
- Registrar separadamente o destino autorizado e sua prova antes de push, tag ou release; abrir esta proposta não dispara publicação.

## Evidência do worker — 2026-11-09

Implementação autorizada sobre `8c5898efc5438be60978d8dfd74468df95469e38`, neste checkout isolado. Recibos locais em `.tmp/verify/comet-blaze-corrections-20261109-codex`. Checkboxes, aceite e arquivamento continuam reservados ao orquestrador; nenhum resultado abaixo declara publicação. Os comandos/exits exatos estão em `verified-ledger.json`.

| Tarefa | Implementação/prova entregue para conferência |
|---|---|
| 1.1 | `prompt_request_identity_precedes_result`, correlação fora de ordem; RED `omp-identity-red.log` → GREEN `verified-01.log`. |
| 1.2 | Resultados local/síncrono, antes de ACK, erro/aborted, foreign/late/duplicate e piso de versão: RED `prompt_outcome_red.log` → GREEN `verified-01.log` (92 passaram, dois vivos ignorados). ACK síncrono usa output anterior à conclusão, conforme `docs/rpc.md` oficial 18.18.0. |
| 2.1 | Yield falso/verdadeiro, resultado antes/depois ACK, wake e assentamento: RED `yield_lifecycle_red.log` → GREEN `verified-01.log`; normalização/timing em `verified-02.log` (89). |
| 2.2 | Adapter OMP real da fixture integrado à engine: RED `omp-engine-background-red.log` → GREEN `verified-08.log` (background/reaper/wake/assentamento) e `verified-09.log` (update, Stop/shutdown). |
| 3.1 | RED `structured_compaction_red.log` → GREEN `verified-01.log`: commit/skipped/aborted/error, eventos fora do escopo, resultado antes de ACK, usage antes de Done e sem duplicação. |
| 3.2 | Compact legado/estruturado, prazo/cancelamento e versões antiga/desconhecida/piso incluídos em `verified-01.log`. |
| 4.1 | RED `startup_extension_red.log` → GREEN `verified-01.log`: diálogo antes/depois ready, espera humana fora do orçamento, EOF/cancelamento, processo encerrado e retenção total limitada. |
| 4.2 | RED `discovery_startup_red.log` → GREEN `verified-01.log`: discovery `--no-ui`, extensões/skills mantidas, Chat interativo; burst/overflow/backpressure preservados. |
| 5.1 | `verified-ledger.json`: harness completa focada, unit OMP, engine afetada; `fmt-final.log` e testes prévios de timing. Sem teste vivo pago nem app iniciado. |
| 5.2 | Validação strict em `align-validate-final.log` (exit 0). Gate obrigatório e modalidade efetiva em `review-pair/1/summary.md`; contingência autorizada, se necessária, em `manual-review/1/summary.md`. |

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

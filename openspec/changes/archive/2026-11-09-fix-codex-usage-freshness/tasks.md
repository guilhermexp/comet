# Tasks

Referências: `proposal.md`, `design.md`, delta `specs/usage-widget-freshness-and-tone/spec.md` e ordem em `../../../WORK-QUEUE.md`. Implementação local e seis tarefas aceitas pelo orquestrador; limitações externas e recibos estão no aceite ao fim deste arquivo.

Usar worker via `/delegate` e `/implement`; preflight `graft build <checkout>`, cadeia `AGENTS.md` e TDD por grupo. **Base confirmada da versão instalada/remota: `8c5898efc5438be60978d8dfd74468df95469e38`. O checkout principal local `28aa8628` diverge.** Não copiar sua ausência de guard como correção nem apagar proteção de identidade. Revalidar eventual sucessor remoto antes de iniciar e preservar o trabalho local. Nomes de novos testes abaixo são propostos. Recibos em `.tmp/verify/fix-codex-usage-freshness/`.

## 1. Aceitar quota da identidade correta

- [x] 1.1 Na base da release, corrigir `codex_usage` em `crates/engine/src/agent_accounts.rs` (ou seam equivalente após evolução da base), validando o `account_id` retornado contra o workspace autenticado, sem comparar diretamente `slot.account_key` composta. Criar fixtures com chave `user::workspace`, resposta correta e estrangeira, dois usuários no mesmo workspace e identidade ausente/não verificável; preservar reset credits e cache separado. Provar RED/GREEN com novos testes `codex_usage_identity_` via `cargo test --locked -p zeron-engine --lib codex_usage_identity_`, além da regressão do parser de contas; atualizar o contrato dono em `crates/engine/AGENTS.md` no grupo.

## 2. Propagar falha e recuperar coleta por conta

- [x] 2.1 Ajustar apresentação dos resultados em `agent_accounts.rs` para distinguir credencial realmente ausente, resposta de identidade rejeitada e 401 sem esconder erro atrás de quota retida. Usar os campos existentes `usage_fetched_at`/`usage_error`. Provar RED/GREEN com fixtures `codex_usage_recovery_` para sucesso após falha, autenticação rejeitada, backoff, duas contas, RPC global bem-sucedido com uma coleta falha e resultado tardio de credencial anterior. Comando: `cargo test --locked -p zeron-engine --lib codex_usage_recovery_`; reaproveitar `a_failure_keeps_the_last_good_usage_and_backs_off`, `a_rejected_token_reprobes_once_the_credentials_change` e `the_usage_cache_file_round_trips`. Documentar recuperação pelos controles existentes em `crates/engine/AGENTS.md`, sem renovador OAuth paralelo nem troca automática de conta.

## 3. Tornar o saldo histórico reconhecível

- [x] 3.1 Em `crates/ui/src/details_sidebar/usage.rs`, derivar frescor por conta a partir da medição, erro e `now`, incluindo orçamento de cinco minutos, timestamp ausente/futuro, falha com janelas existentes e reset histórico vencido. Alterar a expectativa de `warning_replaces_the_generic_summary_only_on_rows_without_quota` e acrescentar `codex_usage_freshness_`. Provar RED/GREEN com `cargo test --locked -p zeron-ui codex_usage_freshness_` e regressões de `usage`; atualizar o contrato em `crates/ui/AGENTS.md` no mesmo grupo.
- [x] 3.2 Adaptar o render existente do Usage em `details_sidebar/view.rs` para estado textual visível no resumo e detalhes de `Last known`, horário e ação, sem tratar chegada de RPC como nova quota. Seguir `/Users/guilhermevarela/.agents/skills/impeccable/SKILL.md`, tokens e geometria existentes. Acrescentar fixture de render para contas A/B, sucesso recuperado e leitura antiga sem erro; provar texto/estado por fixture GPUI segura e derivação, registrando screenshot na observação visual autorizada ou `notProven` quando o ambiente não permitir. Não lançar cópia dev sem o isolamento exigido pelo projeto. Atualizar docs locais junto à mudança e verificar legibilidade em resumo/expansão, sem depender só de cor.

## 4. Verificação integrada e revisão

- [x] 4.1 Rodar os testes de engine/UI afetados e `cargo fmt --all -- --check`, preservando Managed Provider Usage fora de CRDT e os demais providers. Provar em fixture HTTP→snapshot→row que 5% consumido em 604800s vira Week95% da conta correta e que conta rejeitada permanece histórica. Validar `openspec validate fix-codex-usage-freshness --strict` e registrar os comandos da revisão exata. Suíte pesada, se necessária, vai para `box-test run -- cargo test --workspace` conforme `/agents-box`; checks nativos macOS seguem o projeto. Não repetir consultas reais sem uma decisão que exija nova medição.
- [x] 4.2 Antes do commit final, executar `/Users/guilhermevarela/orchestrator.v5/scripts/review-pair.sh --repo <checkout> --base <sha-base> --out <checkout>/.tmp/verify/fix-codex-usage-freshness/review-pair/1 --include-untracked --gate` em saída nova/vazia. Exit 6: corrigir ou refutar cada achado com evidência. Exit 4: contingência imediata por revisor autorizado ou revisão manual completa em `.tmp/verify/fix-codex-usage-freshness/manual-review/1/summary.md`, com base, identidade da árvore incluindo untracked, indisponibilidade, paths, contratos, achados/tratamento, testes e limitações. Manter scanner de segredos, sem segredo pendente, `--agent` no OpenQodex ou `--use-credits` no CodeRabbit. Registrar aqui recibo e modalidade real para liberar o commit.

## Workflow follow-up

- A execução segue a fila; preparar esta change não publica uma correção nem arquiva trabalho pendente. Atualizar o índice quando o estado mudar.
- Recuperação da conta B por login existente, se necessária, é uma ação separada e deve ter prova de sucesso; não pedir novo login à conta A para compensar o defeito de identidade.
- Após implementação aceita, arquivar via CLI, executar `/sync` e `/cleanup`; preservar learning, perfis, credenciais, cache de usuário e trabalho não integrado. Se usado, executar `box-test clean` em todos os checkouts envolvidos e conferir recursos dos workers.
- Push, tag/release e atualização do app exigem destino autorizado e prova correspondente. Não reescrever a principal local/remota, não publicar a partir do checkout divergente e não apagar dados de conta para simular atualização.

## Evidência do worker — 2026-11-09

Implementação autorizada sobre `8c5898efc5438be60978d8dfd74468df95469e38`, neste checkout isolado. Recibos locais em `.tmp/verify/comet-blaze-corrections-20261109-codex`. Checkboxes, aceite e arquivamento continuam reservados ao orquestrador; nenhum resultado abaixo declara publicação. Os comandos/exits exatos estão em `verified-ledger.json`.

| Tarefa | Implementação/prova entregue para conferência |
|---|---|
| 1.1 | RED `codex-identity-red.log` → GREEN `verified-10.log`: workspace autenticado versus slot composto, foreign/missing identity, dois usuários no mesmo workspace e reset credits só leitura. |
| 2.1 | RED `codex-recovery-red.log` → GREEN `verified-10.log`: 401 com saldo retido, conta independente, backoff, nova credencial, ausência e geração anterior descartada. Regressões `zeron-engine-agent_accounts__probe_tests-regressions.log` (11), `zeron-engine-codex_reset_-regressions.log` (2), `zeron-engine-codex_window_labels-regressions.log` (1). |
| 3.1 | RED comportamental `usage-ui-red-2.log` → GREEN `verified-11.log` (19): erro/idade >5min, ausência/futuro unknown, reset histórico e recuperação só após medição. |
| 3.2 | RED `usage-render-red.log` e `usage-rpc-failure-red.log` → GREEN `verified-12.log` (4): renderer GPUI real em light/dark a 300/500px, Last known/horário/ação e RPC falho sem redatar quota. Captura nativa `notProven`, motivo em `context-external-limits.json`. |
| 4.1 | `verified-10.log` exportou snapshot HTTP sintético; `verified-13.log` leu esse DTO no renderer derivado: Week 95% na conta correta e 96% histórico/sign-in na rejeitada. `usage-http-snapshot.json`, `fmt-final.log`, `usage-validate-final.log` (strict exit 0). Nenhuma conta real foi recuperada por esse teste. |
| 4.2 | Gate obrigatório e modalidade efetiva em `review-pair/1/summary.md`; contingência autorizada, se necessária, em `manual-review/1/summary.md`. Scanner obrigatório; sem uso pago de créditos. |

### Revisão e prova complementar

Gate inicial `review-pair/1` exit 10: CodeRabbit protegido pelo scanner; dois valores sintéticos já presentes na base foram conferidos por path/rule/SHA256 (`secret-fixture-verification.json`, exceções exatas em `verified-fixture-secret-allowlist.json`). Oito achados próprios do OpenQodex foram corrigidos; `.cursor/hooks.json` é alteração preexistente, preservada e excluída do commit por instrução explícita. Tratamento por achado em `review-findings-addressed.json`.

Prova após correções: `review-confirmed-ledger.json` (15 comandos exit 0), `admission-final-ledger.json` (admissão imediata, harness completa e background real) e `presets-ledger.json` (quarta correção autorizada). A revisão conjunta final, incluindo presets, fica em `review-pair/2/summary.md`; eventual contingência manual autorizada em `manual-review/1/summary.md`. Nenhum checkbox é aceite do worker.

### Fechamento da revisão conjunta

`review-pair/2` concluiu ambos os revisores e o scanner clean (exit 6 por dez achados). Oito achados próprios foram tratados no escopo; os dois sobre hooks são a mesma alteração preexistente excluída do commit. Tratamento individual em `review2-findings-addressed.json`; regressões RED em `review2-*-red.log` e comandos/exits finais em `review2-final-ledger.json`. O helper nativo existente em `harness/src/executable.rs` foi estendido para provar o mesmo ambiente/cwd da execução, sem outro detector ou dependência. Guards Unix têm inspeção explícita, sem alegar Cargo Windows.

Revisão final dessa correção complementar em `review-pair/3/summary.md`; tratamento/identidade final em `final-review/summary.md`, ou contingência de indisponibilidade em `manual-review/1/summary.md` se exit 4. Os recibos locais distinguem execução de comandos, limitações externas e aceite reservado ao orquestrador.

### Disposição final do gate

`review-pair/3` concluiu OpenQodex e CodeRabbit (cada ferramenta exit 0; gate exit 6 por cinco achados), scanner clean. Os dois achados sobre hooks são alteração preexistente excluída do commit. Os três próprios foram corrigidos e confrontados no delta final: watchdog moderno de inatividade com esperas host/input fora do orçamento; cache com um contexto retido por executável; refresh automático da CLI sem orientação contraditória de reconectar. Não se declara gate exit 0 nem revisão automática do código posterior às correções. Tratamento e identidade final em `final-review/summary.md` e `review3-findings-addressed.json`.

REDs comportamentais: `review3-modern-idle-red.log`, `review3-cache-bound-red.log`, `review3-auto-refresh-red.log` (todos exit 101). Tentativas com erros de montagem/compilação das fixtures estão retidas e explicitamente excluídas da prova RED. GREEN final: `review3-final-ledger.json`, sete comandos exit 0 — OMP 99 (+ dois vivos ignorados), unit filtro omp 89, resolvedor 29, background real 1, Usage 20, freshness/renderer 4, HTTP→snapshot→row 1. Context UI 7 permanece válido em `review2-final-06.log`; presets 2+2+12 em `presets-ledger.json`. Formatação/strict/scanner finais e identidade da árvore são registrados no recibo final antes do commit. Checkboxes/aceite continuam do orquestrador.

## Aceite do orquestrador — 2026-11-09

Conferidos os 51 hashes da entrega `f77bb787d3659b9b6b164d20a73bf9b484050d56` (tree `cf4c86a3a07813ec1bb8f65d889f0de446d2f3be`), os logs focados e os dois GREENs complementares. Revisão liberada por contingência manual após exit 4 real por cota do CodeRabbit, não por gate 0. Recibos sob `.tmp/verify/comet-blaze-corrections-20261109-codex/`: `review-pair/5/summary.md`, `manual-review/2/summary.md` e `review5-delivered-identity.json`. Scanner clean com somente as duas exceções sintéticas exatas já verificadas.

Todas as tarefas desta change foram aceitas no código local, com as limitações externas registradas acima. O arquivamento sincroniza apenas os contratos; não publica nem instala o app.

Destinos: branch local retida; principal local divergente e hook preexistente preservados. Sem push, release, instalação ou recuperação de conta real.

# Tasks

Quarta correção autorizada explicitamente pelo usuário em 2026-11-09, após as três anteriores. Checkboxes/aceite/arquivo permanecem do orquestrador.

## 1. Usar a projeção autoritativa

- [ ] 1.1 Estender a regressão do controller e a fixture da projeção real de presets/catálogo para installed+enabled, enabled+uninstalled, installed+disabled, ordem/favoritos, comandos customizados e vazio. Provar RED antes do fix com `cargo test --locked -p zeron-workers-unpeel --lib list_presets` e `--test controller_mcp list_presets`, sem depender das CLIs instaladas no host nem chamar a tool viva.
- [ ] 1.2 Trocar o consumidor list_presets para Settings e enabled && installed, preservando wire/ordem/escopo e propagação de erros. Provar GREEN nos comandos anteriores e em `cargo test --locked -p zeron-workers-unpeel --test settings`; atualizar AGENTS dono e changelog local com a diferença entre novo código e app instalado.

## 2. Gate da entrega conjunta

- [ ] 2.1 Validar `openspec validate fix-worker-preset-availability --strict`, fmt --check e incluir este diff no gate review-pair final já obrigatório. Registrar recibos/exits como tarefa adicional separada; nenhum aceite, arquivamento, publicação ou reinício implícito.

## Workflow follow-up

- O orquestrador confere evidência e marca aceite/arquiva; atualização da tool viva exige execução explícita do build novo. Preservar worktree/fontes/recibos até aceite e limpar somente recursos exclusivos regeneráveis.

## Evidência do worker

Raiz: `.tmp/verify/comet-blaze-corrections-20261109-codex/`. Tarefa adicional separada; aceite/checkboxes ainda do orquestrador.

| Tarefa | Prova para conferência |
|---|---|
| 1.1 | RED exit 101 em `zeron-workers-unpeel_--lib_list_presets-red.log`: 11 anunciados versus cinco esperados, com projeção real/catálogo determinístico. RED exit 101 em `zeron-workers-unpeel_--test_controller_mcp_list_presets-red.log`: availability e erro de Settings. |
| 1.2 | GREEN `presets-ledger.json`: unit list_presets (2), dispatcher list_presets (2), settings (12), todos exit 0. Favoritos/ordem, disabled/missing, wire inerte, comandos customizados e vazio inicializado preservados. |
| 2.1 | `presets-validate-final.log`, `fmt-final.log` e revisão conjunta final em `review-pair/2/summary.md`, com eventuais tratamentos/contingência documentados. Tool instalada não foi reiniciada nem testada de novo. |

### Fechamento da revisão conjunta

`review-pair/2` concluiu ambos os revisores e o scanner clean (exit 6 por dez achados). Oito achados próprios foram tratados no escopo; os dois sobre hooks são a mesma alteração preexistente excluída do commit. Tratamento individual em `review2-findings-addressed.json`; regressões RED em `review2-*-red.log` e comandos/exits finais em `review2-final-ledger.json`. O helper nativo existente em `harness/src/executable.rs` foi estendido para provar o mesmo ambiente/cwd da execução, sem outro detector ou dependência. Guards Unix têm inspeção explícita, sem alegar Cargo Windows.

Revisão final dessa correção complementar em `review-pair/3/summary.md`; tratamento/identidade final em `final-review/summary.md`, ou contingência de indisponibilidade em `manual-review/1/summary.md` se exit 4. Os recibos locais distinguem execução de comandos, limitações externas e aceite reservado ao orquestrador.

### Disposição final do gate

`review-pair/3` concluiu OpenQodex e CodeRabbit (cada ferramenta exit 0; gate exit 6 por cinco achados), scanner clean. Os dois achados sobre hooks são alteração preexistente excluída do commit. Os três próprios foram corrigidos e confrontados no delta final: watchdog moderno de inatividade com esperas host/input fora do orçamento; cache com um contexto retido por executável; refresh automático da CLI sem orientação contraditória de reconectar. Não se declara gate exit 0 nem revisão automática do código posterior às correções. Tratamento e identidade final em `final-review/summary.md` e `review3-findings-addressed.json`.

REDs comportamentais: `review3-modern-idle-red.log`, `review3-cache-bound-red.log`, `review3-auto-refresh-red.log` (todos exit 101). Tentativas com erros de montagem/compilação das fixtures estão retidas e explicitamente excluídas da prova RED. GREEN final: `review3-final-ledger.json`, sete comandos exit 0 — OMP 99 (+ dois vivos ignorados), unit filtro omp 89, resolvedor 29, background real 1, Usage 20, freshness/renderer 4, HTTP→snapshot→row 1. Context UI 7 permanece válido em `review2-final-06.log`; presets 2+2+12 em `presets-ledger.json`. Formatação/strict/scanner finais e identidade da árvore são registrados no recibo final antes do commit. Checkboxes/aceite continuam do orquestrador.

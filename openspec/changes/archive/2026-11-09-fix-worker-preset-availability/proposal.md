# Proposal

## Why

`workers(action=list_presets)` anuncia presets enabled cuja CLI aparece em NOT INSTALLED em Settings → Presets. `bootstrap()` não carrega a disponibilidade local que a projeção de Settings já resolve.

## What Changes

- Listar somente presets enabled e installed da projeção autoritativa de Settings, mantendo a ordem relativa e fallback_order contíguo, separado de quick_launch/preferred.
- Preservar IDs/comandos personalizados, escopo e campos wire existentes; vazio inicializado e erros continuam explícitos, sem fallback para builtins ou mutação de perfil real.
- Provar o consumidor e a projeção com fixtures determinísticas de catálogo, incluindo ausente+enabled, installed+disabled, favoritos e vazio.

## Capabilities

### New Capabilities

Nenhuma.

### Modified Capabilities

- `workers-preset-initialization`: disponibilidade do catálogo consumido pelo controller acompanha a projeção de Settings.

## Impact

`crates/workers-unpeel/src/controller_mcp.rs`, testes do controller/Settings e contrato dono. Quarta correção explicitamente autorizada em 2026-11-09, após as três anteriores da fila. Sem alteração de launch, detector, estado/configuração real, UI, publicação ou reinício do app. A prova da fixture/binário novo não comprova mudança da tool viva do app instalado. Aceite/checkboxes/arquivo pertencem ao orquestrador.

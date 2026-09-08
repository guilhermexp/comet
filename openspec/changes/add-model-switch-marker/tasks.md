## 1. Marcador durável

- [x] 1.1 `ChatDocHandle::write_model_switch(from, to)`: entry `system` com uma part de texto, no-op com transcript vazio.
- [x] 1.2 `MutateParams::NoteModelSwitch { chatId, from, to }` no handler de Mutate.
- [x] 1.3 `Pickers::pick_model` dispara o marcador só quando o Chat existe e o modelo efetivo muda (labels resolvidos do catálogo).

## 2. Render

- [x] 2.1 Entry `system` vira `RowKind::Notice` em `rows_for_entry`.
- [x] 2.2 Divisor centralizado: hairline · ícone · texto muted · hairline.
- [x] 2.3 Marcador seguido de outro marcador não renderiza (colapso).

## 3. Fechamento

- [x] 3.1 Testes unit: row de notice, colapso de consecutivos.
- [ ] 3.2 `scripts/dev-demo.sh` para validação visual.
- [x] 3.3 DOX pass: `crates/ui/AGENTS.md`, `crates/engine/AGENTS.md`.

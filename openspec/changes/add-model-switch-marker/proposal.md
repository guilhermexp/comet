## Why

Trocar o modelo dentro de um Chat existente não deixa rastro no transcript. O chip do composer passa a dizer outro nome, mas o histórico não distingue o que rodou com o modelo anterior do que rodou com o novo — quem volta no Chat (ou abre em outro device) não tem como saber onde a troca aconteceu.

## What Changes

- Trocar o modelo de um Chat que já tem transcript escreve um **marcador de troca** no session doc, na posição em que a troca aconteceu.
- O transcript renderiza o marcador como divisor centralizado entre hairlines: `Model changed from <anterior> to <novo>.`
- Trocas consecutivas sem turno entre elas colapsam no render: só o último marcador aparece.
- Chat sem transcript (nenhuma mensagem ainda) não ganha marcador — não há o que dividir.
- O marcador é entry `system` com uma part de texto: cliente antigo o lê como uma linha de texto comum, sem quebrar.

## Capabilities

### New Capabilities

- `chat-model-switch-marker`: marcar no transcript a troca de modelo feita no meio de um Chat.

## Impact

- `crates/engine/src/doc_host.rs`: `ChatDocHandle::write_model_switch`.
- `crates/engine/src/rpc.rs`: `MutateParams::NoteModelSwitch`.
- `crates/ui/src/pickers.rs`: `pick_model` dispara o marcador quando o modelo efetivo muda num Chat existente.
- `crates/ui/src/transcript.rs`: entry `system` vira `RowKind::Notice`; colapso de marcadores consecutivos.
- DOX: `crates/ui/AGENTS.md`, `crates/engine/AGENTS.md`, `crates/doc/AGENTS.md`.
- Sem mudança de shape de container CRDT (entry `system` + part de texto já existem no schema).

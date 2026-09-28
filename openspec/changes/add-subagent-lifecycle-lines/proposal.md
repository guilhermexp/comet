## Why

Hoje um lote de subagentes aparece no chat como uma fileira de chips com um status só ("scout scout scout scout Completed"), presa na posição onde foram disparados. Não dá para ver, lendo o transcript, **quando** cada subagente terminou em relação ao que o orquestrador fez no meio tempo. O Codex desktop mostra o ciclo de vida como eventos na linha do tempo: `<avatar> X começou a trabalhar` onde foi disparado e `<avatar> X terminou` no ponto em que terminou.

O schema não guarda esse ponto: o fim do subagente só reescreve o chip de spawn no lugar (`subagentStatus`), então a ordem de chegada se perde.

## What Changes

- O chip de spawn (part `tool`) ganha um campo aditivo `subagentEnd = { entry?, afterPart? }`, carimbado na transição para `done`/`failed`: a entry do parent ativa naquele momento (ausente = a própria entry do chip) e a última part dela. Reabrir o subagente (steer) limpa o campo.
- O transcript troca a fileira de chips por uma linha por subagente: `<avatar> <nome> começou a trabalhar` na posição do spawn e `<avatar> <nome> terminou` (ou `falhou`) na posição de `subagentEnd`. Clicar na linha abre o subagente como o chip abria.
- Docs sem `subagentEnd` (sessões antigas, clientes antigos): a linha de fim aparece logo depois da linha de início.
- Cliente antigo ignora o campo novo (serde default) e continua desenhando o chip.

## Capabilities

### New Capabilities

- `subagent-lifecycle-lines`: linhas de ciclo de vida dos subagentes na linha do tempo do chat.

## Impact

- `crates/doc/src/parts.rs`: `SubagentEnd`, campo `subagent_end`, carimbo no fold.
- `crates/doc/src/schema.rs`: `update_subagent_chip` aceita o anchor.
- `crates/engine/src/sessions.rs`: passa o anchor (entry ativa + última part) no caminho eager-done.
- `crates/ui/src/transcript.rs`: rows de ciclo de vida, posicionamento pós-projeção.
- DOX: `crates/doc/AGENTS.md`, `crates/ui/AGENTS.md`.

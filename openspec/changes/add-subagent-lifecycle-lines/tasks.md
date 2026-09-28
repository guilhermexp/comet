## 1. Doc
- [x] 1.1 `SubagentEnd` + `MessagePart::Tool.subagent_end` (serde default, camelCase)
- [x] 1.2 Fold carimba o anchor na transição para terminal; reabrir limpa
- [x] 1.3 `update_subagent_chip` grava/limpa `subagentEnd`

## 2. Engine
- [x] 2.1 Caminho eager-done passa entry ativa + última part folded

## 3. UI
- [x] 3.1 Linha "começou a trabalhar" por subagente no lugar da fileira de chips
- [x] 3.2 Linha "terminou"/"falhou" posicionada pelo anchor; fallback logo após o início
- [x] 3.3 Clique abre o subagente

## 4. Closeout
- [x] 4.1 Testes doc/engine/ui
- [x] 4.2 DOX pass

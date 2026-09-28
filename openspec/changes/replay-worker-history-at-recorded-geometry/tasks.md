## 1. Host

- [x] 1.1 Gravar `pty-geometry.jsonl` no launch e no resize, sob o lock do viewport
- [x] 1.2 Teste: round-trip em ordem de stream, linha rasgada ignorada

## 2. Adapter

- [x] 2.1 `read_pty_geometry` e `estimate_legacy_geometry`
- [x] 2.2 Teste: a régua da linha inteira vence a borda de caixa

## 3. UI

- [x] 3.1 Catch-up decodifica nas marcas; o painel só vale depois
- [x] 3.2 Testes: redraw diferencial igual ao grid original, troca no meio de um chunk
- [x] 3.3 Worker parado mantém a largura gravada; scroll horizontal quando o painel é mais estreito
- [x] 3.4 Aba de Worker do chat recebe `stopped`
- [ ] 3.5 Validação visual no dev app com Workers parados

# Workers Shift+Enter Design

## Problema

O encoder de teclado do terminal envia `\r` tanto para `Enter` quanto para
`Shift+Enter`. O OMP, portanto, não consegue distinguir envio de mensagem de
inserção de nova linha.

## Decisão

- manter `Enter` como `\r`;
- codificar `Shift+Enter` como `ESC [ 13 ; 2 ~`, formato legado aceito pelo
  editor do OMP e equivalente ao modificador preservado pelo Ghostty no
  Unpeel;
- aplicar a semântica no encoder compartilhado do terminal, sem condicional
  específica por provider.

## Validação

- teste unitário prova que `Enter` permanece `\r`;
- teste unitário prova que `Shift+Enter` produz `ESC[13;2~`;
- suíte focada do terminal e build do app;
- teste manual no OMP: escrever uma linha, pressionar `Shift+Enter`, continuar
  na linha seguinte e confirmar que nada foi enviado antes do `Enter` simples.

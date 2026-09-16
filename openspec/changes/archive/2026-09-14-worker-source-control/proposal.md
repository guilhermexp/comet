# Changes nos Workers

## Why
O painel já existe nos Workers, mas as RPCs recusam projetos sem Chat/Space.

## What Changes
- Autorizar também raízes de projetos locais registrados dos Workers para Source Control.
- Ler o registro atual a cada operação; não incluir grupos nem projetos removidos.
- Preservar isolamento de paths e o painel compartilhado.

## Impact
Engine RPC, adaptador Workers, testes de autorização.

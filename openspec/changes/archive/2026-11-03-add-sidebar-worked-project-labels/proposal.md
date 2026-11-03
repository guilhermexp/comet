# Proposal

## Why

A lista de Sessions identifica o checkout de origem, mas não os outros projetos em que cada Chat trabalhou. O usuário precisa reconhecer esses projetos sem abrir cada Chat ou confundir várias branches do mesmo projeto com projetos diferentes.

## What Changes

- Mostrar os nomes dos projetos trabalhados na linha de metadados da sidebar, ao lado da branch atual. No exemplo enviado, a linha terá `main` e um label de pasta `comet`.
- Usar a mesma definição de Projects worked do Details, agrupando checkouts/worktrees pela identidade real do projeto raiz. Várias branches do Comet resultam em um único label `comet`, não em uma árvore ou lista de branches.
- Manter os dados separados por Chat, inclusive nas linhas não selecionadas; nenhum Chat herda os projetos do Chat aberto.
- Acrescentar `Projects worked` ao menu existente `… > Show`, com visibilidade persistida, habilitada por padrão e independente de Branch e Details.
- Preservar aparência, altura compacta, navegação, ordenação, ícones e demais preferências atuais. Conter nomes longos na largura disponível; Compact continua sem a linha de metadados, como já faz com Branch.

## Capabilities

### New Capabilities

Nenhuma.

### Modified Capabilities

- `chat-navigation-and-sidebar`: identificação dos projetos trabalhados por Chat na linha de branch e controle de visibilidade no submenu Show.

## Impact

A implementação compartilha a detecção pura de paths entre Details e engine, acrescenta um resumo opcional de IDs de projetos à row sincronizada do Chat e usa WatchChats/WatchSpaces para a sidebar. A engine calcula em background a partir de snapshots retidos, sem abrir todos os docs. Uma solicitação compacta em lote permite preencher resumos ausentes das linhas arquivadas efetivamente exibidas, sem varrer todos os arquivados no startup. A UI acrescenta o label e sua preferência persistida. Não há nova dependência nem novo stream de transcript por linha.

Sem renomear projetos/branches/worktrees, alterar a execução de Sessions, criar aliases, mudar a semântica do painel Details, redesenhar a sidebar, publicar release ou substituir o app instalado. As changes de composer/devices e voz de outras sessões permanecem fora do escopo.

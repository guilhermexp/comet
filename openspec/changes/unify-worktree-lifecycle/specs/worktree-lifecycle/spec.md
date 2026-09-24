## Purpose

Dar ao comet um único ciclo de vida de worktree — onde nasce, como se nomeia, quais raízes são do app,
o setup que sempre roda e o que é "em uso" — igual para Chats e Workers.

## ADDED Requirements

### Requirement: Worktrees novos nascem numa raiz canônica, sem colisão de basename

Todo worktree criado pelo app, para um Chat ou para um Worker, SHALL nascer sob a raiz canônica
`~/.zeron/worktrees` (ou sob `ZERON_WORKTREES_DIR`, quando definido e não vazio), num diretório por
repositório que combina o nome do repositório com um hash do seu caminho canônico. Dois repositórios
com o mesmo basename SHALL NOT compartilhar esse diretório. O nome da pasta e da branch continuam sendo
do chamador: um Chat recebe um nome gerado com branch `zeron/<nome>`; um Worker recebe a branch que o
usuário escolheu.

#### Scenario: Chat e Worker criam na mesma raiz
Test: unit — criação sobre repositório temporário com `ZERON_WORKTREES_DIR` apontando para um tempdir.

- **WHEN** um worktree é criado para um Chat e outro para um Worker do mesmo repositório
- **THEN** os dois ficam sob a raiz canônica, no mesmo diretório de repositório
- **AND** nenhum é criado sob `~/.unpeel/worktrees`

#### Scenario: Repositórios homônimos não colidem
Test: unit — dois repositórios temporários com o mesmo basename.

- **WHEN** dois repositórios em caminhos diferentes, com o mesmo basename, recebem um worktree cada
- **THEN** cada worktree fica num diretório de repositório diferente

#### Scenario: O override de raiz vale para os dois lados
Test: unit — criação pelos dois caminhos com o override definido.

- **WHEN** `ZERON_WORKTREES_DIR` está definido
- **THEN** worktrees de Chat e de Worker são criados sob ele

### Requirement: O app reconhece como seus os worktrees das raízes atual e legadas

Um checkout SHALL contar como gerenciado pelo app quando seu caminho canônico está sob a raiz canônica
ou sob uma raiz legada (`~/.zeron/worktrees` no layout anterior e `~/.unpeel/worktrees`). Nenhum
worktree existente SHALL ser movido, renomeado ou recriado por esta mudança. Um caminho fora dessas
raízes, inclusive o criado por ferramenta externa, SHALL NOT contar como gerenciado, e um link
simbólico SHALL NOT tornar gerenciado um checkout externo.

#### Scenario: Worktree legado continua sendo do app
Test: unit — predicado sobre caminhos nas raízes canônica e legadas.

- **WHEN** um worktree criado antes desta mudança está sob `~/.unpeel/worktrees`
- **THEN** ele conta como gerenciado e segue removível pelas regras de remoção

#### Scenario: Worktree externo não é do app
Test: unit — predicado sobre caminho fora das raízes e sobre symlink para dentro delas.

- **WHEN** um worktree foi criado fora das raízes do app, ou é alcançado por um symlink sob elas
- **THEN** ele não conta como gerenciado

### Requirement: Todo worktree criado pelo app roda o setup do projeto

Depois de criar um worktree, para um Chat ou para um Worker, o app SHALL rodar o setup configurado no
repositório de origem (`.comet/worktree.json` ou `.cursor/worktrees.json`), com o mesmo executor, os
mesmos limites e a mesma variável que aponta o checkout principal. Falha de setup SHALL NOT desfazer o
worktree e SHALL chegar a quem pediu a criação com o comando que falhou e o motivo. Repositório sem
setup SHALL criar o worktree sem executar nada e sem erro.

#### Scenario: Worktree de Chat roda o setup
Test: integration — engine materializa o worktree de um run num repositório temporário com setup que grava um marcador.

- **WHEN** um Chat inicia um run em worktree novo num repositório que tem setup
- **THEN** o setup roda dentro do worktree novo antes do run
- **AND** o marcador existe no worktree

#### Scenario: Setup que falha no Chat não apaga o checkout
Test: integration — setup com comando que sai com erro.

- **WHEN** o setup falha durante a criação do worktree de um Chat
- **THEN** o worktree continua no disco
- **AND** o erro nomeia o comando que falhou

### Requirement: Um checkout está em uso quando um Worker ou um Chat trabalha nele

Um checkout SHALL estar "em uso" enquanto houver um Worker vivo nele **ou** um Chat hospedado neste
device com esse checkout como `cwd` e estado `Working`. A comparação SHALL usar caminhos canônicos. O
estado "em uso" SHALL bloquear a remoção física do checkout por qualquer caminho do app e SHALL bloquear
o Retarget de um Chat para esse checkout quando quem o ocupa é um Worker.

#### Scenario: Chat rodando bloqueia a remoção pelo lado Workers
Test: unit — remoção com um Chat `Working` informado no mesmo caminho canônico.

- **WHEN** a remoção de um worktree gerenciado é pedida enquanto um Chat está `Working` nele
- **THEN** a remoção é recusada com o motivo
- **AND** o worktree continua no disco

#### Scenario: Worker vivo bloqueia a remoção pela engine
Test: integration — `DeleteWorktree` com um Worker vivo registrado no checkout.

- **WHEN** a engine recebe o pedido de remover um worktree onde há um Worker vivo
- **THEN** a remoção é recusada
- **AND** o worktree continua no disco

#### Scenario: Checkout livre é removível
Test: unit — mesmo predicado sem Worker vivo nem Chat `Working`.

- **WHEN** nenhum Worker vive no checkout e nenhum Chat está `Working` nele
- **THEN** o predicado de uso não bloqueia a remoção

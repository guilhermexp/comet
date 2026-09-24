## Purpose

Dar ao comet um único ciclo de vida de worktree — onde nasce, como se nomeia, quais raízes são do app,
o setup e os hooks do worktrunk que rodam, quando a branch sai junto e o que é "em uso" — igual para
Chats e Workers.

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

### Requirement: Os hooks de worktree do worktrunk rodam no app

Quando o repositório tem `.config/wt.toml` com hooks aprovados, o app SHALL rodar `pre-start` depois de
criar um worktree (para Chat ou Worker) e antes de devolvê-lo, e `post-start` em segundo plano em
seguida; e SHALL rodar `pre-remove` antes de remover um checkout gerenciado e `post-remove` em segundo
plano depois. Os três formatos do worktrunk SHALL ser aceitos: string, tabela (comandos concorrentes)
e lista de tabelas (etapas em ordem). As variáveis de template SHALL ser renderizadas e escapadas para
shell. Um hook que use variável ou filtro desconhecido SHALL NOT rodar, e o erro SHALL nomear o token.
`pre-remove` que falha SHALL abortar a remoção com o checkout intacto. `pre-start` que falha SHALL NOT
desfazer o worktree e SHALL chegar a quem pediu a criação. A saída de `post-*` SHALL ficar num log por
checkout. O app SHALL NOT exigir o binário `wt` instalado.

#### Scenario: pre-start e post-start rodam na criação
Test: unit — repositório temporário com `wt.toml` aprovado cujos hooks gravam marcadores com `{{ branch }}`.

- **WHEN** o app cria um worktree nesse repositório
- **THEN** o marcador do `pre-start` existe quando a criação retorna, com o nome da branch
- **AND** o marcador do `post-start` aparece depois, e sua saída está no log do checkout

#### Scenario: Pipeline roda em ordem
Test: unit — `[[pre-start]]` com duas etapas, a segunda lendo o arquivo gravado pela primeira.

- **WHEN** o hook é um pipeline de duas etapas
- **THEN** a segunda etapa só roda depois que a primeira terminou com sucesso

#### Scenario: pre-remove que falha preserva o checkout
Test: unit — `pre-remove = "exit 1"` aprovado.

- **WHEN** a remoção de um checkout gerenciado e limpo é pedida
- **THEN** ela é recusada com o comando que falhou
- **AND** o checkout e a branch continuam no disco

#### Scenario: Variável desconhecida não roda
Test: unit — `pre-start = "echo {{ nao_existe }}"`.

- **WHEN** o hook é renderizado
- **THEN** ele não roda e o erro nomeia `nao_existe`

### Requirement: Hooks vindos do repositório só rodam depois de aprovados

Os comandos de `.config/wt.toml` SHALL rodar apenas quando o usuário aprovou aquele conjunto de
comandos para aquele repositório. Qualquer mudança num comando SHALL exigir nova aprovação. Sem
aprovação, a criação do worktree SHALL seguir sem esses hooks e SHALL informar que estão aguardando
aprovação; a remoção SHALL seguir pulando `pre-remove` e `post-remove`, com aviso. Settings ▸ Projects
SHALL mostrar os comandos lidos e permitir aprovar. O setup de `.comet/worktree.json` SHALL NOT exigir
aprovação.

#### Scenario: Hook não aprovado não roda
Test: unit — `wt.toml` sem registro de aprovação.

- **WHEN** o app cria um worktree
- **THEN** nenhum comando do `wt.toml` roda
- **AND** o resultado diz que os hooks aguardam aprovação

#### Scenario: Comando alterado volta a pedir aprovação
Test: unit — aprovar, alterar um comando do `wt.toml`, criar de novo.

- **WHEN** um comando aprovado muda no arquivo
- **THEN** o conjunto deixa de estar aprovado e não roda até nova aprovação

### Requirement: Caches ignorados são clonados quando o projeto pede

Quando o checkout principal tem `.worktreeinclude`, o app SHALL copiar para cada worktree que cria os
arquivos que são ignorados pelo Git **e** casam com os padrões desse arquivo, a partir do checkout
principal, antes do setup e dos hooks. A cópia SHALL usar reflink quando o sistema de arquivos suporta,
e cópia comum quando não. Arquivos já existentes no destino SHALL NOT ser sobrescritos. Arquivos
rastreados, metadados de controle de versão e worktrees aninhados SHALL NOT ser copiados. Sem
`.worktreeinclude`, nada SHALL ser copiado.

#### Scenario: target/ ignorado e incluído é clonado
Test: unit — principal com `target/x` ignorado, `.env` ignorado, `.worktreeinclude` contendo `target/`.

- **WHEN** o app cria um worktree
- **THEN** `target/x` existe no worktree novo
- **AND** `.env` não existe no worktree novo

#### Scenario: Sem `.worktreeinclude` nada é copiado
Test: unit — mesmo repositório sem o arquivo.

- **WHEN** o app cria um worktree
- **THEN** nenhum arquivo ignorado do principal aparece no worktree novo

### Requirement: A branch sai junto com o checkout só quando já foi integrada

Depois de remover um checkout gerenciado, pelo Chat ou pelos Workers, o app SHALL apagar a branch local
desse checkout somente quando ela não é a branch padrão, não está em outro worktree e está integrada à
branch padrão local (mesmo commit, ancestral, diff sem mudanças, árvores iguais, ou merge que não
acrescenta nada). Uma branch com trabalho não integrado SHALL permanecer. Branches remotas SHALL NOT ser
tocadas. Falha ao apagar a branch SHALL NOT desfazer a remoção do checkout e SHALL virar aviso.

#### Scenario: Branch sem trabalho próprio sai
Test: unit — worktree criado da padrão, sem commits, removido.

- **WHEN** o checkout é removido
- **THEN** a branch local dele não existe mais

#### Scenario: Branch com commit não integrado fica
Test: unit — worktree com um commit que a padrão não tem, removido.

- **WHEN** o checkout é removido
- **THEN** a branch local continua existindo e apontando para o commit

#### Scenario: Branch squash-mergeada por árvore igual sai
Test: unit — padrão recebe o mesmo conteúdo por outro commit.

- **WHEN** o checkout é removido
- **THEN** a branch é reconhecida como integrada e apagada

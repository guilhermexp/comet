## Purpose

Operações de arquivo do explorer de Files — criar, renomear, apagar, mover, copiar e duplicar entradas do checkout — executadas sempre no device dono do checkout, com jaula de path, e refletidas na árvore sem recarregar o painel.

## ADDED Requirements

### Requirement: Criação de arquivo e pasta pelo explorer
O explorer SHALL criar arquivo vazio e diretório dentro do checkout, a partir do menu de contexto de uma row ou dos botões New File / New Folder da toolbar. O nome SHALL ser digitado numa row de input inline na posição da nova entrada. O diretório pai SHALL ser a pasta alvo, o pai do arquivo alvo, ou o root quando não há alvo. Um nome com separador de path SHALL criar os diretórios intermediários. Nome vazio ou cancelado SHALL descartar a operação sem tocar o disco. Após criar arquivo, o explorer SHALL abrir a nova entrada no preview; após criar pasta, SHALL expandi-la.

#### Scenario: Arquivo criado dentro da pasta selecionada
- Test: integration — RPC de criação contra checkout temporário no engine.
- **WHEN** o usuário aciona New File com uma pasta selecionada e confirma o nome `notes.md`
- **THEN** o arquivo `notes.md` passa a existir, vazio, dentro daquela pasta e aparece na árvore

#### Scenario: Nome aninhado cria diretórios intermediários
- Test: integration — criação com nome contendo separador.
- **WHEN** o usuário confirma o nome `docs/adr/0001.md` numa criação de arquivo no root
- **THEN** os diretórios `docs` e `docs/adr` são criados e o arquivo nasce dentro deles

#### Scenario: Criação cancelada não toca o disco
- Test: unit — estado da row de input inline descartado.
- **WHEN** o usuário abre a row de input e a fecha com Escape, ou confirma com nome em branco
- **THEN** nenhuma entrada é criada e a árvore volta ao estado anterior

### Requirement: Rename inline de entrada existente
O explorer SHALL renomear arquivo ou pasta por edição inline do nome na própria row, disparada pelo menu de contexto ou por F2. O root do checkout SHALL ser recusado como alvo. Um nome idêntico ao atual SHALL encerrar sem escrever. Colisão com entrada existente SHALL ser recusada com erro visível e sem sobrescrever o destino.

#### Scenario: Rename de arquivo preserva conteúdo
- Test: integration — rename via RPC com leitura do conteúdo depois.
- **WHEN** o usuário renomeia `a.txt` para `b.txt`
- **THEN** `b.txt` existe com o conteúdo de `a.txt`, `a.txt` deixa de existir e a árvore mostra o nome novo

#### Scenario: Rename para nome já ocupado é recusado
- Test: integration — colisão de rename no engine.
- **WHEN** o usuário renomeia `a.txt` para `b.txt` e `b.txt` já existe
- **THEN** a operação falha com erro visível e nenhum dos dois arquivos é alterado

#### Scenario: Root não é renomeável
- Test: unit — itens do menu desabilitados para a row de root.
- **WHEN** o menu de contexto é aberto sobre o root do checkout
- **THEN** Rename, Delete e Cut aparecem indisponíveis

### Requirement: Delete confirmado e permanente
O explorer SHALL apagar arquivo ou pasta (recursivamente) somente após confirmação explícita do usuário, e a remoção SHALL ser permanente — sem lixeira e sem undo. A tecla Delete sobre a row selecionada SHALL disparar o mesmo fluxo confirmado. A entrada removida SHALL sumir da árvore e, se estava aberta no preview, o preview SHALL soltá-la.

#### Scenario: Pasta apagada some com o conteúdo
- Test: integration — delete recursivo via RPC.
- **WHEN** o usuário confirma Delete sobre uma pasta com filhos
- **THEN** a pasta e todo o conteúdo deixam de existir no disco e somem da árvore

#### Scenario: Delete sem confirmação não apaga
- Test: unit — estado do diálogo de confirmação.
- **WHEN** o usuário aciona Delete e recusa a confirmação
- **THEN** nada é apagado

### Requirement: Mover, copiar e duplicar entradas
O explorer SHALL manter um clipboard interno de uma entrada com modo Cut ou Copy, colável numa pasta alvo. Paste de Cut SHALL mover a entrada; paste de Copy SHALL copiá-la recursivamente. Duplicate SHALL copiar a entrada dentro do próprio pai. Cópia cujo nome já existe no destino SHALL receber nome único derivado do original em vez de sobrescrever. Move cujo destino já existe SHALL ser recusado. Colar uma pasta dentro dela mesma ou de um descendente SHALL ser recusado.

#### Scenario: Cut e paste movem a entrada
- Test: integration — move via RPC entre duas pastas.
- **WHEN** o usuário corta `src/a.rs` e cola em `src/util`
- **THEN** o arquivo passa a existir em `src/util/a.rs` e não existe mais em `src/a.rs`

#### Scenario: Copy para destino ocupado gera nome único
- Test: integration — cópia com colisão de nome.
- **WHEN** o usuário copia `a.txt` e cola numa pasta que já tem `a.txt`
- **THEN** a cópia nasce com nome único derivado de `a.txt` e o arquivo existente permanece intacto

#### Scenario: Pasta não é colada dentro de si mesma
- Test: integration — move/copy com destino descendente da origem.
- **WHEN** o usuário corta a pasta `docs` e tenta colar em `docs/adr`
- **THEN** a operação é recusada e nada muda no disco

### Requirement: Mutação executa no device dono, jaulada no checkout
Toda mutação SHALL ser executada pelo engine dono do checkout, com o mesmo contrato de alvo e a mesma autorização das leituras de Files: requisição de device que não é dono SHALL ser recusada. Path relativo SHALL ser validado antes de tocar o disco — path absoluto, componente `..`, path vazio ou qualquer alcance a `.git` SHALL ser recusado. Uma mutação recusada SHALL retornar erro tipado e não SHALL deixar efeito parcial no disco.

#### Scenario: Path com escape é recusado
- Test: integration — criação/rename com `..` e com path absoluto.
- **WHEN** uma requisição de mutação carrega um path relativo que sai do checkout
- **THEN** o engine recusa com erro de parâmetro e nada é escrito fora do checkout

#### Scenario: Device que não é dono é recusado
- Test: integration — mutação com target de checkout de outro device.
- **WHEN** um device pede mutação num checkout que não lhe pertence
- **THEN** o engine recusa por autorização

#### Scenario: Mutação em checkout de peer atravessa o relay
- Test: integration — relay de mutação entre dois engines.
- **WHEN** o usuário cria um arquivo na árvore de um checkout peer-owned
- **THEN** a criação é executada pelo engine dono e a árvore do device que pediu passa a mostrar a entrada

### Requirement: Árvore reflete a mutação sem recarregar o painel
Após uma mutação bem-sucedida, o explorer SHALL atualizar só os diretórios afetados, preservando expansão, seleção e posição de scroll. Uma mutação feita fora do app no mesmo checkout SHALL chegar pelo mesmo caminho de atualização. Erro de mutação SHALL ser mostrado ao usuário sem derrubar a árvore.

#### Scenario: Expansão sobrevive à criação
- Test: unit — reconciliação do cache de diretório após mutação.
- **WHEN** uma entrada é criada numa pasta expandida enquanto outras pastas estão expandidas
- **THEN** só aquela pasta é relistada e todas as pastas seguem expandidas

### Requirement: Ações de path, terminal e Finder no menu de contexto
O menu de contexto SHALL oferecer Copy Path (absoluto) e Copy Relative Path (relativo ao root do checkout), escrevendo no clipboard do sistema. SHALL oferecer Reveal in Finder e Open in Terminal apontando para a entrada, ou para o diretório pai quando o alvo é arquivo. Reveal in Finder e Open in Terminal SHALL aparecer apenas quando o checkout é local ao device que renderiza; num checkout peer-owned SHALL ficar ausentes.

#### Scenario: Copy Relative Path entrega o path do checkout
- Test: unit — derivação do path relativo a partir da row.
- **WHEN** o usuário aciona Copy Relative Path sobre `src/util/a.rs`
- **THEN** o clipboard do sistema recebe `src/util/a.rs`, sem o prefixo absoluto

#### Scenario: Checkout remoto não oferece Finder nem terminal
- Test: unit — composição do menu por tipo de acesso ao checkout.
- **WHEN** o menu de contexto é aberto numa árvore de checkout peer-owned
- **THEN** Reveal in Finder e Open in Terminal não aparecem, e as demais ações permanecem

## Purpose

Estado de git de um checkout — status por arquivo separado entre índice e worktree, branch, upstream e divergência — e as operações que o alteram: stage, unstage, discard, commit, push, pull e sync, sempre disparadas por ação explícita do usuário.

## ADDED Requirements

### Requirement: Estado de git observável por checkout
O sistema SHALL expor, para um checkout, o branch corrente, o upstream quando existe, os contadores ahead/behind contra esse upstream, e a lista de arquivos mudados com o par de status índice/worktree (staged e unstaged distintos). Arquivo untracked SHALL aparecer como untracked, não como adicionado no índice. Rename SHALL carregar o path antigo. O estado SHALL ser atualizado quando o checkout muda, sem polling da UI. O cálculo de ahead/behind SHALL usar apenas refs locais — nenhuma operação de rede implícita.

#### Scenario: Arquivo com mudança staged e unstaged aparece nas duas seções
- Test: integration — status XY contra checkout temporário.
- **WHEN** um arquivo tem parte das mudanças no índice e parte só no worktree
- **THEN** ele aparece em Staged Changes e em Changes, com a letra correspondente em cada seção

#### Scenario: Untracked é distinguido de added
- Test: integration — porcelain com `??` e com `A `.
- **WHEN** o checkout tem um arquivo novo nunca adicionado e outro já adicionado ao índice
- **THEN** o primeiro é reportado untracked e o segundo staged como adicionado

#### Scenario: Ahead e behind sem tocar a rede
- Test: integration — divergência com upstream local e runner de processo espião.
- **WHEN** o branch está à frente e atrás do upstream
- **THEN** os dois contadores são reportados e nenhum comando de rede é executado

### Requirement: Stage e unstage de arquivo e de seção
O sistema SHALL mover as mudanças de um arquivo para o índice e retirá-las do índice, por arquivo e para todos os arquivos da seção de uma vez. Unstage SHALL preservar as mudanças no worktree. Stage de arquivo deletado SHALL registrar a remoção no índice. Uma operação sobre path fora do checkout SHALL ser recusada.

#### Scenario: Stage move o arquivo de seção
- Test: integration — stage por RPC e releitura do status.
- **WHEN** o usuário aciona stage num arquivo modificado
- **THEN** o arquivo passa a constar em Staged Changes e sai de Changes

#### Scenario: Unstage preserva o conteúdo do worktree
- Test: integration — unstage seguido de leitura do arquivo.
- **WHEN** o usuário retira do índice um arquivo com mudanças staged
- **THEN** o índice volta ao estado do HEAD para aquele arquivo e o conteúdo em disco permanece modificado

#### Scenario: Path fora do checkout é recusado
- Test: integration — stage com path absoluto e com `..`.
- **WHEN** uma operação de índice recebe um path que sai do checkout
- **THEN** é recusada com erro de parâmetro e o índice não muda

### Requirement: Discard confirmado, tracked e untracked
O sistema SHALL descartar as mudanças de worktree de um arquivo tracked restaurando-o, e SHALL remover do disco um arquivo untracked. A UI SHALL exigir confirmação explícita, nomeando o arquivo e dizendo quando a ação apaga o arquivo. Discard SHALL preservar o que já está no índice daquele arquivo. Discard em massa SHALL seguir a mesma regra para cada arquivo da seção.

#### Scenario: Discard de tracked volta ao HEAD e preserva o índice
- Test: integration — discard com mudanças staged e unstaged no mesmo arquivo.
- **WHEN** o usuário descarta as mudanças não staged de um arquivo que também tem mudanças staged
- **THEN** o worktree passa a refletir o índice e as mudanças staged permanecem

#### Scenario: Discard de untracked apaga o arquivo
- Test: integration — discard sobre arquivo nunca adicionado.
- **WHEN** o usuário confirma discard de um arquivo untracked
- **THEN** o arquivo deixa de existir no disco

#### Scenario: Recusar a confirmação não muda nada
- Test: unit — estado do diálogo de confirmação.
- **WHEN** o usuário aciona discard e recusa a confirmação
- **THEN** nenhum comando de git é executado

### Requirement: Commit do que está staged
O sistema SHALL criar um commit com a mensagem digitada, usando exatamente o que está no índice. Mensagem vazia ou só com espaços SHALL ser recusada sem executar commit. Índice vazio SHALL ser recusado com a razão visível, sem commitar mudanças não staged. Após commit bem-sucedido a mensagem SHALL ser limpa e o estado SHALL refletir o índice vazio e o novo ahead.

#### Scenario: Commit consome o índice
- Test: integration — commit por RPC seguido de leitura do status e do log.
- **WHEN** o usuário escreve uma mensagem e aciona Commit com arquivos staged
- **THEN** um commit com aquela mensagem passa a existir, Staged Changes fica vazio e ahead aumenta em um

#### Scenario: Mensagem vazia é recusada
- Test: integration — commit com mensagem em branco.
- **WHEN** o usuário aciona Commit sem mensagem
- **THEN** nenhum commit é criado e o erro é mostrado

#### Scenario: Nada staged é recusado
- Test: integration — commit com índice vazio e worktree sujo.
- **WHEN** o usuário aciona Commit com mudanças apenas não staged
- **THEN** nenhum commit é criado e a razão é mostrada

### Requirement: Sincronização explícita com o remoto
O sistema SHALL oferecer uma ação de sincronizar que traz os commits do upstream em fast-forward e depois envia os locais. Branch sem upstream SHALL oferecer publicar, definindo o upstream no primeiro envio. Fast-forward impossível SHALL abortar sem merge nem rebase automáticos, reportando o conflito de divergência. Falha de rede ou de autenticação SHALL ser reportada com a saída do git, sem deixar a UI em estado de progresso.

#### Scenario: Sync traz e envia
- Test: integration — sync contra remoto local com commit de cada lado (fast-forwardable).
- **WHEN** o usuário aciona Sync com um commit local e um commit remoto encadeável
- **THEN** o branch local passa a conter os dois commits e o remoto recebe o commit local

#### Scenario: Divergência não vira merge automático
- Test: integration — histórias divergentes.
- **WHEN** o upstream divergiu de forma que não permite fast-forward
- **THEN** a operação aborta, nada é enviado e o motivo é reportado

#### Scenario: Branch sem upstream publica
- Test: integration — push inicial definindo upstream.
- **WHEN** o usuário aciona a ação num branch que nunca foi enviado
- **THEN** o branch é criado no remoto com upstream configurado

### Requirement: Superfície de Source Control na Details Sidebar
A Details Sidebar SHALL ter uma aba Changes ao lado de Details e Files, com badge de quantidade de arquivos mudados, que persiste como aba ativa igual às outras. A aba SHALL mostrar branch e ahead/behind no cabeçalho, a caixa de mensagem, os botões Commit e Sync/Publish, e as seções Staged Changes e Changes com a letra de status por arquivo. Cada arquivo SHALL oferecer stage ou unstage e discard; cada seção SHALL oferecer a ação em massa correspondente. Clicar num arquivo SHALL abrir o diff dele no painel Changes existente, no escopo working tree, sem abrir um segundo visualizador. Checkout sem repositório git SHALL mostrar estado vazio explícito, sem oferecer ação.

#### Scenario: Badge acompanha a quantidade de mudanças
- Test: unit — derivação do badge a partir do estado de git.
- **WHEN** o checkout passa a ter três arquivos mudados
- **THEN** a aba Changes mostra três no badge

#### Scenario: Arquivo abre no diff existente
- Test: unit — evento emitido ao clicar na row.
- **WHEN** o usuário clica num arquivo da seção Changes
- **THEN** o painel Changes existente passa a mostrar o diff daquele arquivo no escopo working tree

#### Scenario: Diretório sem git não oferece ação
- Test: unit — composição da aba para checkout sem repositório.
- **WHEN** o contexto aponta para um diretório que não é repositório git
- **THEN** a aba mostra estado vazio e nenhum botão de commit ou sync

### Requirement: Apresentação compacta do painel
O painel SHALL empilhar mensagem, Commit e Sync Changes/Publish em largura total.
A seção Staged Changes SHALL ficar oculta quando vazia. As seções SHALL poder ser
recolhidas e mostrar contagem em badge. Cada arquivo SHALL mostrar ícone de tipo,
basename destacado, diretório secundário truncável e caminho completo em tooltip,
com status colorido à direita (U para untracked, ! para conflito). As ações SHALL
usar ícones com tooltip, revelados no hover da linha, sem abrir o diff ao acioná-los.

#### Scenario: Lista compacta em painel estreito
- Test: none — render GPUI validado visualmente em janela nativa.
- **WHEN** o painel mostra caminhos longos e nenhuma mudança staged
- **THEN** o cabeçalho Staged vazio não ocupa espaço, os botões ocupam a largura e as linhas mantêm ícone, nome e status legíveis com o diretório truncado

#### Scenario: Controles da seção e da linha
- Test: none — interação nativa com seções, hover e confirmação.
- **WHEN** o usuário recolhe uma seção ou aciona um ícone de stage/discard
- **THEN** o disclosure altera a lista e a ação usa o handler existente sem abrir diff, preservando a confirmação de discard

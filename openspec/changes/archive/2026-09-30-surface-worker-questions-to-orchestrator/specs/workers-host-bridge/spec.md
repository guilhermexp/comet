## ADDED Requirements

### Requirement: Worker bloqueado acorda o Orquestrador

O sistema SHALL emitir uma notificação ao chat pai quando um Worker com tarefa
registrada passa a bloqueado esperando input, sem esperar o fim do turno. A
notificação MUST nomear o Worker e MUST ser emitida uma única vez por episódio
de bloqueio, de modo que responder e voltar a bloquear notifica de novo.

#### Scenario: Bloqueio emite notificação ao pai
- Test: unit — motor de notificações com uma sessão que transiciona para
bloqueado.

- **WHEN** um Worker com tarefa registrada passa a bloqueado esperando input
- **THEN** uma notificação de espera por input é emitida para o chat pai
- **AND** a notificação identifica o Worker e o projeto

#### Scenario: Bloqueio contínuo não repete a notificação
- Test: unit — motor de notificações em passadas sucessivas sobre o mesmo estado.

- **WHEN** o Worker permanece bloqueado por várias reconciliações
- **THEN** apenas a primeira passada emite notificação
- **AND** responder e bloquear de novo emite uma nova notificação

### Requirement: A notificação ao pai preserva o conteúdo reportado

O sistema MUST preservar o texto visível da cauda de output ao montar a
notificação ao pai. Uma linha terminada em retorno de carro — o desenho normal
de um terminal que repinta — MUST manter o último conteúdo pintado em vez de
resultar em bloco vazio. A notificação SHALL declarar ausência de conteúdo
apenas quando não houver texto visível a reportar.

#### Scenario: Linha terminada em retorno de carro sobrevive
- Test: unit — montagem do prompt de notificação a partir de cauda com repaint.

- **WHEN** a cauda de output termina em retorno de carro após o último texto
  pintado
- **THEN** o bloco de output da notificação contém esse texto
- **AND** a notificação não declara ausência de conteúdo

#### Scenario: Repaint sucessivo reporta o último estado
- Test: unit — montagem do prompt com múltiplos repaints na mesma linha.

- **WHEN** a mesma linha é repintada várias vezes na cauda de output
- **THEN** o bloco de output contém o último conteúdo pintado
- **AND** não contém as versões anteriores empilhadas

#### Scenario: Ausência real de conteúdo continua sinalizada
- Test: unit — montagem do prompt com cauda sem texto visível.

- **WHEN** a cauda de output não tem nenhum texto visível
- **THEN** a notificação declara ausência de conteúdo

### Requirement: Não lido tem fonte de verdade local

O sistema SHALL derivar o indicador de não lido de uma sessão do estado de
notificação ao pai que o próprio aplicativo mantém, e NOT SHALL depender de um
arquivo de estado que apenas o aplicativo nativo do upstream escreve. Uma
sessão com notificação pendente de reconhecimento MUST reportar não lido.

#### Scenario: Notificação pendente marca a sessão como não lida
- Test: unit — projeção de sessão com notificação registrada e não reconhecida.

- **WHEN** uma sessão tem notificação ao pai emitida e ainda não reconhecida
- **THEN** a sessão reporta não lido para o painel e para o Orquestrador

#### Scenario: Reconhecimento limpa o não lido
- Test: unit — projeção de sessão após reconhecimento da notificação.

- **WHEN** a notificação ao pai daquela sessão é reconhecida
- **THEN** a sessão deixa de reportar não lido

### Requirement: Transcript do runtime da família pi é legível pelo controller

O sistema SHALL expor o transcript de um Worker de runtime da família pi cujo
diretório de sessão é gerenciado, de modo que o Orquestrador possa ler o
conteúdo estruturado da conversa em vez de depender da cauda crua do terminal.

#### Scenario: Runtime declara transcript e resolve adaptador
- Test: unit — resolução de provedor de transcript a partir do catálogo pinado.

- **WHEN** o catálogo é consultado para um runtime da família pi com diretório
  de sessão gerenciado
- **THEN** um adaptador de transcript é resolvido para ele

#### Scenario: Leitura de transcript devolve a conversa
- Test: integration — controller MCP lendo o transcript de uma sessão gerenciada.

- **WHEN** o Orquestrador pede o transcript de uma sessão desse runtime
- **THEN** a resposta contém as entradas da conversa
- **AND** a leitura não é recusada por runtime sem suporte

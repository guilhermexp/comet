# composer-device-mentions Specification

## Purpose
Permitir referências inequívocas aos dispositivos Comet no composer, para que o agente saiba qual máquina o usuário mencionou ao orquestrar trabalho remoto.

## Requirements

### Requirement: Devices no menu de menções

O `@` SHALL oferecer Devices junto das categorias existentes, reutilizando navegação por teclado e clique. A lista SHALL usar o catálogo e o critério de presença de Settings → Devices, mostrando apenas máquinas online, incluindo a local, sem aposentados ou Cloud efêmero. Ela SHALL ser filtrável por nome/ID. Abrir a categoria SHALL NOT inserir texto nem disparar execução.

#### Scenario: Abrir e filtrar dispositivos
Test: unit — fixture GPUI do autocomplete do composer.

- **WHEN** o usuário abre `@`, entra em Devices e digita parte do nome
- **THEN** vê os dispositivos correspondentes com plataforma e presença atuais
- **AND** Escape volta ao nível raiz sem modificar o prompt

#### Scenario: Somente dispositivos ativos
Test: unit — projeção do mesmo catálogo e presença de Settings → Devices.

- **WHEN** há um dispositivo local, dois remotos online e outros remotos offline no catálogo
- **THEN** Devices oferece somente o local e os dois remotos online
- **AND** nenhum offline ou identificador desconhecido é oferecido como destino

### Requirement: Lista alinhada à presença atual

A lista aberta SHALL acompanhar mudanças de presença e nome pela fonte existente de Settings → Devices. A aceitação SHALL revalidar o ID selecionado: se ele deixou de estar online, SHALL NOT inserir aquele item nem outro por troca de índice. Uma menção já inserida no rascunho SHALL conservar nome/ID mesmo após a desconexão.

#### Scenario: Mudança de presença com o menu aberto
Test: unit — fixture GPUI com atualização de snapshot e relógio controlado.

- **WHEN** um remoto desconecta ou expira a presença enquanto Devices está aberto e outro remoto fica online
- **THEN** o primeiro sai da lista e o segundo entra conforme o mesmo critério de Settings
- **AND** a aceitação de uma seleção stale não menciona outro dispositivo por engano

#### Scenario: Desconexão após inserir o chip
Test: unit — fixture do composer e texto durável.

- **WHEN** o usuário já inseriu uma referência e aquele dispositivo depois fica offline
- **THEN** o rascunho conserva a referência e seu ID, sem apagar ou substituir texto
- **AND** o dispositivo offline deixa de aparecer nas novas escolhas da lista

### Requirement: Identidade estável e desambiguada

Selecionar um dispositivo SHALL inserir uma referência que preserve nome de apresentação e ID estável, projetada como chip de dispositivo, não como arquivo. Nomes iguais SHALL ser distinguíveis na lista e na inspeção do chip. A identidade persistida SHALL NOT depender de rename posterior ou da ordem do catálogo.

#### Scenario: Dois dispositivos de mesmo nome
Test: unit — parser/serializer em proto e aceitação no composer.

- **WHEN** dois dispositivos têm o mesmo nome e o usuário seleciona um deles
- **THEN** a referência contém o ID exato do item selecionado
- **AND** atualizar o catálogo ou renomear a máquina não troca o ID daquela referência

### Requirement: Referência preservada até o agente

Rascunho, mensagem enfileirada, edição, reenvio e Chat Transcript SHALL preservar a referência canônica. O prompt inicial e o steer entregues ao agente SHALL identificar nome e ID selecionados de forma legível, utilizável nos argumentos de dispositivo das ferramentas existentes. Exibição compacta SHALL NOT remover a identidade do texto durável.

#### Scenario: Enfileirar e reenviar a referência
Test: unit — round-trip compartilhado e consumidores de edição/envio; integration — caminho existente de prompt do engine quando aplicável.

- **WHEN** a menção passa pela fila, é reeditada e depois enviada ou reenviada
- **THEN** o agente recebe o mesmo ID escolhido e o Chat Transcript conserva a referência original

#### Scenario: Harness inicial e steer
Test: unit — `invocation::harness_prompt` e consumidores compartilhados de harness.

- **WHEN** uma menção é enviada num novo turno ou como steer, inclusive nos caminhos Cursor e OpenCode existentes
- **THEN** ambos os caminhos preservam identificação equivalente do dispositivo

### Requirement: Menção não concede acesso nem altera host

Mencionar um dispositivo SHALL ser referência contextual, não autorização, migração ou execução. Host, cwd, projeto e pickers do Chat atual SHALL permanecer inalterados. O orquestrador SHALL receber informação para usar as ferramentas existentes, sem presumir que um path local existe no destino.

#### Scenario: Trabalho solicitado em outro dispositivo
Test: unit — aceitação da menção não altera configuração do Chat.

- **WHEN** o prompt pede melhorias no projeto X usando um dispositivo remoto mencionado
- **THEN** a menção identifica aquele dispositivo sem mover o Chat atual
- **AND** o projeto e seu caminho no destino continuam sujeitos à descoberta e às permissões existentes

### Requirement: Formato seguro e compatível entre versões

O formato canônico SHALL preservar ID e nome também como texto legível em hosts/clientes antigos sem o novo chip. Parser e projeção SHALL distinguir referências reais de exemplos de código, escapes e links inválidos, sem transformar dados em caminho de arquivo, grant ou instrução executada automaticamente.

#### Scenario: Host sem a nova expansão
Test: unit — texto canônico submetido ao pipeline anterior sem transformação de device.

- **WHEN** um host antigo recebe uma referência de dispositivo
- **THEN** a URI textual ainda contém o ID e o label contém o nome, sem perda de identidade

#### Scenario: Referência literal ou inválida
Test: unit — parser compartilhado de menções de device.

- **WHEN** há um exemplo em código, link escapado ou identificador inválido
- **THEN** o texto permanece literal e não vira chip nem referência expandida ao agente

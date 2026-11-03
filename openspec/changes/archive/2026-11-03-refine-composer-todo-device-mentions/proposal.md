# Proposal

## Why

A bandeja Todo nasce expandida e ocupa o espaço de leitura sem ação do usuário. O menu `@` permite referências a projetos, mas não a dispositivos, obrigando o usuário a descrever manualmente em qual máquina o orquestrador deve realizar o trabalho.

## What Changes

- Todo do composer inicialmente recolhido, inclusive com tarefas pendentes; clique continua expandindo/recolhendo. Progresso não força abertura. Preservar escolha manual por Chat em memória e o recolhimento existente ao concluir e ficar ocioso.
- Entrada Devices no `@`, alinhada ao catálogo e à presença de Settings → Devices, mostrando somente dispositivos online, incluindo o local. Nomes repetidos são desambiguados pelo ID. Atualizar a lista conforme presença/rename; excluir offline, aposentados e Cloud efêmero.
- Chip de dispositivo mantém referência canônica com nome e ID estável. Rascunho, fila e transcript preservam a referência; o prompt real do agente recebe uma identificação legível e utilizável nas ferramentas existentes.
- A menção é contexto de orquestração: não muda o host/cwd do Chat atual, não inicia execução, não concede acesso e não presume o caminho do projeto em outra máquina.
- Reutilizar componentes, estilos, navegação e transporte textual existentes; sem redesenho ou nova preferência global.

## Capabilities

### New Capabilities

- `composer-todo-presentation`: apresentação inicialmente recolhida e expansão manual da bandeja Todo do composer.
- `composer-device-mentions`: seleção e preservação de referências inequívocas a dispositivos no composer e no prompt do agente.

### Modified Capabilities

Nenhuma capability existente precisa ser reescrita. As menções de projetos permanecem textuais e a geometria do Todo existente não muda.

## Impact

UI: `todo_panel.rs`, `composer.rs` e `composer/chip.rs`, com seus testes e DOX. Projeção compartilhada de links e transformação de prompt em `zeron-proto`, integrada ao caminho `invocation::harness_prompt` já consumido por engine/harnesses. Não alterar schemas CRDT/RPC, APIs de criação de Chat/Worker, SSH, sync de projetos, credenciais ou o harness Orchestrator. Coordenar com a mudança Projects worked já em andamento, preservando WIP e targets alheios.

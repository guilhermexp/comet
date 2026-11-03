# Spec Delta

## Purpose

Manter a bandeja Todo do composer compacta até que o usuário escolha abri-la, preservando progresso, acessibilidade e estado visual independente por Chat.

## ADDED Requirements

### Requirement: Todo inicialmente recolhido

A bandeja Todo SHALL iniciar recolhida quando não houver escolha explícita de expansão para aquele Chat, independentemente de haver itens pendentes. Atualizações do agente SHALL NOT abri-la automaticamente. O header SHALL continuar exibindo o resumo atualizado.

#### Scenario: Lista nova recebe progresso sem clique
Test: unit — estado e fixture GPUI em `todo_panel.rs`.

- **WHEN** um Chat apresenta sua primeira lista Todo pendente e depois recebe uma atualização de progresso
- **THEN** a lista permanece recolhida nas duas apresentações
- **AND** o header reflete a nova contagem e tarefa atual

### Requirement: Expansão e recolhimento explícitos

O header SHALL permitir expandir e recolher a lista pelos controles existentes. A escolha explícita SHALL permanecer por Chat em memória durante atualizações normais; a escolha de um Chat SHALL NOT alterar outro.

#### Scenario: Clique seguido de atualização
Test: unit — fixture GPUI com clique real no header.

- **WHEN** o usuário expande a bandeja e o agente atualiza o progresso
- **THEN** o corpo continua aberto com os novos dados
- **AND** recolher pelo header impede abertura automática na atualização seguinte

#### Scenario: Estado separado por Chat
Test: unit — fixture GPUI com alternância de Chat.

- **WHEN** o usuário expande a bandeja do Chat A e visita um Chat B sem escolha anterior
- **THEN** B inicia recolhido
- **AND** retornar a A conserva a escolha manual de A

### Requirement: Lifecycle existente preservado

A conclusão de todos os itens seguida de turno ocioso SHALL recolher a bandeja uma única vez, como já ocorre. Expansão manual posterior SHALL permanecer respeitada. Lista vazia, dismiss por conteúdo, folds de listas longas e acessibilidade SHALL manter seus contratos existentes.

#### Scenario: Conclusão e expansão posterior
Test: unit — estado e fixture GPUI de conclusão.

- **WHEN** a lista conclui, o turno fica ocioso e o usuário depois expande o Todo
- **THEN** a bandeja primeiro recolhe e depois permanece aberta por escolha do usuário

## ADDED Requirements

### Requirement: Changes em projetos Workers
O sistema SHALL aceitar Source Control para a raiz de um projeto Worker registrado no device, mesmo sem Chat ou Space. A autorização SHALL consultar o registro atual, excluir grupos e preservar a recusa de checkout não registrado.

#### Scenario: Worker sem Chat ou Space
- Test: integration — RPC contra registro e checkout temporários.
- **WHEN** um projeto Worker registrado tem mudanças e nenhum Chat/Space
- **THEN** status e stage funcionam no checkout desse projeto

#### Scenario: Remoção revoga autorização
- Test: integration — releitura do registro entre RPCs.
- **WHEN** o projeto é removido do registro Workers
- **THEN** uma nova operação de Source Control é recusada se não houver Chat/Space correspondente

## ADDED Requirements

### Requirement: Continuação agendada não anuncia Stop

Quando a extensão de lifecycle da família pi recebe `agent_end`, o transporte de notificação MUST anunciar `Stop` somente se o evento não marcar continuação já agendada. `willContinue === true` (API pública de extensão, `AgentEndEvent`) MUST NOT anunciar `Stop`. `willContinue === false` e o legado sem a flag MUST anunciar `Stop`. Todo anúncio emitido MUST preservar o metadata de identidade do provider (id da conversa e caminho do transcript) que o consumidor já lê. `agent_start` MUST continuar anunciando `Start`.

#### Scenario: Continuação já agendada não anuncia Stop
- Test: unit — seam Bun/notify.sh em adapter/setup.rs; falha se a continuação anunciar Stop.
- **WHEN** o agente emite `agent_start` e em seguida `agent_end` com `willContinue` estritamente verdadeiro
- **THEN** o transporte de notificação contém `Start` e não contém `Stop`

#### Scenario: Fim terminal explícito ainda anuncia Stop
- Test: unit — seam Bun/notify.sh em adapter/setup.rs; falha se todo Stop for apagado.
- **WHEN** o agente emite `agent_start` e em seguida `agent_end` com `willContinue` estritamente falso
- **THEN** o transporte de notificação contém `Start` seguido de `Stop`
- **AND** o `Stop` carrega o id da conversa do provider e o caminho do transcript

#### Scenario: Fim legado sem flag permanece Stop
- Test: unit — seam Bun/notify.sh em adapter/setup.rs; compatibilidade pi/prime sem flag.
- **WHEN** o agente emite `agent_end` sem a flag `willContinue`
- **THEN** o transporte de notificação anuncia `Stop`
- **AND** o `Stop` carrega o id da conversa do provider e o caminho do transcript

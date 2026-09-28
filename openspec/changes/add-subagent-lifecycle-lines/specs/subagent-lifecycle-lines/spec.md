## Purpose

Mostrar, na linha do tempo do chat, quando cada subagente começou e quando terminou, no ponto em que isso aconteceu.

## ADDED Requirements

### Requirement: O fim do subagente guarda sua posição na linha do tempo

Quando o status de um chip de spawn transiciona para `done` ou `failed`, o chip SHALL receber `subagentEnd` apontando a entry do parent ativa naquele momento (omitida quando é a própria entry do chip) e o id da última part dessa entry, se houver. Quando o segmento ativo ainda não tem parts (sessão estacionada), o anchor SHALL ser a entry mais nova já gravada no doc e sua última part. Um novo `running` (steer/reatribuição) SHALL limpar `subagentEnd`. Status terminal repetido SHALL NOT mover o anchor.

#### Scenario: Subagente termina enquanto o orquestrador continua
Test: unit — `parts.rs`, fold de `Subagent{Done}` após novas parts.

- **WHEN** o orquestrador dispara um subagente, roda dois comandos e então chega o `Done` do subagente
- **THEN** o chip fica `done` com `subagentEnd.afterPart` = id do segundo comando

#### Scenario: Subagente termina depois do turno do chip
Test: unit — `schema.rs`, `update_subagent_chip` com anchor.

- **WHEN** o chip está numa entry já finalizada e o `Done` chega durante uma entry posterior
- **THEN** `subagentEnd.entry` é a entry posterior

### Requirement: Subagentes aparecem como linhas de ciclo de vida

O transcript SHALL desenhar cada subagente de um spawn como linha `<avatar> <nome> começou a trabalhar` na posição do spawn e, quando terminal, uma linha `<avatar> <nome> terminou` (ou `falhou`) após a row que contém a part apontada por `subagentEnd`. Sem anchor resolvível, a linha de fim SHALL aparecer logo depois da linha de início. Clicar em qualquer das linhas SHALL abrir o subagente.

#### Scenario: Linha de fim no ponto certo
Test: unit — `transcript.rs`, rows projetadas com anchor.

- **WHEN** um chip `done` aponta `afterPart` para um comando posterior
- **THEN** a row de fim vem depois da row desse comando e antes da seguinte

#### Scenario: Sessão antiga sem anchor
Test: unit — `transcript.rs`, chip `done` sem `subagentEnd`.

- **WHEN** o chip está `done` e não tem `subagentEnd`
- **THEN** a linha de fim vem logo depois da linha de início

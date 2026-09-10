## ADDED Requirements

### Requirement: Hibernação vem ligada

O sistema SHALL nascer com hibernação ligada, de modo que uma instalação que
nunca abriu a seção Resources ainda assim pare Workers ociosos além do prazo.
Um bloco de settings de recursos persistido SEM o campo de hibernação MUST ser
carregado como ligado; um bloco com o campo explicitamente em `false` MUST ser
respeitado como desligado.

#### Scenario: Instalação que nunca tocou nas settings
Test: `hibernation_is_on_by_default_so_idle_workers_do_not_pile_up`

- **WHEN** a política roda com as settings de recursos default e existe um
  Worker elegível ocioso além do prazo
- **THEN** ele é candidato à hibernação

#### Scenario: Settings gravadas antes de o campo existir
Test: `a_persisted_block_without_the_field_loads_hibernation_on`

- **WHEN** um bloco de settings de recursos sem o campo de hibernação é
  carregado do disco
- **THEN** a hibernação é lida como ligada, e não como o default do tipo

#### Scenario: Usuário desligou a hibernação
Test: `an_explicitly_disabled_block_stays_disabled`

- **WHEN** o bloco carregado traz o campo explicitamente em `false`
- **THEN** a hibernação permanece desligada
- **AND** nenhum Worker é hibernado, por mais ocioso que esteja
  (`hibernation_never_runs_while_explicitly_disabled`)

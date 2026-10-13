## ADDED Requirements

### Requirement: Revisão interativa de hooks bloqueia o worker

O sistema SHALL tratar como menu à espera de input um footer com ação de revisão ou confiança junto com fechar, na mesma linha ou na adjacente. O detalhe aberto por Enter também SHALL bloquear quando uma linha, separada por LF ou CRLF, é exatamente `t trust · esc back` depois de removidos das duas bordas só os bytes 0x09, 0x0A, 0x0C, 0x0D e 0x20, sem distinção de caixa ASCII. U+000B não é removido. Um CR isolado MUST NOT separar a linha. Citação dessa frase no meio de outra linha, sufixo desconhecido, prefixo e o rótulo `Trust` MUST NOT bloquear. Aviso, um detalhe já confiado ou passivo sem essa linha, e prosa com a palavra trust MUST NOT bloquear. Fechar o menu MUST restaurar o estado do latch.

#### Scenario: Footer real de revisão de hooks bloqueia
- Test: unit — `viewport_has_menu_prompt` com o footer `t trust all · enter review · esc close`.

- **WHEN** o viewport mostra o footer interativo de revisão de hooks
- **THEN** o detector de menu é verdadeiro
- **AND** a atividade derivada de um worker ocupado ou ocioso é `blocked`

#### Scenario: Detalhe da revisão com `t trust · esc back` bloqueia
- Test: unit — `viewport_has_menu_prompt` com `SessionStart hooks`, o aviso, `Command /usr/bin/true` e o footer `t trust · esc back`.

- **WHEN** o viewport mostra o detalhe interno da revisão com esse footer
- **THEN** o detector de menu é verdadeiro

#### Scenario: Citação, sufixo e prefixo não são o detalhe
- Test: unit — `viewport_has_menu_prompt` com `Log: "t trust · esc back"`, `t trust · esc backwards` e `ét trust · esc back`.

- **WHEN** a frase aparece citada, com sufixo desconhecido, ou com prefixo
- **THEN** o detector de menu é falso

#### Scenario: Aviso isolado, prosa e lista passiva não bloqueiam
- Test: unit — `viewport_has_menu_prompt` com aviso "1 hook needs review before it can run", prosa e uma lista sem o footer de ação.

- **WHEN** o viewport só mostra o aviso, prosa sobre revisão, ou uma lista passiva de hooks
- **THEN** o detector de menu é falso

#### Scenario: Fechar o menu restaura o estado
- Test: unit — derivação de atividade com o menu ligado e depois desligado sobre o mesmo latch de hook.

- **WHEN** o menu de revisão deixa de estar visível
- **THEN** a atividade volta a ser a do latch de hook, sem permanecer `blocked`

### Requirement: Interrupção observável derruba o spinner sem concluir

Quando a row de status mais nova do viewport é `■ Conversation interrupted`, com somente um sufixo conhecido do CLI (`- use /feedback if something went wrong` ou nenhum), com o composer abaixo dela e sem footer de trabalho depois, o sistema SHALL reportar `idle`. A row mais nova com esse glifo e essa frase encerra a varredura: um sufixo desconhecido nela MUST NOT deixar uma row válida mais antiga contar. O composer é a row de prompt mais de baixo (`›`, `❯` ou `>`). Vazio, placeholder do CLI ou draft ainda não enviado contam como composer. Uma única row `› texto` MUST NOT ser lida como Enter. Uma row de prompt anterior, ainda abaixo da interrupção e seguida do composer atual, é user row já enviada e MUST NOT derrubar o spinner, mesmo antes do footer de trabalho. Essa ociosidade MUST NOT confirmar `Stop` nem casar `completed`. A frase dentro de prosa, blockquote ou saída de ferramenta MUST NOT contar. Um prompt só acima da row, um shell vivo sem composer embaixo, ou um footer de trabalho depois da row MUST NOT derrubar o spinner. Um footer de trabalho novo MUST rearmar `working`.

#### Scenario: Interrupção do turno atual derruba o spinner
- Test: unit — derivação de atividade após `Start`, com a evidência de interrupção do viewport atual.

- **WHEN** o Codex foi interrompido, o CLI continua vivo e o viewport atual mostra a interrupção sem footer de trabalho depois dela
- **THEN** a atividade reportada é `idle`
- **AND** o latch não fica confirmado por `Stop`

#### Scenario: Transcript antigo não derruba o turno novo
- Test: unit — `viewport_shows_settled_interrupt` com a frase antiga seguida de `esc to interrupt`.

- **WHEN** a frase de interrupção permanece no viewport e o footer de trabalho do turno atual vem depois
- **THEN** a evidência de interrupção assentada é falsa
- **AND** a atividade do turno iniciado continua `working`

#### Scenario: Prosa, citação, prompt acima e shell vivo não são a row
- Test: unit — `viewport_shows_settled_interrupt` com a frase em prosa ou blockquote, com o prompt só acima da row, e com um shell vivo sem composer embaixo.

- **WHEN** a frase não é a row `■ Conversation interrupted` com sufixo conhecido seguida do composer
- **THEN** a evidência de interrupção assentada é falsa

#### Scenario: Draft no composer não rearma
- Test: unit — `viewport_shows_settled_interrupt` com `› fix the spinner` como única row de prompt abaixo da interrupção, inclusive com `1 background terminal running` no meio.

- **WHEN** o texto está só no composer e não há user row enviada nem footer de trabalho
- **THEN** a evidência de interrupção assentada permanece verdadeira

#### Scenario: User row enviada e o composer de volta não é pausa
- Test: unit — `viewport_shows_settled_interrupt` com `› fix the spinner` seguido de `› Ask Codex to do anything`, antes de `esc to interrupt`.

- **WHEN** uma row de prompt já enviada fica abaixo da interrupção e o composer atual está de novo visível
- **THEN** a evidência de interrupção assentada é falsa

#### Scenario: Sufixo desconhecido na row mais nova não revive a row antiga
- Test: unit — `viewport_shows_settled_interrupt` com uma row `■ Conversation interrupted` válida acima de uma row mais nova `■ Conversation interrupted - the agent kept going`.

- **WHEN** a row de interrupção mais nova não tem sufixo conhecido
- **THEN** a evidência de interrupção assentada é falsa

### Requirement: Espera por conclusão ou saída solta quando o worker precisa de input

`wait_for_status` de `completed` ou `exited` SHALL retornar no mesmo tick em que a atividade é `blocked`, com `matched` falso, `needs_input` verdadeiro, snapshot do worker e orientação para ler o output e responder. O resultado MUST NOT declarar conclusão nem timeout, e MUST NOT aprovar hooks. Status pedido `blocked` SHALL continuar casando.

#### Scenario: Espera por completed ou exited retorna bloqueado sem casar
- Test: integration — `wait_until_matching` com worker `running`/`blocked` e timeout longo.

- **WHEN** o orquestrador espera `completed` ou `exited` e o worker está `blocked`
- **THEN** a espera retorna antes do timeout
- **AND** o resultado tem `needs_input` e `blocked` verdadeiros, `matched` falso, e não marca conclusão

#### Scenario: Esperar blocked continua casando
- Test: integration — `wait_until_matching` com status pedido `blocked`.

- **WHEN** o orquestrador espera `blocked` e o worker está `blocked`
- **THEN** o resultado tem `matched` verdadeiro

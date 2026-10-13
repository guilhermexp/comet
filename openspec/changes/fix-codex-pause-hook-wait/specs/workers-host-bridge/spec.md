## ADDED Requirements

### Requirement: Revisão interativa de hooks bloqueia o worker

O sistema SHALL tratar como menu à espera de input um footer com ação de revisão ou confiança junto com fechar, na mesma linha ou na adjacente. Aviso, prosa ou lista passiva sem esse par MUST NOT bloquear. Fechar o menu MUST restaurar o estado do latch.

#### Scenario: Footer real de revisão de hooks bloqueia
- Test: unit — `viewport_has_menu_prompt` com o footer `t trust all · enter review · esc close`.

- **WHEN** o viewport mostra o footer interativo de revisão de hooks
- **THEN** o detector de menu é verdadeiro
- **AND** a atividade derivada de um worker ocupado ou ocioso é `blocked`

#### Scenario: Aviso isolado, prosa e lista passiva não bloqueiam
- Test: unit — `viewport_has_menu_prompt` com aviso "1 hook needs review before it can run", prosa e uma lista sem o footer de ação.

- **WHEN** o viewport só mostra o aviso, prosa sobre revisão, ou uma lista passiva de hooks
- **THEN** o detector de menu é falso

#### Scenario: Fechar o menu restaura o estado
- Test: unit — derivação de atividade com o menu ligado e depois desligado sobre o mesmo latch de hook.

- **WHEN** o menu de revisão deixa de estar visível
- **THEN** a atividade volta a ser a do latch de hook, sem permanecer `blocked`

### Requirement: Interrupção observável derruba o spinner sem concluir

Quando o viewport atual mostra a row de status `■ Conversation interrupted`, com somente um sufixo conhecido do CLI (`- use /feedback if something went wrong` ou nenhum), o composer ocioso do CLI abaixo dela e sem footer de trabalho depois, o sistema SHALL reportar `idle`. Composer ocioso é vazio ou um placeholder exato do CLI (`Ask Codex to do anything`, `Ask a follow-up question`). Essa ociosidade MUST NOT confirmar `Stop` nem casar `completed`. A frase dentro de prosa, blockquote ou saída de ferramenta MUST NOT contar. Um sufixo diferente, um prompt só acima da row, um shell vivo sem composer embaixo, texto digitado no composer (mesmo que comece como o placeholder) antes do footer pintar, ou um footer de trabalho depois da row MUST NOT derrubar o spinner. Footer de trabalho e submissão digitada têm precedência sobre o placeholder. Um footer de trabalho novo MUST rearmar `working`.

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
- Test: unit — `viewport_shows_settled_interrupt` com a frase em prosa ou blockquote, com o prompt só acima da row, com um shell vivo sem composer embaixo, e com submissão nova no composer antes do footer.

- **WHEN** a frase não é a row `■ Conversation interrupted` com sufixo conhecido seguida do composer ocioso
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

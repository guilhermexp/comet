## Purpose

Liberar memória e CPU de Workers CLI que já encerraram o turno e ficaram
ociosos, parando-os e arquivando-os automaticamente sem perder a conversa,
segundo as settings de recursos do painel Workers.

## ADDED Requirements

### Requirement: Worker ocioso além do prazo é hibernado

Com hibernação ligada, o sistema SHALL parar e arquivar um Worker cuja
atividade esteja em `idle` há pelo menos `hibernate_after_idle_minutes`,
preservando o diretório de sessão e a conversa, de modo que Restart retome
o Worker de onde parou.

#### Scenario: Worker idle além do prazo
Test: unit — política de hibernação sobre snapshot de sessões.

- **WHEN** hibernação está ligada com prazo de 15 min e um Worker `running`
  está em `idle` há 16 min segundo seu último sinal de atividade
- **THEN** o Worker é parado e marcado como arquivado
- **AND** o Worker continua listado no arquivo do painel com sua conversa
  retomável

#### Scenario: Worker idle dentro do prazo
Test: unit — política de hibernação sobre snapshot de sessões.

- **WHEN** um Worker `running` está em `idle` há 14 min com prazo de 15 min
- **THEN** nada acontece com ele

#### Scenario: Hibernação desligada
Test: unit — política de hibernação com `hibernation_enabled = false`.

- **WHEN** hibernação está desligada
- **THEN** nenhum Worker é hibernado, independentemente de ociosidade ou teto

#### Scenario: Restart de Worker hibernado retoma a conversa
Test: integration — archive seguido de restart (`session_actions`).

- **WHEN** um Worker hibernado é reiniciado pelo painel ou pelo controller MCP
- **THEN** o Worker volta a `running`, deixa de estar arquivado e retoma a
  conversa anterior segundo a capability de resume do seu runtime

### Requirement: Workers protegidos nunca são hibernados

O sistema MUST excluir da hibernação, mesmo além do prazo ou acima do teto:
Workers em `working`; Workers em `blocked`; Workers pinados; a sessão
selecionada no painel; sessões de terminal sem runtime de agente; Workers
com lançamento de runtime pendente; e Workers cujo runtime não expõe a
capability de restart.

#### Scenario: Worker esperando decisão humana
Test: unit — política de hibernação com atividade `blocked`.

- **WHEN** um Worker está em `blocked` há mais tempo que o prazo
- **THEN** ele não é hibernado

#### Scenario: Worker pinado ou selecionado
Test: unit — política de hibernação com `pinned` e sessão selecionada.

- **WHEN** um Worker `idle` além do prazo está pinado, ou é a sessão
  selecionada no painel
- **THEN** ele não é hibernado

#### Scenario: Runtime sem resume
Test: unit — política de hibernação com capability `restart` ausente.

- **WHEN** um Worker `idle` além do prazo roda um runtime sem capability de
  restart
- **THEN** ele não é hibernado, para não descartar uma conversa irrecuperável

### Requirement: Teto de Workers ociosos vivos

Com hibernação ligada, quando o número de Workers `running` em `idle` e
elegíveis exceder `max_live_idle_workers`, o sistema SHALL hibernar os
ociosos há mais tempo, do mais antigo para o mais novo, até o número voltar
ao teto, mesmo que ainda não tenham atingido o prazo.

#### Scenario: Acima do teto
Test: unit — política de hibernação com teto.

- **WHEN** o teto é 12, existem 14 Workers `idle` elegíveis e nenhum passou
  do prazo
- **THEN** os 2 ociosos há mais tempo são hibernados
- **AND** os outros 12 permanecem vivos

### Requirement: Controller MCP retoma Worker hibernado

O controller MCP SHALL expor uma ação de restart de Worker e MUST responder
a `send_text` e `send_keys` dirigidos a um Worker que não está `running`
com um erro que nomeia essa ação, sem escrever no PTY.

#### Scenario: Orquestrador escreve em Worker hibernado
Test: integration — `controller_mcp` com sessão arquivada.

- **WHEN** o Orquestrador chama `send_text` num Worker arquivado
- **THEN** a chamada falha com uma mensagem que instrui a chamar
  `restart_worker` antes
- **AND** nada é escrito no Worker

#### Scenario: Orquestrador reinicia Worker hibernado
Test: integration — `controller_mcp` com `restart_worker`.

- **WHEN** o Orquestrador chama `restart_worker` num Worker arquivado
- **THEN** o Worker volta a `running` com a conversa retomada e a resposta
  informa o id da sessão resultante

### Requirement: Controles de hibernação nas settings

A seção Resources das settings de Workers SHALL expor o toggle de
hibernação, o prazo em minutos e o teto de ociosos vivos, persistindo nos
mesmos campos já gravados pelo snapshot de settings.

#### Scenario: Usuário liga hibernação
Test: manual — `cargo run`, settings de Workers, seção Resources.

- **WHEN** o usuário liga hibernação e ajusta o prazo para 30 min
- **THEN** as settings persistem `hibernation_enabled = true` e
  `hibernate_after_idle_minutes = 30`
- **AND** no próximo ciclo de refresh a política usa esses valores

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

### Requirement: Hibernação exige evidência positiva, nunca ausência de sinal

O sistema MUST hibernar um Worker somente quando duas evidências positivas
existirem para ele: o fim do turno CONFIRMADO pelo hook do próprio runtime, e
a certeza de que relançar retomaria a conversa. Ociosidade inferida de tela
parada e a mera existência de receita de resume no runtime NÃO satisfazem
nenhuma das duas.

#### Scenario: Ociosidade inferida de tela parada
Test: unit — política de hibernação com idle não confirmado por hook.

- **WHEN** um Worker `omp` está há mais tempo que o prazo com a tela parada,
  sem que o runtime tenha reportado fim de turno
- **THEN** ele não é hibernado, porque tela parada é também o que um
  subprocesso longo e silencioso ou uma chamada de provider travada produzem
- **AND** o relógio de ociosidade desse caso aponta para o INÍCIO do turno,
  então o prazo já nasce estourado

#### Scenario: Fim de turno confirmado pelo runtime
Test: unit — máquina de estados de atividade com `Stop` e com sweep.

- **WHEN** o runtime reporta fim de turno por hook
- **THEN** a ociosidade passa a contar como confirmada
- **AND** um novo início de turno revoga essa confirmação
- **AND** um `Stop` desconfiado que é re-armado para `working` por saída
  crescente também perde a confirmação, mesmo que a tela pare depois
- **AND** limpar a atenção pelo menu do app leva a `idle` sem confirmar fim
  de turno; só um `Stop` de hook posterior confirma

#### Scenario: Conversa sem nada para retomar
Test: unit — política de hibernação com conversa não retomável.

- **WHEN** um Worker da família pi ocioso além do prazo não tem id de conversa
  de provider capturado nem diretório de sessão pinado
- **THEN** ele não é hibernado, porque o relançamento seria um agente limpo e
  a conversa desapareceria do ponto de vista do Comet

#### Scenario: Evidência de retomada vem da receita do runtime
Test: unit — sonda de conversa retomável no `activity_bridge`.

- **WHEN** existe id de conversa de provider capturado, ou o comando fixa
  diretório de sessão gerenciado, ou o comando já fixa um id explícito de
  conversa (`codex resume <id>`, `--resume <id>`)
- **THEN** a conversa é considerada retomável, inclusive na forma já
  reescrita que um Worker acordado de hibernação carrega
- **AND** uma receita que só retomaria a conversa mais recente do diretório
  (`codex resume --last`, `gemini --resume latest`) sem nenhuma das três
  evidências não é, porque poderia retomar a conversa de outro Worker
- **AND** uma sessão de terminal nunca é

### Requirement: A decisão de hibernar é reconfirmada antes de executar

O sistema MUST reavaliar a política sobre um snapshot fresco antes de parar
cada Worker, e MUST parar somente os Workers presentes nas duas avaliações.

#### Scenario: Worker recebe trabalho entre a decisão e a execução
Test: unit — segunda passada da política sobre snapshot fresco.

- **WHEN** um Worker era candidato no snapshot do painel e, no snapshot
  fresco, voltou a trabalhar
- **THEN** ele não é parado
- **AND** os demais candidatos seguem sendo parados

#### Scenario: A segunda passada não amplia a primeira
Test: unit — segunda passada da política sobre snapshot fresco.

- **WHEN** o snapshot fresco contém um Worker elegível que não estava na
  primeira decisão
- **THEN** ele não é parado neste ciclo

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

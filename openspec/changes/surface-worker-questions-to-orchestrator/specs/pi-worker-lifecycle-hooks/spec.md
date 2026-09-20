## ADDED Requirements

### Requirement: Worker da família pi reporta atenção quando pede input

O sistema SHALL emitir um evento de lifecycle de atenção quando o agente de um
Worker da família pi abre um prompt interativo que bloqueia o turno esperando
resposta, e SHALL refletir esse estado como Worker bloqueado para o painel e
para o Orquestrador. Prompt de pergunta ao usuário MUST produzir a transição de
atenção: o motor de estado hoje a descarta, sob a premissa de que a pergunta já
aparece no terminal — premissa que vale para um humano olhando o pane e não
para um Orquestrador que só enxerga o estado reportado.

#### Scenario: Prompt interativo emite evento de atenção
- Test: unit — extensão de lifecycle da família pi sob o host de extensão real.

- **WHEN** o agente de um Worker da família pi abre um prompt interativo que
  bloqueia o turno
- **THEN** o transporte de notificação recebe um evento de atenção
- **AND** o evento nomeia a ferramenta que abriu o prompt
- **AND** nenhum evento de fim de turno é emitido, porque o turno não terminou

#### Scenario: Motor de estado aceita atenção de prompt de pergunta
- Test: unit — `ActivityEngine` recebendo o evento de permissão com o nome da
ferramenta de pergunta ao usuário.

- **WHEN** o motor recebe um evento de atenção cuja ferramenta é um prompt de
  pergunta ao usuário
- **THEN** o estado do Worker passa a atenção
- **AND** o evento deixa de ser tratado como apenas ancoragem de posse de hook

#### Scenario: Worker bloqueado é distinguível de Worker trabalhando
- Test: unit — `derive_activity` para a família pi.

- **WHEN** um Worker da família pi está parado num prompt interativo
- **THEN** a atividade reportada é bloqueado, não trabalhando
- **AND** um Worker no meio de um turno continua reportando trabalhando

#### Scenario: Resposta ao prompt retoma o turno
- Test: unit — `ActivityEngine` com atividade de tela após o evento de atenção.

- **WHEN** o prompt é respondido e o agente volta a produzir
- **THEN** a atividade reportada volta a trabalhando
- **AND** o fim do turno continua chegando como evento de conclusão

#### Scenario: Runtime da família pi declara atenção confiável
- Test: unit — capabilities por sessão a partir do catálogo pinado.

- **WHEN** o painel Workers carrega uma sessão de runtime da família pi
- **THEN** o runtime declara que reporta atenção por evento
- **AND** a derivação de atenção não depende de reconhecer o desenho do prompt
  no viewport

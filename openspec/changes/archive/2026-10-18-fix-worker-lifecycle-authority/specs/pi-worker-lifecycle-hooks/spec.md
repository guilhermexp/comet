# Spec Delta

## MODIFIED Requirements

### Requirement: Worker da família pi reporta atenção quando pede input

O sistema SHALL usar sinais suportados pelo runtime para reportar atenção quando um prompt interativo primário bloqueia o Worker esperando resposta. OMP, Pi e Prime MUST ter suas APIs verificadas separadamente; um callback inexistente não constitui integração. Uma pergunta reconhecida MUST bloquear a atividade e sua resposta MUST retomar o trabalho sem emitir conclusão. A ausência de suporte comprovado MUST ser representada honestamente nas capabilities em vez de anunciar atenção confiável.

#### Scenario: Prompt interativo emite evento de atenção
Test: unit — extensão executável e transições de atividade.

- **WHEN** a ferramenta de pergunta primária do OMP espera resposta sem solicitar aprovação de ferramenta
- **THEN** o Worker reporta atenção e a notificação fica disponível ao Chat pai
- **AND** o callback de aprovação não é necessário para detectar essa pergunta

#### Scenario: Resposta ao prompt retoma o turno
Test: unit — extensão executável e motor de atividade.

- **WHEN** uma interação bloqueante termina por resposta ou cancelamento
- **THEN** o Worker deixa atenção e retoma a atividade apropriada
- **AND** o fim da interação não é publicado como fim de turno

#### Scenario: Continuação automática do agente
Test: unit — extensão executável.

- **WHEN** o runtime termina um segmento e informa que continuará o turno
- **THEN** nenhum evento de conclusão primária é publicado
- **AND** uma conclusão efetiva posterior permanece notificável

#### Scenario: Runtime da família pi declara atenção confiável
Test: unit — reporters executáveis e catálogo por runtime.

- **WHEN** um runtime da família pi não possui um sinal de prompt suportado e verificado
- **THEN** a integração não declara atenção confiável para esse runtime
- **AND** a conclusão por evento suportado continua funcionando

#### Scenario: Motor de estado aceita atenção de prompt de pergunta
Test: unit — motor de atividade com atenção nomeando a ferramenta de pergunta.

- **WHEN** um runtime com suporte verificado reporta atenção de uma pergunta primária
- **THEN** o Worker fica bloqueado e o evento não é descartado como mera posse de hook

#### Scenario: Worker bloqueado é distinguível de Worker trabalhando
Test: unit — derivação da atividade do Worker.

- **WHEN** o Worker está num prompt primário suportado que espera resposta
- **THEN** sua atividade é bloqueado
- **AND** um Worker executando um turno sem interação pendente permanece trabalhando

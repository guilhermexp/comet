# pi-worker-lifecycle-hooks Specification

## Purpose
Garantir que um Worker `pi` reporte lifecycle por hook — como `omp`,
`prime-agent` e os demais runtimes hook-owned — e que a instalação dos hooks
gerenciados sobreviva a uma reinstalação limpa do CLI.

## Requirements

### Requirement: Worker `pi` reporta lifecycle por hook

O sistema SHALL lançar um Worker `pi` com a extensão de lifecycle da família pi
carregada, e SHALL declarar para o runtime `pi` as capabilities de lifecycle por
hook e de notificação de conclusão.

#### Scenario: Lançamento carrega a extensão uma vez
Test: unit — comando de startup do adapter `pi`.

- **WHEN** um Worker `pi` é lançado com o comando do preset
- **THEN** o comando efetivo carrega a extensão de lifecycle exatamente uma vez
- **AND** aplicar a preparação de novo sobre o comando já preparado não duplica
  o flag
- **AND** o resto do comando do usuário é preservado

#### Scenario: Início e fim de turno chegam como evento
Test: unit — extensão de lifecycle sob o host de extensão real.

- **WHEN** o agente de um Worker `pi` inicia e termina um turno
- **THEN** o transporte de notificação recebe um evento de início e um de fim
- **AND** cada evento carrega o id da conversa do provider e o caminho do
  transcript

#### Scenario: Atividade do painel vem do hook, não do output
Test: unit — `derive_activity` para a família pi.

- **WHEN** um Worker `pi` recebe o evento de início e depois o de fim
- **THEN** o painel mostra o Worker trabalhando e volta a ocioso imediatamente
  no fim, mesmo com o terminal ainda repintando
- **AND** um runtime sem hooks no catálogo continua derivando atividade do wire

#### Scenario: Conclusão notificável exposta ao painel
Test: unit — capabilities por sessão a partir do catálogo pinado.

- **WHEN** o painel Workers carrega uma sessão `pi`
- **THEN** a sessão expõe notificação de conclusão como disponível, no mesmo
  formato de uma sessão `omp`

### Requirement: Instalação de hooks gerenciados sobrevive a um root apagado

O sistema MUST instalar um asset de hook gerenciado mesmo quando o diretório
raiz dele não existe, e MUST tentar todos os runtimes do catálogo mesmo quando a
instalação de um deles falha.

#### Scenario: Root apagado é recriado
Test: unit — escrita atômica de asset gerenciado.

- **WHEN** um asset gerenciado é instalado e o diretório pai não existe
- **THEN** o diretório é criado e o asset fica no lugar
- **AND** a instalação não falha por diretório ausente

#### Scenario: Falha de um runtime não silencia os outros
Test: unit — `install_runtime_hooks_with` recebe a lista de runtimes e o
instalador como closure; a regressão injeta falha no alias do meio e prova que
todos os aliases foram tentados em ordem e que o erro nomeia o runtime. A
composição install+prune (`combine_migration_outcome`) tem regressão própria
provando que o motivo raiz da instalação não é mascarado pelo erro da poda.

- **WHEN** a instalação de um runtime falha
- **THEN** os runtimes restantes ainda são instalados
- **AND** o erro relatado nomeia o runtime que falhou

#### Scenario: Hook de outra ferramenta sob root temporário não bloqueia
Test: integration — verificação do root de hooks legado (`hook_migration`).

- **WHEN** um config de provider contém um hook gerenciado válido e, em outro
  comando, um hook de outra ferramenta sob um diretório temporário
- **THEN** a migração do root legado prossegue
- **AND** um hook gerenciado nosso deixado sob um root temporário continua
  bloqueando a migração

### Requirement: Worker da família pi reporta atenção quando pede input

O sistema SHALL usar sinais suportados pelo runtime para reportar atenção quando um prompt interativo primário bloqueia o Worker esperando resposta. OMP, Pi e Prime MUST ter suas APIs verificadas separadamente; um callback inexistente não constitui integração. Uma pergunta reconhecida MUST bloquear a atividade e sua resposta MUST retomar o trabalho sem emitir conclusão. A ausência de suporte comprovado MUST ser representada honestamente nas capabilities em vez de anunciar atenção confiável.

#### Scenario: Prompt interativo emite evento de atenção
Test: unit — extensão executável e transições de atividade.

- **WHEN** a ferramenta de pergunta primária do OMP espera resposta sem solicitar aprovação de ferramenta
- **THEN** o Worker reporta atenção e a notificação fica disponível ao Chat pai
- **AND** o callback de aprovação não é necessário para detectar essa pergunta

#### Scenario: Motor de estado aceita atenção de prompt de pergunta
Test: unit — motor de atividade com atenção nomeando a ferramenta de pergunta.

- **WHEN** um runtime com suporte verificado reporta atenção de uma pergunta primária
- **THEN** o Worker fica bloqueado e o evento não é descartado como mera posse de hook

#### Scenario: Worker bloqueado é distinguível de Worker trabalhando
Test: unit — derivação da atividade do Worker.

- **WHEN** o Worker está num prompt primário suportado que espera resposta
- **THEN** sua atividade é bloqueado
- **AND** um Worker executando um turno sem interação pendente permanece trabalhando

#### Scenario: Resposta ao prompt retoma o turno
Test: unit — extensão executável e motor de atividade.

- **WHEN** uma interação bloqueante termina por resposta ou cancelamento
- **THEN** o Worker deixa atenção e retoma a atividade apropriada
- **AND** o fim da interação não é publicado como fim de turno

#### Scenario: Runtime da família pi declara atenção confiável
Test: unit — reporters executáveis e catálogo por runtime.

- **WHEN** um runtime da família pi não possui um sinal de prompt suportado e verificado
- **THEN** a integração não declara atenção confiável para esse runtime
- **AND** a conclusão por evento suportado continua funcionando

#### Scenario: Continuação automática do agente
Test: unit — extensão executável.

- **WHEN** o runtime termina um segmento e informa que continuará o turno
- **THEN** nenhum evento de conclusão primária é publicado
- **AND** uma conclusão efetiva posterior permanece notificável

### Requirement: Primary Worker lifecycle excludes nested OMP subagents

The system SHALL exclude identifiable nested OMP subagent hooks from primary Worker lifecycle. Explicit primary/subagent identity MUST take precedence over transcript layout. A declared subagent or an otherwise unidentified nested conversation beneath a primary transcript's artifacts directory MUST NOT affect primary lifecycle. Hooks without provider identity or transcript metadata MUST retain legacy handling.

#### Scenario: Nested subagent completes while the primary is working
Test: unit + integration — pi-family lifecycle extension and Worker hook ingress isolation.

- **WHEN** an OMP Worker is working and a nested subagent finishes its turn
- **THEN** the nested completion does not mark the primary Worker idle or complete
- **AND** the primary conversation's completion still transitions the Worker normally

#### Scenario: Nested subagent asks for input
Test: unit + integration — pi-family lifecycle extension and Worker hook ingress isolation.

- **WHEN** a nested OMP subagent reports an interactive prompt or prompt response
- **THEN** that event does not change the primary Worker's attention state
- **AND** primary conversation prompts continue to report attention and resume

#### Scenario: Legacy hooks lack provider identity
Test: integration — backward-compatible Worker hook ingress.

- **WHEN** a legacy hook reports lifecycle without provider identity or transcript metadata
- **THEN** existing primary Worker lifecycle handling remains available
- **AND** no provider model or tokens are invented

#### Scenario: Provider declares a primary conversation at a former child path
Test: unit + integration — explicit-role lifecycle emission and HTTP ingress.

- **WHEN** the provider declares the callback as primary while its transcript occupies a former subagent artifacts path
- **THEN** the primary lifecycle event reaches the Worker normally
- **AND** the explicit primary identity takes precedence over path-based nesting detection

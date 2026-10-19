# Spec Delta

## ADDED Requirements

### Requirement: Workers da box compartilham a lista com origem explícita
O widget de Workers do Chat e a lista global de Workers SHALL apresentar as sessões remotas acompanhadas junto às locais, com tag textual `box`. O widget do Chat MUST filtrar pelo vínculo de origem. Identidades distintas MUST preservar seleção, ordenação e ações mesmo quando os nomes coincidem.

#### Scenario: Sessões locais e remotas coexistem
Test: unit — projeção e seleção por identidade; none — prova visual nativa separada.
- **WHEN** um Chat tem um worker local e um worker remoto com o mesmo título
- **THEN** ambos aparecem como itens distintos e somente o remoto recebe a tag `box`
- **AND** o título, a idade de lançamento e os dados disponíveis seguem as convenções atuais
- **AND** ausência de tokens remotos não produz números inventados

#### Scenario: Outro Chat tem uma sessão da box
Test: unit — filtro por vínculo de origem.
- **WHEN** a lista global contém uma sessão vinculada a outro Chat
- **THEN** ela não aparece no widget do Chat selecionado
- **AND** a lista global conserva a identificação da box e do Chat de origem

### Requirement: Indicador remoto representa evidência do turno
Uma row de worker da box SHALL mostrar spinner para trabalho em curso, indicação distinta para espera de resposta e sinal de término para um turno confirmado. Estado ocioso, processo encerrado sem término confirmado, falha, cancelamento, desativação e desconexão MUST permanecer distinguíveis e MUST NOT receber um check de sucesso por fallback local.

#### Scenario: Trabalhador aguarda resposta e depois termina
Test: unit — transições de apresentação; none — prova visual nativa separada.
- **WHEN** uma sessão passa de trabalhando a aguardando resposta e depois encerra o turno
- **THEN** a row muda de spinner para espera e então para término de turno
- **AND** o texto acessível distingue esses estados sem depender apenas de cor ou animação

#### Scenario: Box desconecta com worker ativo
Test: unit — projeção de conexão separada de atividade.
- **WHEN** a box desconecta enquanto o último estado conhecido era trabalhando
- **THEN** a row mostra desconexão e estado desatualizado em vez de um spinner que afirme atividade atual
- **AND** não anuncia conclusão nem perde a identidade da sessão

### Requirement: Clique em worker remoto abre seu backend
Clicar em um worker da box SHALL abrir seu terminal dentro do Comet usando a identidade remota selecionada. O aplicativo MUST NOT procurar PID, viewport ou diretório de sessão Unpeel para esse item. Ações exclusivas do backend local MUST ficar indisponíveis para rows remotas.

#### Scenario: Abrir worker remoto sem arquivo local
Test: integration — despacho de abertura; none — prova visual nativa separada.
- **WHEN** o usuário clica em um worker da box sem manifesto ou output local
- **THEN** o painel interno abre a sessão remota correta
- **AND** o worker local homônimo não é selecionado ou iniciado

#### Scenario: Integração desativada
Test: unit — disponibilidade de ação e erro de abertura.
- **WHEN** o usuário tenta abrir uma sessão histórica com a integração desativada
- **THEN** o Comet informa que é preciso reativar a conexão
- **AND** mantém a row histórica sem iniciar um worker local substituto

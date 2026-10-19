# Spec Delta

## Purpose

Integrar sessões de agentes executadas em uma box Shipyard ao Comet, com ferramentas, informações da máquina cloud, acompanhamento confiável, notificações ao Chat de origem e terminal remoto, preservando a execução na box.

## ADDED Requirements

### Requirement: Integração opcional por device host
O Comet SHALL oferecer a integração Box / Shipyard desligada por padrão, com configuração local ao device host do Chat. A ativação MUST verificar acesso autenticado e capacidades necessárias antes de oferecer operações. Configurações ausentes ou incompatíveis MUST produzir erro acionável sem alterar a box.

#### Scenario: Ativação válida
Test: integration — engine com transporte controlado; smoke real de leitura autenticada.
- **WHEN** o usuário ativa uma conexão SSH existente para uma box compatível
- **THEN** a integração informa conectada e disponibiliza operações ao Chat autorizado
- **AND** nenhuma porta pública, instalação ou alteração de sessão é realizada pela ativação

#### Scenario: Acesso ou capacidade indisponível
Test: integration — falha de autenticação e ausência de journal/turnos/MCP.
- **WHEN** SSH, credencial ou uma capacidade necessária está indisponível
- **THEN** a integração informa qual pré-requisito falta
- **AND** não oferece operações como se a conexão estivesse pronta nem muda a confiança SSH automaticamente

### Requirement: Credenciais fora das ferramentas e da sincronização
A integração MUST manter credenciais de transporte fora de prompts, schemas, resultados de ferramentas, logs, URLs registradas e estado sincronizado. O canal para a API privada MUST ser autenticado e criptografado; o agente MUST NOT receber o token da gateway.

#### Scenario: Token inválido
Test: integration — resposta 401 e inspeção de diagnósticos capturados.
- **WHEN** a gateway recusa a credencial
- **THEN** o Comet informa falha de autenticação sem repetir a credencial
- **AND** não recorre a um endpoint público ou a uma conexão sem autenticação

### Requirement: MCP da box disponível com contexto do Chat
Quando habilitada, a integração SHALL oferecer o catálogo do MCP nativo Shipyard aos harnesses com bridge MCP do Comet, incluindo OMP. As chamadas SHALL executar na box selecionada com contexto de origem fornecido pelo host. Desabilitada, nenhuma operação remota MUST ser aceita, inclusive por um catálogo já carregado.

#### Scenario: Ferramenta executa na box correta
Test: integration — chamada MCP completa pelos harnesses suportados; smoke real de berth_sessions.
- **WHEN** um Chat autorizado chama uma ferramenta de listagem da box
- **THEN** recebe o resultado daquela box, sem confundi-lo com workers ou caminhos locais
- **AND** nenhum arquivo de configuração global do provider é modificado para disponibilizar a ferramenta

#### Scenario: Desativação durante um Chat
Test: integration — catálogo pré-carregado seguido de desativação.
- **WHEN** a integração é desativada e o agente tenta usar uma ferramenta que já conhecia
- **THEN** a chamada é recusada como integração desativada antes de executar na box
- **AND** a interface informa quando a atualização do catálogo exige o próximo turno

### Requirement: Sessões iniciadas têm vínculo de origem durável
Sessões criadas pelas operações de delegação da integração SHALL ser vinculadas ao Chat chamador antes de serem apresentadas como workers acompanhados. O vínculo MUST usar identidade da conexão, sessão remota e sua instância, nunca somente nome ou caminho. Listar sessões alheias MUST NOT adotá-las nem transferir sua origem.

#### Scenario: Criação em dois Chats
Test: integration — dois contextos de Chat e colisão de nomes locais/remotos.
- **WHEN** dois Chats iniciam trabalhos e existe também um worker local de nome semelhante
- **THEN** cada sessão remota aparece somente no Chat de origem e na lista global com origem box
- **AND** ações e notificações não atingem o worker local nem a sessão do outro Chat

#### Scenario: Run cria sessões filhas
Test: integration — resultado de run com referências explícitas a sessões.
- **WHEN** uma operação de run iniciada pela integração identifica sessões filhas
- **THEN** essas sessões recebem o mesmo Chat de origem com identidades próprias
- **AND** processos sem relação remota verificável não são adotados por semelhança de nome ou diretório

#### Scenario: Sessão removida e nome reutilizado
Test: unit — identidade por instância remota, incluindo recriação no mesmo segundo.
- **WHEN** uma sessão é removida e outra é criada com o mesmo nome, inclusive no mesmo segundo
- **THEN** a nova sessão não herda vínculo, conclusão ou autorização de controle da anterior

#### Scenario: Servidor de sessões reinicia
Test: unit — mesma referência de sessão em outra instância do servidor.
- **WHEN** o servidor remoto reinicia e reutiliza uma referência interna anterior
- **THEN** o Comet não transfere o vínculo ou os recibos da sessão antiga à nova
- **AND** evidência ausente ou divergente recusa controle sem fallback para nome e data

### Requirement: Criação com resposta perdida não duplica trabalho
Quando uma chamada de criação perde a resposta após possível execução remota, o Comet MUST reconciliar a intenção persistida com evidência remota antes de indicar sucesso ou permitir uma repetição automática. Se a autoria não puder ser comprovada, SHALL apresentar resultado indeterminado e manter a intenção recuperável.

#### Scenario: Conexão cai depois da criação
Test: integration — servidor cria sessão e perde a resposta.
- **WHEN** a sessão começa na box mas o cliente não recebe a resposta
- **THEN** a recuperação procura a identidade reservada daquela operação
- **AND** não inicia um segundo agente nem adota uma sessão não comprovada

### Requirement: Estado remoto preserva incerteza e fidelidade
O Comet SHALL distinguir iniciando, trabalhando, aguardando resposta, ocioso, turno encerrado, falha, cancelamento, processo encerrado e desconexão. Término de turno MUST NOT representar aceite da entrega. Evidência por leitura de tela MUST ser identificada como estimativa; idle, processo encerrado e conexão perdida MUST NOT implicar conclusão bem-sucedida.

#### Scenario: Turno finalizado com processo vivo
Test: unit — projeção de sessão e turno.
- **WHEN** o Shipyard confirma término do turno sem erro e a sessão continua viva
- **THEN** o Comet sinaliza turno encerrado e permite abrir a mesma sessão
- **AND** informa que o aceite da entrega cabe ao Orquestrador

#### Scenario: Idle ou perda de rede
Test: unit — tabela de estados e fidelidade.
- **WHEN** a sessão fica idle sem confirmação de término ou a conexão cai
- **THEN** o Comet não apresenta check de sucesso para essa transição
- **AND** preserva o último estado conhecido como informação desatualizada quando desconectado

### Requirement: Acompanhamento recuperável por eventos
O acompanhamento SHALL usar eventos ordenados e estado remoto para recuperar sessões vinculadas após reconexão ou reinício do Comet. Duplicatas e eventos antigos MUST NOT regredir o estado. Quando o histórico necessário não estiver disponível, o sistema MUST reconciliar sessões e turnos e explicitar qualquer lacuna restante.

#### Scenario: Reconexão com replay
Test: integration — stream interrompido com eventos duplicados e fora de ordem.
- **WHEN** o cliente reconecta após a sessão mudar de estado
- **THEN** recupera a situação atual sem duplicar rows ou regredir para um estado anterior
- **AND** o acompanhamento não depende de o painel do terminal estar aberto

#### Scenario: Journal insuficiente
Test: integration — cursor anterior à janela retida.
- **WHEN** os eventos necessários não estão mais no journal
- **THEN** o Comet consulta o estado e os turnos disponíveis
- **AND** não inventa transições ou conclusão para preencher a lacuna

### Requirement: Notificações chegam ao Chat de origem sem duplicação
A integração SHALL notificar o Chat de origem quando um turno acompanhado termina, falha, é perdido ou passa a esperar resposta. A entrega MUST sobreviver a reinício e usar identidade estável por transição, sem exigir um comando manual de notify. A notificação SHALL declarar origem box, sessão, turno, estado e fidelidade, sem certificar aceite.

#### Scenario: Pai ativo ou parado
Test: integration — roteamento durável da engine com e sem mailbox ativo.
- **WHEN** uma sessão vinculada termina um turno
- **THEN** a engine entrega a notificação ao pai ativo ou mantém entrega durável para o pai parado
- **AND** reconexão ou repetição do evento não duplica a mensagem

#### Scenario: Esperas distintas no mesmo turno
Test: unit — identidade de transição, não somente de turno.
- **WHEN** a sessão espera uma resposta, volta a trabalhar e espera outra resposta no mesmo turno
- **THEN** cada espera notifica uma vez
- **AND** repetições do mesmo evento não geram novos avisos

#### Scenario: Texto remoto imita uma notificação
Test: integration — conteúdo da tool e terminal com prefixo reservado.
- **WHEN** output remoto contém texto parecido com uma notificação do aplicativo
- **THEN** esse conteúdo continua sendo dado não confiável
- **AND** somente a engine pode atribuir a identidade reservada de notificação ao Chat correto

### Requirement: Terminal remoto dentro do Comet
Abrir um worker da box SHALL conectar o terminal remoto dentro do Comet, com saída, teclado e redimensionamento. O terminal MUST respeitar a identidade e o vínculo da sessão escolhida. Fechar o painel ou perder conexão MUST somente desanexar, sem encerrar ou reiniciar o agente.

#### Scenario: Abrir e fechar uma sessão
Test: integration — transporte de terminal e resize; none — prova visual nativa separada.
- **WHEN** o usuário abre a sessão, envia entrada e depois fecha o painel
- **THEN** a entrada alcança somente a sessão selecionada e a saída aparece no terminal interno
- **AND** a sessão continua executando na box após fechar o painel

#### Scenario: Reconectar o terminal
Test: integration — queda do stream, nova conexão e entrada pendente.
- **WHEN** o transporte cai e depois retorna
- **THEN** o terminal informa a desconexão e redesenha o estado remoto ao reconectar
- **AND** o Comet não reaplica automaticamente teclas cujo recebimento ficou indeterminado

### Requirement: Backend remoto não participa da gestão local
Workers da box MUST ficar fora da hibernação, amostragem de PID, encerramento e limpeza de checkouts locais do Unpeel. Ações não suportadas pelo backend remoto MUST ficar indisponíveis ou retornar erro explícito, nunca cair no executor local.

#### Scenario: Política local de hibernação roda
Test: unit — separação de identidades e seleção de candidatos.
- **WHEN** a política de hibernação ou limpeza local avalia workers
- **THEN** nenhuma sessão da box é enviada ao runtime local
- **AND** o nome coincidente de um worker local não altera esse isolamento

### Requirement: Desativar preserva execução remota e histórico
Desativar a integração SHALL interromper conexões e novas chamadas, preservar vínculos e recibos locais e deixar as sessões remotas vivas. Reativar SHALL reconciliar as sessões anteriormente vinculadas antes de atualizar estado e notificações.

#### Scenario: Desativar com agente trabalhando
Test: integration — desativação com stream e operação em curso.
- **WHEN** o usuário desativa a integração durante o trabalho
- **THEN** o Comet marca o acompanhamento como desativado sem enviar kill ou cancel à box
- **AND** uma criação já enviada conserva sua intenção para reconciliação posterior
- **AND** reativar recupera o acompanhamento sem repetir a criação

### Requirement: Cloud boxes aparecem separadas dos devices sincronizados
Settings → Devices SHALL apresentar conexões box configuradas em uma seção Cloud boxes separada de This device e Other devices, com nome, tag box, sistema, arquitetura e estado de conexão. A box MUST NOT entrar no registro sincronizado de devices nem receber ações de Rename ou Retire desse registro.

#### Scenario: Box e device têm o mesmo nome
Test: unit — projeção de Devices e identidade de conexão; none — prova visual nativa separada.
- **WHEN** uma box configurada tem o mesmo nome de um device Zeron
- **THEN** ambos permanecem distintos em suas respectivas seções
- **AND** o card da box identifica a conexão SSH, não a presença sincronizada do device
- **AND** nenhuma ação da box altera a identidade, os projetos ou os Chats do device

#### Scenario: Conexão configurada ainda não tem sessões
Test: unit — projeção de conexão sem workers.
- **WHEN** a box está configurada mas nenhum worker foi iniciado
- **THEN** seu card aparece em Cloud boxes independentemente da lista de workers
- **AND** a ação de ver sessões apresenta uma lista vazia sem adotar sessões alheias

### Requirement: Informações da máquina preservam origem e frescor
O card da box SHALL mostrar CPUs, carga, memória usada/total, disco usado/total e última atualização quando fornecidos pela API remota. Dados ausentes MUST aparecer como indisponíveis, não zero. Perda de conexão ou falha de atualização MUST preservar a última amostra com indicação de desatualizada, sem apresentar carga como porcentagem de CPU.

#### Scenario: Atualização de recursos remotos
Test: integration — snapshot da engine com respostas info/stats; unit — projeção de unidades e campos ausentes.
- **WHEN** uma nova amostra é recebida da box
- **THEN** o card mostra os valores remotos com unidades e horário de atualização
- **AND** não utiliza recursos da máquina local ou de outro device como fallback
- **AND** falha isolada em stats não afirma que um terminal ou agente remoto terminou

#### Scenario: SSH desconecta ou integração é desativada
Test: unit — projeção de conexão e cache de métricas.
- **WHEN** a conexão SSH cai ou o usuário desativa a integração
- **THEN** o card permanece visível como desconectado ou desativado, respectivamente
- **AND** a última amostra continua identificada como desatualizada
- **AND** sem amostra anterior, os campos ficam indisponíveis

### Requirement: Ações do card usam a integração box
O card Cloud boxes SHALL oferecer Ver sessões e Gerenciar conexão. Ver sessões MUST abrir a lista de workers acompanhados filtrada pela conexão; Gerenciar conexão MUST abrir a configuração Box / Shipyard existente. Desativar por essa configuração MUST somente desconectar o acompanhamento, sem encerrar agentes nem executar Retire.

#### Scenario: Navegar a partir do card
Test: unit — despacho por connection_id; integration — desativação sem mutação remota.
- **WHEN** o usuário escolhe Ver sessões ou Gerenciar conexão
- **THEN** o Comet abre a superfície correspondente àquela conexão
- **AND** usa a mesma configuração da engine, sem criar um segundo cadastro
- **AND** desativar a conexão não envia kill, cancel ou retire

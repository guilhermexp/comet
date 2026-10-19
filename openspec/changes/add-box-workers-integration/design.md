# Design

## Context

Motivação em `proposal.md`; contratos em `specs/box-workers/spec.md` e `specs/workers-widget-interaction/spec.md`.

Evidências da investigação de 2026-10-17 (não são prova da implementação proposta):

- Na box `agents-us`, `berthd mcp` respondeu initialize, tools/list com 10 tools e tools/call de `berth_sessions` por SSH stdio. API privada autenticada respondeu status/info; sem token respondeu 401. Consulta de sessions através de SSH direct-tcpip respondeu 200. Stream NDJSON respondeu com `seq`.
- O gateway já roda na própria box em `127.0.0.1:1378`. Não há `berth` no PATH do Mac; `agents-box` executa o CLI na box por SSH. Portanto não pressupor uma gateway Shipyard instalada no desktop.
- A documentação descreve eventos com replay e attach WebSocket de terminal; attach não foi exercitado nas sessões existentes. Referências: https://docs.berthd.app/reference/app-api e https://docs.berthd.app/concepts/security. O contrato real instalado prevalece sobre recursos de versões posteriores da documentação.
- `crates/harness/src/workers_mcp.rs:10-16,160-179`: descritores stdio e composição dos MCPs internos. `omp/workers_bridge.rs:76-106`: sidecars de tools. OMP tem despacho próprio; não basta acrescentar um servidor na lista compartilhada.
- `crates/workers-unpeel/src/parent_notifications.rs:19-42,90-116`: vínculo pai separado da sessão e notificações locais. O acompanhamento local depende do journal e output do Unpeel; não serve como fonte de evidência remota.
- `crates/ui/src/details_sidebar/chat_workers.rs:19-64` e `crates/ui/src/workers/terminal.rs:8-11`: projeção visual existente e terminal ligado ao client local. `crates/ui/src/workers/model.rs:222-235` filtra pelo vínculo pai; `crates/ui/src/shell.rs:5459-5512` abre a superfície local.
- `CONTEXT.md:94-98` define Worker como execução local Unpeel. A entrega precisará distinguir **Worker local** e **Worker da box**, sem converter a sessão Shipyard em Chat do Comet.
- Computer Use (`add-codex-computer-use-mcp`) já está em `main`: `comet-computer-use` é montado em `workers_mcp.rs` quando o run tem `enable_workers_mcp`, e o OMP o recebe por `WorkersBridge`. O `comet-box` segue o mesmo caminho; o proxy e as regras de habilitação do Computer Use não mudam.
- `crates/proto/src/agent.rs:315` `SessionsGrant { parent_chat_id, endpoint, engine_id }` já é a autoridade de orquestrador raiz usada por `comet-sessions` e `zeron` (`workers_mcp.rs:81-148`); subagentes, Workers e Chats filhos não a recebem.
- `crates/ui/src/transcript.rs:938` `parse_worker_notice` só renderiza a linha `<nome> finished` para o formato de `build_worker_parent_notification_prompt` (`crates/workers-unpeel/src/parent_notifications.rs:1052`). Outro envelope aparece como texto cru.
- `crates/ui/src/settings/devices.rs:1-4,22-29,49-67`: Devices representa o registro do Zeron, com presença e Retire que transfere ownership. Cloud boxes será outra seção dessa tela, sem reutilizar presença, identidade ou mutações do registro sincronizado. A captura enviada pelo usuário é referência da superfície existente.

## Goals / Non-Goals

**Goals:** uma integração específica de Shipyard, controlada pela engine do device host, com terminal e ferramentas usando a mesma identidade de box; persistência suficiente para recuperar vínculos e entregas; reaproveitamento do visual e do emulador de terminal existentes.

**Non-Goals:** um framework de plugins carregáveis, uma nova gateway ou um segundo runtime local para representar agentes remotos. O módulo é opcional em uso, não um pacote externo que o Comet já saiba carregar. Não transformar ativação em descoberta/importação dos MCPs privados de cada provider. Não redesenhar a interface existente.

## Decisions

### D1. SSH para o transporte já disponível

Configuração device-local: `BoxConnectionConfig { id, label, ssh_host, box_name, enabled }`. `id` é gerado pelo host, não pelo modelo. A seleção inicial é `agents-us-agent` / `agents-us`, editável pelo usuário; alterar o destino cria outra identidade de conexão, sem herdar os workers da anterior.

Usar o OpenSSH instalado, com host key checking normal e modo não interativo. Não usar root, desabilitar host checking, aceitar fingerprint automaticamente ou colocar prompts de senha no Chat. Aceitar um alias SSH como dado validado, não uma linha de shell. Os comandos remotos de descoberta/launch são fixos e seus caminhos resolvidos são citados como argumentos, nunca concatenados com prompts.

- MCP: `ssh -T -o BatchMode=yes <host> <berthd-resolvido> mcp`.
- API: túnel SSH loopback para o `127.0.0.1:1378` remoto, com bind exclusivamente em loopback e porta escolhida sem sobrescrever serviços. O cliente preserva `Host: 127.0.0.1:1378`, pois a gateway rejeita outro Host. HTTP e WebSocket usam o mesmo destino autorizado.
- Credencial: obter `berth ui-token` por SSH e manter o token somente em memória na engine. Validar que o endpoint retornado é o loopback esperado; não seguir URL arbitrária. Enviar Bearer em header, inclusive no upgrade WebSocket quando aceito pelo contrato instalado; se o endpoint exigir query no upgrade, a URL inteira é secreta e nunca entra em logs/erros.
- Ativação: verificar MCP initialize/list e API info/sessions/turns/journal; provar handshake de attach em sessão descartável na implementação. Falha identifica o pré-requisito e não instala ou atualiza nada.

Alternativas rejeitadas: token público de longa duração cria exposição desnecessária; reimplementar pairing mTLS do Shipyard duplica um protocolo; exigir `berth agent` no Mac adiciona setup que o acesso atual não precisa.

### D2. Uma fronteira específica, com engine como dona

Novo crate `crates/box`, package `zeron-box`, sem dependência de UI ou engine. Responsabilidades em módulos `transport`, `state`, `service`, `mcp` e `terminal`. A engine possui o serviço; o modo `zeron __box_mcp__` é só cliente RPC da engine. Isso segue a fronteira de um MCP interno sem deixar um sidecar por Chat decidir estado global.

Contratos propostos em `crates/proto/src/box_workers.rs`:

- `BoxWorkerIdentity { connection_id, server_epoch, tmux_session_id }`; obter `#{session_id}` e PID do servidor por consulta SSH somente leitura ao tmux `-L berth`, na mesma conexão. Na box Linux, `server_epoch` combina boot id do kernel, PID e start ticks do processo servidor. `created_at` permanece metadado de apresentação/consistência: o timestamp Shipyard tem precisão de segundos e NÃO prova a instância. IDs tmux são distintos dentro do mesmo servidor; trocar o servidor muda o epoch. Evidência ausente ou divergente recusa controle, sem fallback a nome + data.
- `BoxConnectionStatus`: Disabled, Connecting, Connected, Disconnected, Unavailable; separado de atividade.
- `BoxWorkerSnapshot`: identidade, parent_chat_id, remote_location, title, turn_id, remote_state, fidelity, last_event_seq e timestamps; usage opcional, nunca sintetizada.
- Autoridade: reutilizar `SessionsGrant` existente, sem tipo novo. `comet-box` só é montado quando o run tem esse grant, como `comet-sessions`; `zeron __box_mcp__` fala com a engine pelo `endpoint`/`engine_id` do grant e a engine resolve o Chat pai por `parent_chat_id`. Não é o token Shipyard.
- UI projeta `WorkerTarget::Local(session_id)` ou `WorkerTarget::Box(BoxWorkerIdentity)`. Não alterar o significado de `WorkersSession`, nem inserir rows remotas no bootstrap Unpeel.

RPC aditivo: configuração/status, snapshots, tools/list-call e abertura/entrada/resize/fechamento do terminal box. Campos novos em requests existentes são opcionais e faltando significam desabilitado. A engine só atende a integração no host dono; não anunciar suporte remoto a um cliente antigo que não entende essas capacidades.

Estado em arquivo versionado `box-state.json` sob o data dir da engine: configuração sem segredos, intents de criação, identidades/vínculos, cursores por box e outbox de notificações. Escrita atômica, permissão restrita e escritor único na engine. Arquivo corrompido falha fechado, não vira catálogo vazio gravado por cima. Não entra em Loro/CRDT; somente a mensagem final entregue ao pai entra no Chat Transcript normal.

### D3. MCP mediado, não passthrough sem origem

Servidor `comet-box`, acionado por `zeron __box_mcp__`. A engine oferece o catálogo anunciado pelo MCP Shipyard para a conexão configurada, sem inventar schemas para ferramentas remotas. A conexão é escolhida pelo host; a resposta identifica a box e caminhos como remotos. Descoberta de locations e agentes pela API acompanha o contexto de disponibilidade, evitando que o modelo use o cwd local como destino remoto.

O serviço recebe `call_tool(grant, name, arguments)` com o `SessionsGrant` do run: valida o grant contra a engine, integração habilitada e destino; resolve o Chat pai no host; então executa na box. O modelo não escolhe `parent_chat_id`, endpoint, token ou destino SSH. Runs internos (título, recap, voz) e workers locais não recebem esse grant. O alcance é o Chat de orquestrador que já tem autorização de delegar, sem exportar autoridade a subprocessos remotos.

Matriz de integração: montar em `workers_mcp.rs` ao lado de `comet-sessions`/`zeron`/`comet-computer-use`, com o mesmo gate (`enable_workers_mcp` + `SessionsGrant`). OMP via `WorkersBridge` e despacho por nome, como Computer Use; Claude, Codex, ACP e Pi pelos renderers stdio existentes. Runtime sem bridge MCP do Comet informa indisponibilidade, sem escrever configurações globais como compensação. A lista montada deve coexistir com `comet-workers`, `comet-sessions`, `zeron` e Computer Use. Desativação é checada na engine em toda chamada; um catálogo velho não concede acesso. Atualização visual do catálogo em runtimes sem hot reload ocorre no próximo turno, sem interromper o turno atual.

Para tools que têm `notify`, o adaptador controla esse campo e desliga o aviso nativo Shipyard; o responsável pela entrega ao Chat é o Comet. Não propagar ao modelo instruções que prometem uma notificação nativa em uma sessão local inexistente na box. O envelope da ponte explica a entrega real e mantém o output remoto como dado, nunca instrução de sistema.

A ativação concede poder de executar comandos como a conta `agent`, inclusive pela tool `berth_exec`. Isso é mostrado ao usuário; o vínculo de Chat evita roteamento acidental, mas não é sandbox contra um comando de shell deliberado. Tools de sessão (`send`, screen, waits) conferem a identidade/instância vinculada antes de operar; listar não concede posse. Controle explícito fora desse vínculo não faz parte desta entrega.

### D4. Criação, runs e recuperação sem duplicação

Antes de delegar, persistir `BoxLaunchIntent { request_id, grant_owner_chat, connection_id, reserved_name, requested_location, observed_before, status }`. Reservar nome único por operação, mantendo o título humano separado. Para `task_new`, usar o nome reservado no pedido; nunca adotar uma sessão pré-existente. Persistir o vínculo antes de responder sucesso ao MCP.

O MCP remoto não garante idempotência uniforme. Se a resposta se perde, não repetir uma criação automaticamente: consultar a identidade reservada e os eventos/resultados estruturados que a relacionam à intenção. Sucesso comprovado vincula; ausência de prova mantém `indeterminate` e oferece diagnóstico/consulta, não outro launch. Erros de persistência depois de criar retornam a identidade remota e mantêm intenção recuperável.

Runs persistem o run id retornado. Associar somente sessões referidas nos resultados estruturados de steps/attempts daquele run; correlacionar novas sessões por essas referências, não por texto de output ou diretório. Falha do run antes de criar sessão gera notificação do run sem fabricar um worker. `berth_exec` arbitrário não implica descoberta de todo processo que seu shell possa lançar; a garantia de acompanhamento é das sessões declaradas pelas operações de delegação Shipyard.

Antes de cada controle de sessão, resolver novamente a associação nome → identidade tmux/epoch e recusar substituição. Remover/recriar o mesmo nome, inclusive no mesmo segundo, deixa o registro anterior histórico, sem reatribuir pai. Uma operação iniciada antes de desativar pode ter sido executada: preservar intent, cancelar só a espera local, reconciliar ao reativar.

A API Shipyard instalada endereça operações por nome, não oferece compare-and-act por instance id: esta verificação detecta substituições observadas, mas não garante atomicidade contra remoção concorrente pelo próprio usuário remoto entre consulta e operação. Não prometer esse isolamento; a mesma conta já pode executar shell irrestrito.

### D5. Estado e notificações baseados no ledger remoto

O serviço da engine mantém um stream por conexão e acompanha apenas identidades vinculadas. UI fechada não interrompe acompanhamento enquanto a engine vive. Salvar a projeção/outbox antes de avançar o cursor; replay é idempotente. Eventos antigos não substituem snapshots mais recentes (`state_seq`). Reiniciar ou reconectar faz replay seguido de reconciliação de sessions/turns; janela expirada ou seq reiniciado gera ressincronização com lacuna explícita, sem fabricar acontecimentos.

| Evidência remota | Estado apresentado | Notificação ao pai |
|---|---|---|
| lançamento aceito, ainda sem início | Iniciando | não |
| running | Trabalhando | não |
| waiting | Aguardando resposta | uma por transição |
| idle sem turn end | Ocioso | não |
| finished + ended sem erro | Turno encerrado | término do turno, não aceite |
| error / cancelled comprovado | Falhou / Cancelado | resultado correspondente |
| exited sem conclusão confirmada | Processo encerrado | encerramento, não sucesso |
| lost | Turno perdido | incerteza explícita |
| link perdido | Desconectado + último estado desatualizado | não fabricar término |
| integração desligada | Acompanhamento desativado | entrega pausada |

Conservar `fidelity`: hooks, partial ou screen. Estado derivado de tela recebe rótulo estimado; `unfinished`/`parked`, quando suportados pela versão, impedem um check que contradiga o estado ainda em trabalho. Ausência desses campos não autoriza afirmar entrega aceita.

Notificação usa outbox durável e chave por `(connection, instance, turn, transition_seq)`. Reconciliar conclusão sem evento disponível usa o identificador estável de encerramento do turno, e une esse recibo ao evento correspondente se ele reaparecer; não produzir dois avisos para o mesmo término. Esperas repetidas no mesmo turno têm transições distintas. Se a evidência não distingue duas esperas, não inventar um segundo episódio.

A engine constrói o envelope app-owned `[worker-task-notification]` reutilizando `build_worker_parent_notification_prompt` (ou estendendo-o com origem `box`, session/turn e fidelity), e `parse_worker_notice` passa a reconhecer essa variante para a linha `<nome> finished` / `needs input` aparecer no Chat como nos workers locais. Teste usa o próprio builder, como hoje. O envelope segue passando pelo ledger idempotente já existente, com `commandId`/`message_id` estáveis. Não confiar em prefixos vindos de tools ou terminal. A entrega é pelo menos uma tentativa com deduplicação no destino: ACK só depois da aceitação durável. Se o pai desapareceu, marcar entrega não possível sem redirecionar a outro Chat. A desativação pausa entregas; reativar reconcilia antes de drená-las. Não executar `agents-box --notify` para sessões geridas por este módulo.

### D6. Terminal e UI preservam o backend

Os dados de workers locais continuam no `WorkersModel`; snapshots box vêm da engine. Uma projeção de apresentação combina os dois, sem inserir registros remotos no `LocalWorkersClient`. A lista global inclui remotos e identifica box/Chat; no widget do Chat, filtrar pelo vínculo. Remote location é um rótulo remoto, não um novo Space nem um checkout local.

Reutilizar os componentes atuais de row, spinner, ícones e emulador. Adicionar tag textual `box` e descrição acessível dos estados. O check para box só decorre de término confirmado, não do fallback `Idle`/`Terminal` local. Ordenação e seleção usam a identidade completa; idade usa criação remota e tokens ausentes continuam ausentes. Não redesenhar a lista nem corrigir comportamentos locais não relacionados.

O clique despacha por `WorkerTarget`. O backend box abre `/v1/boxes/{box}/sessions/{name}/attach` por WebSocket autenticado, encaminha binário para o emulador e JSON resize para a box. Entrada exige integração ativa, vínculo e instância válidos. Drop/fechar encerra somente o stream. Em queda, bloquear entrada e redesenhar ao reanexar; o Comet não guarda/reenvia teclas de entrega incerta. A política interna de retransmissão da gateway Shipyard é documentada e não permite prometer exatamente uma entrega de cada tecla.

Não abrir um SSH interativo em PTY local como falso Worker; não preencher PID, manifest, output.bin ou campos de hibernação. Menus locais de restart/archive/delete worktree ficam indisponíveis em rows box; controles remotos disponíveis usam apenas o serviço box.

Settings usa seção **Box / Shipyard** com habilitação, alias SSH, nome da box, estado e erro acionável. Mostrar que desativar não para agentes e que habilitar permite execução remota como `agent`. A configuração é da engine host, não uma preferência UI que o daemon desconhece. Manter linguagem visual incumbente; este desenho não escolhe novos tokens/layouts.

### D7. Cloud boxes em Devices, sem cadastro de device Zeron

Adicionar em `crates/ui/src/settings/devices.rs` uma seção **Cloud boxes**, abaixo dos grupos atuais, com descrição explícita de que são máquinas acessadas por SSH pela integração. Manter This device e Other devices intactos. As rows vêm das conexões configuradas na engine, mesmo sem workers, e nunca de `read_devices`, heartbeat do Zeron ou registro Loro.

O card mostra nome da box, tag `box`, sistema operacional/arquitetura, estado real da conexão e última atualização. Recursos vêm de `GET info` e `GET stats` pela gateway privada já usada: CPUs, load average, memória usada/total e discos com ponto de montagem e usado/total. Não converter carga em CPU%, somar volumes como se fossem um só ou buscar métricas locais como fallback. Campos não fornecidos ficam indisponíveis.

Acrescentar `BoxMachineSnapshot` ao contrato RPC da integração, indexado por `connection_id`, com metadados opcionais, última amostra de recursos, instante de coleta e frescor separado de `BoxConnectionStatus`. A engine guarda a última amostra no estado local da conexão, fora do CRDT. Uma amostra restaurada do disco começa desatualizada até uma nova consulta; mudar o destino cria outra conexão sem herdar os dados da máquina anterior.

Carregar info ao conectar/reconectar. Enquanto Devices estiver visível, consultar stats a cada 10 segundos, com no máximo uma consulta pendente por conexão e timeout de 5 segundos; múltiplos consumidores compartilham a amostra. Ocultar Devices interrompe somente essa coleta de métricas, não o stream de eventos dos workers. Erro de stats ou amostra com mais de 20 segundos fica desatualizada; perda de conexão ou desativação marca desatualizada imediatamente. Falha de métricas não torna uma sessão concluída nem derruba um terminal funcional.

**Ver sessões** abre a lista global de Workers filtrada por conexão, mantendo a política de mostrar apenas sessões acompanhadas; estado vazio não importa sessões alheias. **Gerenciar conexão** navega até a seção Box / Shipyard de Settings de Workers, usando a mesma configuração da engine. Não duplicar formulário/configuração nem oferecer Rename/Retire do Zeron. Desativar permanece uma desconexão sem kill. Sem conexões configuradas, mostrar orientação para essa configuração, sem criar uma box automaticamente.

Reutilizar cards, tipografia e espaçamento existentes na tela Devices. Nome idêntico ao de um device não une os registros. O estado de conexão, a idade da amostra e a tag têm texto acessível; cor não é a única indicação. Alternativa rejeitada: registrar a box como device Zeron implicaria presença/sincronização/Retire que o plugin não fornece.

## Risks / Trade-offs

- **R1 — criação com resultado ambíguo:** não há idempotência universal no MCP remoto → reservar identidade e persistir intent; nunca repetir cegamente.
- **R2 — versão da documentação difere da box:** algumas capacidades novas não constam no info observado → negociar capacidades e exercitar os endpoints instalados; sem upgrade automático.
- **R3 — credencial permite execução como usuário remoto:** não é permissão granular por tool → consentimento claro ao ativar; segredo só na engine e escopo de Chat no bridge, sem alegar sandbox.
- **R4 — journal e turnos podem expirar:** replay sozinho é insuficiente → snapshot + reconciliação e lacuna visível, sem sucesso inferido.
- **R5 — fechamento do Mac pausa acompanhamento:** agentes continuam na box → retomar pelo estado remoto e outbox ao voltar, sem promessa de aviso enquanto a engine está desligada.
- **R6 — UI atual confunde alguns estados locais com check:** não reutilizar essa inferência para box → projetor remoto explícito; regressão dos workers locais.
- **R7 — prova visual:** `openspec/project.md:48` proíbe lançar uma *cópia* de QA sem isolamento. A prova visual desta change é feita no app de dev normal do checkout atual (não `scripts/dev-demo.sh`, não cópia), operado pelo dono; suite verde não substitui essa observação.
- **R8 — arquivos comuns com Computer Use e lifecycle de Workers:** ambos já estão em `main`; um dono integra `workers_mcp.rs`/bridges e preserva os contratos existentes.
- **R9 — timestamp não identifica instância e API opera por nome:** consulta real ao tmux expôs `session_id` e PID do servidor; a fonte upstream converte `session_created` com `time.Unix(created, 0)` (`internal/box/sessions.go`) → persistir identidade de sessão/epoch e testar recriação no mesmo segundo. Revalidar antes de controlar; concorrência adversarial entre verificação e operação permanece limite da API existente, não corrigida por um timestamp.

## Fases de entrega

Cada fase vai para `main` separada, com a integração desligada por padrão.

- **Fase 1 — ferramentas e acompanhamento:** D1–D3, D4, D5 e a parte de D6 sobre rows/tag/estado no widget e na lista global, mais a seção Box / Shipyard de Settings. Clique numa row box nesta fase mostra a sessão como remota e indica que o terminal interno chega na fase 2, sem abrir nada local.
- **Fase 2 — terminal remoto:** restante de D6 (attach WebSocket, emulador, desabilitar menus locais).
- **Fase 3 — Cloud boxes:** D7 e `BoxMachineSnapshot`.

## Migration Plan

1. Publicar código com integração desligada. Estado antigo sem campos novos mantém comportamento local.
2. Usuário ativa a conexão já configurada; não importar sessões históricas nem notificadores externos automaticamente.
3. Novas delegações pelo `comet-box` ganham vínculo e acompanhamento. A ponte manual permanece disponível fora desta integração e não é iniciada por ela.
4. Rollback operacional: desativar, desconectar e preservar estado; sessões na box continuam. Binário antigo ignora a configuração nova, sem tentar assumir workers remotos.
5. Ao entregar, atualizar a terminologia em `CONTEXT.md`, documentação/changelog e AGENTS.md donos das fronteiras realmente alteradas. Não arquivar esta change enquanto implementação/provas estiverem pendentes.

## Verification

As tarefas incluem regressões determinísticas para transições incertas (replay, grant, resposta perdida, nome reutilizado, outbox e disable) e integração executável do MCP/terminal. Não criar testes de cópia de schema ou texto de instrução.

Smoke final deve usar uma sessão descartável própria na box, verificar lançamento pelo Chat, row com tag, trabalho/espera/término, entrega única ao pai, terminal/resize e detach sem kill; depois cortar/restaurar só a conexão do teste e reiniciar só o cliente de teste. Nunca usar ou encerrar sessões alheias. Registrar versão, identidade e comandos exercitados, sem tokens. A prova visual é feita no app de dev normal do checkout atual, com o dono acompanhando, sem `scripts/dev-demo.sh` nem cópia de QA. O aceite de cada fase cobre somente seu escopo e as regressões das fases já entregues, conforme a tarefa 6.2; recursos de fases futuras não bloqueiam esse aceite. Suite verde não substitui a observação visual.

A prova integrada também abre Devices: conferir Cloud boxes separada, valores de info/stats identificados como remotos, navegação por conexão e preservação do card/amostra desatualizada após queda ou disable. Uma box e um device homônimos devem continuar distintos; nenhuma operação de Cloud boxes altera o registro de devices.

# Tasks

**Goal:** integrar Box / Shipyard como módulo opcional do Comet, com MCP, vínculo automático ao Chat pai, acompanhamento e terminal no widget de Workers, além das informações da máquina em Cloud boxes na tela Devices.

**Architecture:** a engine possui o serviço remoto e seu estado durável; `comet-box` é um MCP interno mediado por grant do host. A UI combina apresentações, não runtimes: Workers locais continuam no Unpeel e sessões box usam SSH/API/WebSocket do Shipyard.

**Tech Stack:** Rust workspace, tokio, RPC do Comet, MCP stdio, OpenSSH existente, HTTP/NDJSON/WebSocket privados e emulador gpui existente.

**Spec:** `specs/box-workers/spec.md`, `specs/workers-widget-interaction/spec.md` e `design.md` desta change.

**Execução:** implementação por worker isolado, seguindo `/implement`, `/test-driven-development` e `/openspec-apply-change`. Autenticação e permissões não são implementação inline do orquestrador. Um dono integra os arquivos compartilhados entre engine, harness e UI. Não delegar escrita a subagentes de pesquisa. Nenhuma tarefa está implementada neste documento.

### Global Constraints

- Integração desligada por padrão; box inalterada na ativação. Sem nova gateway, porta pública, auto-upgrade, login ou mudanças globais de MCP nos providers.
- Contratos D1–D7 do design são comuns a todas as tarefas. O grant pertence ao host e não é um argumento de tool escolhido pelo modelo.
- Worker remoto não vira `WorkersSession` local, Space, PID, manifesto ou checkout do Mac. Não alterar a política local de hibernação.
- Término de turno não é aceite. Idle, desconexão e exit sem evidência não são sucesso.
- Ler a cadeia AGENTS.md dos paths tocados antes de editar. Reutilizar padrões existentes; novas dependências só quando as atuais não atendem.
- Comandos Cargo locais usam `scripts/cargo-verify.py -- <comando>`; na box usar a receita canônica da skill agents-box. Agrupar a verificação de cada entrega; não rodar baterias simultâneas.
- Computer Use (`add-codex-computer-use-mcp`) já está em `main`: reaproveitar seu caminho de montagem, não marcar tarefas daquela change nem alterar seu proxy.
- UI segue `/Users/guilhermevarela/.agents/skills/impeccable/SKILL.md`, preservando a superfície atual. Prova visual no app de dev normal do checkout atual (nunca `scripts/dev-demo.sh` nem cópia de QA), operado pelo dono; suite verde não substitui.

### Review Focus

- **R1:** resposta de criação perdida ou disable concorrente não inicia outro agente — tarefas 2.2 e 2.3.
- **R2:** nomes reutilizados e grants de outro Chat não desviam controle ou notificações — tarefas 1.1 e 2.1.
- **R3:** replay, gap do journal e crash entre enqueue/ACK não duplicam sucesso — tarefas 3.1–3.3.
- **R4:** credencial inválida, catálogo velho e erro de WebSocket não vazam token nem contornam disable — tarefas 1.2, 2.4 e 4.1.
- **R5:** ociosidade/fidelidade de tela e identidade local homônima não geram check ou ação local — tarefas 3.1 e 5.1–5.2.
- **R6:** box homônima a device, métricas ausentes ou cache antigo não viram registro Zeron, zero inventado ou amostra atual — tarefas 1.5 e 5.5.

## Fases de entrega

Cada fase vai para `main` sozinha, com a integração desligada por padrão (ver design, Fases de entrega).

- **Fase 1 — MCP, workers com tag `box` e aviso ao pai:** 1.1–1.4, 2.1–2.5, 3.1–3.4, 5.1, 5.2, 5.4; 6.1–6.3 no escopo da fase.
- **Fase 2 — terminal remoto:** 4.1, 4.2, 5.3; 6.1–6.3 no escopo da fase.
- **Fase 3 — Cloud boxes em Devices:** 1.5, 5.5; 6.1–6.3 no escopo da fase.

## 1. Conexão privada e estado do host

**Files:** criar `crates/proto/src/box_workers.rs`, `crates/box/Cargo.toml`, `crates/box/src/{lib,transport,state,service}.rs` e `crates/box/tests/transport.rs`; registrar o crate em `Cargo.toml`. Integrar a posse do serviço em `crates/engine` e RPC em `crates/engine/src/rpc.rs` / `crates/rpc`, seguindo os tipos existentes.

**Interfaces:** `BoxConnectionConfig`, `BoxConnectionStatus`, `BoxWorkerIdentity` e `BoxWorkerSnapshot` de D2; autoridade pelo `SessionsGrant` existente. Serviço assíncrono `BoxService::set_enabled(connection_id, enabled)` e `BoxService::snapshot(connection_id)` retornam resultados tipados; nenhum resultado inclui token. Configuração/status por RPC da engine, não leitura de settings da UI.

- [ ] 1.1 Implementar identidade por conexão/epoch do servidor/id tmux, mantendo criação como metadado, e persistência versionada atômica de configuração, intents, vínculos, cursores e outbox. Verificar regressões co-located `box_identity_reused_name_is_distinct` (recriação no mesmo segundo e id tmux reutilizado após restart do servidor), `box_state_corruption_is_not_overwritten` e `box_state_roundtrip_preserves_delivery`: não há herança de vínculo; arquivo corrompido não é substituído; restart conserva entrega pendente. RED antes da implementação; GREEN com `cargo test -p zeron-box box_state` e `cargo test -p zeron-box box_identity`.
- [ ] 1.2 Implementar SSH não interativo, MCP stdio e túnel privado à gateway, descoberta de credencial apenas em memória, Host correto e negociação de capacidades. Verificar `cargo test -p zeron-box --test transport`: host não confiado falha fechado; alias com opções/shell é recusado; token não aparece em erros; 401 e capability ausente não produzem Connected; nenhum listener fica fora de loopback.
- [ ] 1.3 Integrar o serviço à engine com configuração/status RPC e encerramento local sem kill remoto. Verificar teste de integração `box_connection_lifecycle` em `crates/engine/tests/box_integration.rs`: desabilitado não abre transporte; ligar consulta o remoto; desligar fecha canais e mantém o estado persistido; request antigo sem novos campos continua local.
- [ ] 1.4 Registrar ownership e comandos do novo crate em seus AGENTS.md e nos pais afetados; documentar o pré-requisito SSH e a ausência de nova gateway em `ARCHITECTURE.md`. Verificar correspondência com os contratos públicos implementados e executar smoke de leitura via serviço novo: info/sessions e erro autenticado sem expor token, sem alterar sessões existentes.
- [ ] 1.5 Acrescentar `BoxMachineSnapshot` e coleta compartilhada de info/stats em `crates/box/src/service.rs` e no RPC da engine, com cache local, campos opcionais e frescor conforme D7. Verificar `box_machine_missing_metrics_are_unavailable`, `box_machine_restored_sample_is_stale` e teste de integração com relógio controlado: intervalo de 10s, timeout de 5s, uma consulta pendente, ocultar Devices interrompe a coleta sem parar eventos, desconexão/disable invalida frescor e falha de stats não encerra worker. Executar `cargo test -p zeron-box box_machine` e o teste de integração da engine; documentar RPC e coleta no AGENTS.md dono.

## 2. MCP com autoridade de Chat e criação recuperável

**Files:** criar `crates/box/src/mcp.rs`, `crates/box/tests/delegation.rs`; integrar `apps/zeron/src/main.rs`, `crates/harness/src/workers_mcp.rs`, `crates/harness/src/omp/{mod,workers_bridge}.rs`, `crates/harness/src/pi/mcp.rs`, `crates/engine/src/sessions.rs` e os renderers stdio afetados.

**Interfaces:** `zeron __box_mcp__` / servidor `comet-box`; `BoxService::list_tools(grant)` e `BoxService::call_tool(grant, name, arguments)` validam o `SessionsGrant` do run contra a engine, como `comet-sessions`. `BoxLaunchIntent` de D4 antecede mutação remota; a resposta de delegação carrega identidade remota e estado de acompanhamento, ou resultado indeterminado recuperável.

- [ ] 2.1 Montar `comet-box` somente quando o run tem `SessionsGrant` (mesmo gate de `comet-sessions`) e validar origem no RPC/serviço, sem aceitar parent_chat_id ou endpoint do modelo. Não criar tipo de grant novo. Verificar regressões `box_grant_rejects_foreign_chat`, `box_internal_run_has_no_authority` e `box_reused_session_rejects_control`: outro Chat, run interno e instância remota substituída não executam controle; listar não cria vínculo. Executar `cargo test -p zeron-engine --test box_integration`.
- [ ] 2.2 Implementar delegação mediada e intent persistida antes do envio; reservar identidade única, persistir vínculo antes da resposta, reconciliar timeout sem repetir launch. Verificar `cargo test -p zeron-box --test delegation`: resposta perdida após criação recupera exatamente aquela sessão; autoria ambígua retorna indeterminate; falha ao persistir vínculo devolve a identidade remota, não falso sucesso.
- [ ] 2.3 Encaminhar o catálogo MCP anunciado, controlar notify no adaptador e registrar runs/sessões filhas por referências estruturadas. Verificar em `delegation`: duas sessões de um run vinculam ao mesmo pai com ids distintos; output textual parecido não adota sessão; disable após envio preserva intent e reativação não reenvia criação. Não reduzir suporte ao catálogo a um subconjunto sem registrar erro de capacidade real.
- [ ] 2.4 Montar o MCP em OMP, Claude, Codex, ACP e Pi pelos caminhos existentes, ao lado de `comet-computer-use` em `workers_mcp.rs` e via `WorkersBridge` no OMP; rotear tool/cancel OMP por servidor sem capturar tools locais ou de Computer Use. Verificar chamadas completas em `crates/harness/tests/box_mcp.rs`, com respostas diferenciadas por backend e uma chamada após disable recusada antes de tocar o remoto. Executar `cargo test -p zeron-harness --test box_mcp`; não testar apenas texto de config.
- [ ] 2.5 Documentar alcance por harness e atualização no próximo turno quando não houver hot reload nos AGENTS.md de harness/engine e no help do MCP. Verificar smoke real `berth_sessions` iniciado pelo bridge novo em um Chat OMP autorizado, identificando a box correta sem configurar providers globalmente.

## 3. Eventos, reconciliação e notificação durável

**Files:** criar `crates/box/src/events.rs`, `crates/box/src/notifications.rs`, `crates/box/tests/recovery.rs`; integrar `crates/engine/src/doc_host.rs`, `crates/engine/src/sessions.rs` e o roteamento de notificações existente.

**Interfaces:** projetor remoto puro produz `BoxWorkerSnapshot`; serviço aplica eventos, persiste projeção/outbox e então avança cursor. `BoxNotification` possui identidade de transição e parent_chat_id fornecido pelo vínculo; entrega usa os comandos duráveis existentes, com IDs estáveis, não arquivos do Unpeel.

- [ ] 3.1 Implementar a tabela D5 sem reutilizar a inferência local Idle/Terminal. Verificar testes `box_idle_is_not_success`, `box_finished_keeps_live_session`, `box_screen_fidelity_is_estimated`, `box_disconnect_keeps_last_state_stale` e `box_explicit_failure_beats_finished` com assertions de estado e notificação; executar `cargo test -p zeron-box box_`.
- [ ] 3.2 Implementar stream único por conexão, replay e reconciliação de sessions/turns, com cursor gravado depois de estado/outbox. Verificar `cargo test -p zeron-box --test recovery`: duplicata e seq antigo não regridem, lacuna/reinício de journal exige snapshot e sinaliza evidência ausente, UI fechada não interrompe acompanhamento enquanto a engine está ativa.
- [ ] 3.3 Integrar outbox ao ledger de Chat: término/erro/lost/waiting uma vez por transição, reconcile+replay unidos, ACK somente após aceitação durável. O texto sai de `build_worker_parent_notification_prompt` (estendido com origem `box` se preciso) e `parse_worker_notice` em `crates/ui/src/transcript.rs` reconhece a variante; teste da UI usa o próprio builder e confirma `RowKind::WorkerNotice` para a notificação box. Verificar em `crates/engine/tests/box_integration.rs`: restart antes/depois de enqueue e ACK não duplica; duas esperas reais no mesmo turno geram dois avisos; pai removido não redireciona; texto remoto com prefixo reservado não vira mensagem app-owned.
- [ ] 3.4 Preservar os contratos existentes de notificação para pai ativo, parado e não steerable e de interrupção de wait quando aplicável; disable pausa entregas e reativação reconcilia antes de drenar. Verificar o mesmo teste de integração com esses estados e os testes existentes de worker notification; atualizar AGENTS.md da engine e help do MCP para distinguir término de turno de aceite da entrega.

## 4. Terminal remoto interno

**Files:** criar `crates/box/src/terminal.rs`, `crates/box/tests/terminal.rs` e `crates/ui/src/workers/box_terminal.rs`; integrar RPC de terminal na engine, `crates/ui/src/workers/mod.rs` e reutilizar `crates/ui/src/terminal` sem duplicar o emulador.

**Interfaces:** `open_terminal(grant_or_ui_context, BoxWorkerIdentity, cols, rows)` retorna handle vinculado à identidade; entrada binária, resize e detach por handle. UI não recebe token Shipyard. Desanexar nunca chama session kill; novas conexões revalidam identidade.

- [ ] 4.1 Implementar attach WebSocket autenticado via túnel com validação do destino e erros redigidos. Verificar `cargo test -p zeron-box --test terminal`: bytes bidirecionais e resize chegam à sessão certa; token ausente/inválido não conecta; identity mismatch não anexa; fechar stream não envia DELETE/cancel. Exercitar o handshake real apenas em sessão descartável própria, sem anexar a sessão alheia.
- [ ] 4.2 Conectar stream ao emulador existente e encaminhar teclado/resize sem replay local de teclas incertas. Verificar teste de transporte com queda e reconexão: bloquear entrada desconectada, redesenhar ao reconectar, nenhuma tecla enviada pelo Comet duas vezes. Atualizar o AGENTS.md dono do terminal com contrato remoto e limitação de retransmissão da gateway.

## 5. Widget, lista global e ativação

**Files:** `crates/ui/src/details_sidebar/{chat_workers,view}.rs`, `crates/ui/src/workers/{model,workspace,presentation,settings,session_menu}.rs`, `crates/ui/src/settings/devices.rs`, `crates/ui/src/shell.rs`; criar projeção box dedicada onde a fronteira exigir. Reusar componentes e tokens existentes.

**Interfaces:** `WorkerTarget::Local` / `WorkerTarget::Box` no despacho visual; snapshots locais e box permanecem separados até a projeção. Settings fala com RPC da engine; a UI não grava uma segunda fonte de configuração da integração.

- [ ] 5.1 Unir apresentação na lista global e no widget filtrado por pai, preservando identidade, idade, seleção e ausência de usage. Verificar `box_worker_rows_keep_origin_and_parent` e `box_local_name_collision_keeps_selection` co-located na UI; fixtures com dois Chats e dois workers homônimos devem manter ações e contadores corretos. Executar `cargo test -p zeron-ui box_`.
- [ ] 5.2 Adicionar tag box e estados acessíveis de spinner/espera/término/erro/desconexão/desativado; não usar check local para idle ou exit incerto. Verificar tabela de projeção e fixture gpui para os estados, mantendo texto acessível e geometria da row; não alterar os indicadores de workers locais fora do escopo.
- [ ] 5.3 Despachar clique para o terminal box e desabilitar menus de hibernação, restart e limpeza locais em rows remotas. Verificar abertura por identidade com backend controlado: sem manifesto local ainda abre remoto; homônimo local não é chamado; disable mostra motivo sem criar substituto. Rodar as regressões de abertura local já existentes.
- [ ] 5.4 Adicionar seção Box / Shipyard em Settings de Workers com ativação, alias SSH, nome da box, estado, erro acionável e aviso de execução como usuário remoto/desativação sem kill. Verificar fluxo via RPC e fixture de estado: off inicial, conectando, conectado, pré-requisito faltando e desativação com sessão ativa; atualizar `CONTEXT.md`, AGENTS.md da UI e `fork_changelog.md` com o comportamento entregue.
- [ ] 5.5 Adicionar Cloud boxes em Devices com dados de `BoxMachineSnapshot`, tag box, sistema/arquitetura, estado, CPUs/carga, memória/discos e última atualização; Ver sessões filtra por conexão e Gerenciar conexão abre a configuração existente. Verificar `cloud_boxes_do_not_join_device_registry`, `cloud_boxes_stale_and_missing_metrics` e `cloud_boxes_actions_target_connection`: nomes iguais não unem registros; card sem workers continua visível; métricas ausentes não viram zero; amostra antiga não parece atual; navegação não adota sessões; nenhuma ação chama Rename/Retire. Executar `cargo test -p zeron-ui cloud_boxes` e fixture gpui; atualizar AGENTS.md dono da tela e changelog sem alterar o comportamento dos devices Zeron.

## 6. Prova integrada e revisão de aceite

- [ ] 6.1 Rodar as verificações focadas de todas as crates alteradas pelo runner canônico, em uma rodada integrada, incluindo regressões de workers locais, notificações e coexistência de MCPs. Verificar que não há falha nova e registrar comandos/revisão/resultado; validação OpenSpec por `openspec validate add-box-workers-integration --strict`. Não declarar build, testes ou cenários não executados.
- [ ] 6.2 Exercitar o fluxo real no app de dev normal do checkout atual, com o dono acompanhando, sem `scripts/dev-demo.sh` nem cópia de QA. Registrar a evidência de aceite separadamente por fase, sem exigir recursos de fases futuras:
  - **Fase 1 — ferramentas e acompanhamento:** ativar; delegar pelo Chat; observar tag/spinner/espera/término; receber aviso único renderizado no pai; cortar/restaurar a conexão do teste; reiniciar o cliente e verificar recuperação; desativar sem parar a sessão. Conferir que o clique identifica a sessão como remota e informa a indisponibilidade do terminal interno, sem abrir um worker local.
  - **Fase 2 — terminal remoto:** abrir a sessão no terminal interno, digitar e redimensionar; fechar sem kill; cortar/restaurar a conexão do teste e conferir reconexão sem replay local de teclas incertas. Verificar as regressões do acompanhamento e dos avisos da fase 1.
  - **Fase 3 — Cloud boxes:** na tela Devices, conferir Cloud boxes separada dos devices Zeron, métricas coerentes com a box, Ver sessões e Gerenciar conexão; desconectar/desativar e observar card preservado com amostra desatualizada, sem alterar o registro sincronizado. Verificar as regressões do acompanhamento, dos avisos e do terminal das fases anteriores.
  - Em todas as fases, registrar evidência visual e remota por identidade, usar somente sessões descartáveis próprias e manter a integração desligada por padrão. Suite verde não substitui a observação visual; concluir esta tarefa somente após registrar o aceite das três fases.
- [ ] 6.3 Revisar implementação, segurança e cobertura contra todos os cenários desta change, incluindo token, grant, outbox, ambiguidade de criação e ausência de efeitos no Unpeel. Verificar que achados materiais foram corrigidos e que somente recursos criados pela prova foram encerrados/removidos; nenhuma sessão do usuário, config de provider ou change concorrente foi modificada indevidamente.

## Workflow follow-up

- Após aprovação da entrega e todas as tarefas concluídas, arquivar via OpenSpec; não editar specs canônicas à mão.
- Sincronizar AGENTS.md afetados e executar cleanup de branch/worktree e recursos próprios conforme as skills, preservando o trabalho do usuário.

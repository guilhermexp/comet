## Fasing

| Fase | U-IDs | Seções | Depends on | Audit state | Audited commit | Entrega | UAT mode |
|---|---|---|---|---|---|---|---|
| F1 | U1 | §1 | — | pending | — | Saída local ordenada, persistida e terminal | artifact-driven |
| F2 | U3 | §2 | F1 | pending | — | Atividade real de compactação no Chat | live-runtime |
| F3 | U2 | §3 | F2 | pending | — | Fila pós-turno durável e integrada | live-runtime |

## Boundary Map

Serial ownership: F1 altera o driver/normalizador OMP; F2 estende seus eventos e projeta atividade na Session/UI; F3 consome essa atividade ao decidir elegibilidade. Nenhuma fase reimplementa as outras. Cada worker lê proposal.md, design.md e specs antes de editar; só marca as tasks de sua seção após prova. Apenas o orquestrador escreve Audit state/Audited commit após auditoria.

A identidade canônica é Chat para conversa durável, Session para execução e Chat Transcript para projeção sincronizada. Comandos e fila residem no ledger; UI não executa. D-01–D-08 da proposal valem em todas as fases. Preservar as correções concorrentes de steering/host-tool fence já presentes na main.

## 1. F1 — Resultados dos comandos locais

### must_haves

- M1.1: command_output chega como texto, em ordem, uma vez; local agentInvoked:false fecha sem agent_end.
- M1.2: request e eventos progridem mesmo com mais de 256 frames antes da resposta; falha/EOF/cancel não deixam Working eterno e preservam texto parcial.
- M1.3: prompt normal, streaming e steering existentes permanecem corretos; prazo longo local é finito e não reaproveita ACK curto.
- M1.4: resultado persiste no transcript e percorre o export existente, sem novo canal de notice nem turno artificial de modelo.
- M1.5: reprodução red-capable em green, smoke pelo adapter com OMP instalado e DOX atualizado. Captura nativa cumulativa entra no aceite final com F2/F3, não substitui este smoke de runtime.

- [ ] 1.1 Fixar regressão de output antes da resposta, rajada acima da capacidade e terminal local. files: `crates/harness/tests/omp_rpc.rs`, `crates/harness/tests/fixtures/fake-omp-rpc.sh`. verify: `cargo test -p zeron-harness --test omp_rpc local_command_output_is_preserved -- --exact --nocapture` — executar vermelho antes do patch e o mesmo comando verde depois; proibir zero testes como prova.
- [ ] 1.2 Consumir eventos durante a requisição inicial e normalizar command_output mantendo ordem, erro, EOF, cancelamento e prazos distintos. files: `crates/harness/src/omp/mod.rs`, `crates/harness/src/omp/normalize.rs`, `crates/harness/src/omp/process.rs`. verify: `cargo test -p zeron-harness --test omp_rpc`.
- [ ] 1.3 Provar persistência/reabertura do resultado com os caminhos existentes, sem refatorar engine/export. files: `crates/engine/tests/e2e.rs`, `crates/ui/src/chat_export.rs`. verify: `cargo test -p zeron-engine --test e2e` e `cargo test -p zeron-ui chat_export --lib`; só adicionar regressão quando uma perda plausível falhar nela.
- [ ] 1.4 Exercitar o adapter real contra OMP instalado com comando local sem modelo, registrar texto e terminalidade; atualizar contratos locais apenas onde mudaram. files: `crates/harness/AGENTS.md`. verify: smoke temporário através de OmpHarness, com comando literal e output no closeout; `cargo test -p zeron-harness`; `openspec validate complete-omp-chat-runtime-parity --strict`.

## 2. F2 — Compactação observável

### must_haves

- M2.1: eventos automáticos e estado manual real produzem atividade start/end, com outcomes corretos; /compact não é inferido pela string do input.
- M2.2: atividade é opcional, bounded e scoped ao run; abort/erro/EOF/stale limpam estado; willRetry não assenta turno.
- M2.3: tooltip/trailer mostram atividade sem progresso fictício; context usage continua last-known até medição real; idle recap respeita atividade.
- M2.4: resumo/contexto interno não entra no transcript nem no fanout público novo; payloads antigos continuam legíveis.
- M2.5: runtime automático/manual capturado, build e superfície nativa observados sem reiniciar o Comet atual.

- [ ] 2.1 Capturar o protocolo instalado e normalizar ciclo automático/manual com observação get_state limitada à operação. files: `crates/harness/src/omp/mod.rs`, `crates/harness/src/omp/normalize.rs`, `crates/harness/tests/omp_rpc.rs`, `crates/harness/tests/fixtures/fake-omp-rpc.sh`. verify: `cargo test -p zeron-harness --test omp_rpc`; smoke temporário instalado, registrado literalmente no closeout.
- [ ] 2.2 Adicionar tipos opcionais de atividade e projetá-los na Session antes do fanout, com guards de run e fechamento completo. files: `crates/proto/src/agent.rs`, `crates/proto/src/entities.rs`, `crates/engine/src/sessions.rs`, `crates/engine/tests/e2e.rs`. verify: `cargo test -p zeron-proto` e `cargo test -p zeron-engine --test e2e`; compilar todos os consumidores exhaustivos afetados.
- [ ] 2.3 Renderizar compactação no trailer/tooltip existentes e alimentar idle recap com atividade real. files: `crates/ui/src/state.rs`, `crates/ui/src/composer.rs`, `crates/ui/src/transcript.rs`, `crates/ui/src/details_sidebar/view.rs`, `crates/ui/src/details_sidebar/idle_recap.rs`, `crates/proto/src/view.rs`. verify: `cargo test -p zeron-ui --lib` e `cargo test -p zeron-proto`; native isolated capture conforme AGENTS.md, com screenshot de atividade e término.
- [ ] 2.4 Completar contratos locais e prova de integração F1+F2. files: `crates/harness/AGENTS.md`, `crates/engine/AGENTS.md`, `crates/ui/AGENTS.md`, `crates/proto/AGENTS.md`. verify: `cargo test -p zeron-harness` e `cargo build` e `openspec validate complete-omp-chat-runtime-parity --strict`; confirmar /context e /compact no uso nativo sem duplicação de output.

## 3. F3 — Fila durável pós-turno

### must_haves

- M3.1: FollowUp é intent durável distinto; Enter segue steering e submissão incompatível não faz fallback para Run/Steer.
- M3.2: FIFO de múltiplas mensagens; somente após turn settlement real, sem AwaitingInput/compactação/driver teardown; configuração/anexos/resume preservados.
- M3.3: Interrupt/RespondInput elegíveis ultrapassam FollowUp bloqueado, inclusive por anexos; Stop invalida cadeia anterior sem ordenar por relógio entre devices.
- M3.4: erro/crash/entrega incerta não reexecutam cadeia; texto e motivo recuperáveis; TTL/falha de upload chegam a estado terminal.
- M3.5: projeção da fila vem do doc, navegação/restart não perdem entries, cancelamento respeita autor+pending e message_id não duplica eco.
- M3.6: invariantes de isolamento/ownership e consumidores antigos continuam válidos; uso nativo com steering, fila, compactação e Stop observado.

- [ ] 3.1 Introduzir FollowUp reutilizando payload/configuração/identidade e comandos existentes, sem renomear containers. files: `crates/doc/src/commands.rs`, `crates/doc/src/schema.rs`, `crates/rpc/src/method.rs`. verify: `cargo test -p zeron-doc` e `cargo test -p zeron-rpc`.
- [ ] 3.2 Implementar seleção/eligibilidade FIFO e re-disparo após assentamento, com prioridade dos controles e attachments íntegros. files: `crates/engine/src/doc_host.rs`, `crates/engine/src/sessions.rs`, `crates/engine/src/rpc.rs`, `crates/engine/tests/run_controls_chat_id.rs`, `crates/engine/tests/queued_attachments.rs`, `crates/engine/tests/turn_quiesce.rs`. verify: `cargo test -p zeron-engine --test run_controls_chat_id --test queued_attachments --test turn_quiesce`.
- [ ] 3.3 Fechar Stop/erro/crash/claim incerto, TTL, deduplicação e compatibilidade com host antigo, sem replay automático. files: `crates/engine/src/doc_host.rs`, `crates/engine/src/sessions.rs`, `crates/doc/src/commands.rs`, `crates/engine/tests/restart_resume.rs`, `crates/engine/tests/run_controls_chat_id.rs`. verify: `cargo test -p zeron-engine --test restart_resume --test run_controls_chat_id` e `cargo test -p zeron-doc`.
- [ ] 3.4 Adicionar ação explícita, projeção/cancelamento da fila e recuperação do input sem mudar Enter/Shift+Enter. files: `crates/ui/src/composer.rs`, `crates/ui/src/state.rs`, `crates/ui/src/shell.rs`. verify: `cargo test -p zeron-ui --lib`; smoke nativo enfileirando B/C durante A, navegando entre Chats, cancelando B e acionando Stop com fila pendente.
- [ ] 3.5 Completar DOX e integração das três fases com build e cenários nativos. files: `crates/doc/AGENTS.md`, `crates/engine/AGENTS.md`, `crates/ui/AGENTS.md`, `crates/rpc/AGENTS.md`. verify: `cargo test -p zeron-engine` e `cargo test -p zeron-ui --lib` e `cargo build` e `openspec validate complete-omp-chat-runtime-parity --strict`; capturar output/screenshots reais sem usar o perfil da instância atual.

## Acceptance ownership

Cada fase exige closeout durável, transporte verificado, doctor sem errors e auditoria de must_haves/cenários/testes antes de ACCEPT. Nomes de testes novos são definidos pelo worker exceto o sinal red-capable explícito de 1.1. Nenhum teste vazio, de texto-fonte ou apenas de wiring conta como prova; adicionar somente cenários de perda/corrida/compatibilidade plausíveis.

O orquestrador faz o review final Standards × Spec sobre todo o diff desde o baseline pré-F1 e valida a superfície acumulada. Reality não comprovada bloqueia archive. Standards & Security de publicação ficam not yet due enquanto não houver push/merge/release pedido. Gate nativo indisponível deve nomear o missing input; não marcar task visual como comprovada com cargo test.

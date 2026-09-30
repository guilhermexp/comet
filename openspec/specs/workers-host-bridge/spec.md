# workers-host-bridge Specification

## Purpose

Host bridge transport deadlines, concurrent and cancellable worker controller dispatch, and orchestrator-owned wait ceilings.

## Requirements

### Requirement: Workers host tool transport deadline exceeds tool blocking budget

The host bridge transport deadline for a Workers tool call SHALL be derived per call: for `wait_for_status` it SHALL be the requested `timeout_seconds` plus a margin of at least 60 seconds for JSON-RPC IPC round-trip, serialization and host scheduling; for every other action it SHALL be `TOOL_CALL_TIMEOUT` (900s). The margin therefore holds for any wait the orchestrator requests, up to the controller ceiling.

#### Scenario: Tool call timeout exceeds wait_for_status maximum ceiling
Test: harness integration test deriving the maximum wait duration from the controller MCP tool schema and asserting the transport timeout exceeds it with a round-trip margin.

- **WHEN** the Workers controller MCP schema advertises a maximum blocking wait (`timeout_seconds.maximum`)
- **THEN** the bridge transport deadline derived for `wait_for_status` at that maximum exceeds it by at least 60 seconds

#### Scenario: Wait deadline follows the requested timeout
Test: harness unit test on `call_timeout_for` deriving the deadline from `wait_for_status` arguments at the controller ceiling.

- **WHEN** the orchestrator calls `wait_for_status` with `timeout_seconds` equal to the controller ceiling
- **THEN** the bridge deadline exceeds that value by at least 60 seconds

#### Scenario: Other actions keep the fixed transport deadline
Test: harness unit test on `call_timeout_for` with a non-wait action.

- **WHEN** the tool call is any action other than `wait_for_status`
- **THEN** the bridge deadline is `TOOL_CALL_TIMEOUT`

### Requirement: Single constant for Workers status wait ceiling and clear timeout semantics

The Workers controller MCP tool schema, `action=help` limits, and the runtime clamp for `wait_for_status` SHALL derive their maximum blocking wait from a single public constant (`WAIT_FOR_STATUS_MAX_TIMEOUT_SECONDS` = 14400, four hours) in `zeron-workers-unpeel`; the orchestrator chooses any `timeout_seconds` up to it and the default remains 30 seconds. The controller SHALL dispatch requests concurrently so a pending wait never blocks `stop_worker`, `archive_worker` or `ping`, SHALL honour `notifications/cancelled` by interrupting the pending wait without sending a response, and SHALL cancel pending waits when its input closes. A `timed_out: true` result SHALL carry a `next` field stating that the caller may wait again with a timeout sized to the work or end the turn and receive `[worker-task-notification]`.

#### Scenario: Schema, help limits, and runtime clamp derive from single constant
Test: controller MCP integration test asserting schema `maximum`, `help` limits, and runtime clamp match `WAIT_FOR_STATUS_MAX_TIMEOUT_SECONDS`.

- **WHEN** the controller MCP tool schema is inspected
- **THEN** `timeout_seconds.maximum` equals `WAIT_FOR_STATUS_MAX_TIMEOUT_SECONDS` (14400)
- **AND** `action=help` reports `limits.wait_seconds` equal to `WAIT_FOR_STATUS_MAX_TIMEOUT_SECONDS`

#### Scenario: Pending wait does not block the control channel
Test: controller MCP integration test driving `serve` with a blocking request followed by `ping` and a cancellation.

- **GIVEN** a request is pending in `serve`
- **WHEN** a `ping` arrives on the same channel
- **THEN** the `ping` is answered before the pending request completes

#### Scenario: Cancellation interrupts a pending wait without a response
Test: controller MCP integration tests on `serve` and on `wait_until` with a cancel flag.

- **WHEN** `notifications/cancelled` names a pending request id
- **THEN** the pending wait stops within one poll tick and no response is written for that id

#### Scenario: Timeout result carries next-step guidance
Test: controller MCP unit test on `wait_until` expiring against a running worker.

- **WHEN** a `wait_for_status` expires with the worker still running
- **THEN** the result has `timed_out: true`, the worker snapshot and a `next` string naming `[worker-task-notification]` and the option to wait again

### Requirement: Native orchestrator runtimes receive a matching MCP client deadline

When the Workers controller MCP is mounted into a native orchestrator runtime, the harness SHALL configure that runtime's MCP tool-call deadline to `WORKERS_CLIENT_DEADLINE_SECONDS` = `WAIT_FOR_STATUS_MAX_TIMEOUT_SECONDS + 60`: Claude via the `MCP_TOOL_TIMEOUT` environment variable (milliseconds) and Codex via the `mcp_servers.comet-workers.tool_timeout_sec` override. The harness constant SHALL be pinned to the controller constant by test.

#### Scenario: Claude process deadline
Test: harness unit test on `build_command` with Workers MCP enabled.

- **WHEN** the Claude harness builds a command with Workers MCP enabled
- **THEN** the process environment carries `MCP_TOOL_TIMEOUT` equal to `WORKERS_CLIENT_DEADLINE_SECONDS * 1000`

#### Scenario: Codex override deadline
Test: harness unit test on Codex overrides with Workers MCP enabled.

- **WHEN** the Codex harness mounts the Workers MCP
- **THEN** the overrides contain `mcp_servers.comet-workers.tool_timeout_sec=<WORKERS_CLIENT_DEADLINE_SECONDS>`

### Requirement: wait_for_status status is a validated, documented contract

The Workers controller SHALL accept for `wait_for_status` only `completed` and the lifecycle values that `list_workers`/`inspect_worker` report for `state` and `activity`, compared case-insensitively. Any other value SHALL be rejected with an error that lists the accepted values, without waiting. `completed` on a worker whose task episodes are not tracked (no parent chat binding, so completion can never be observed) SHALL be rejected immediately with an error saying so, instead of blocking until the timeout. The tool schema, the tool description and `action=help` SHALL name `completed` as the status for "the worker finished its task", and SHALL state that `idle` matches any pause (including a worker waiting on its own subagents) and `exited` matches only a dead process.

#### Scenario: Unknown status is rejected without waiting
- Test: integration — controller MCP integration test calling `wait_for_status` with an unknown status against a live worker and asserting an immediate error listing the accepted values.

- **GIVEN** a live worker
- **WHEN** the orchestrator calls `wait_for_status` with `status` set to a value outside the accepted set
- **THEN** the call returns an error naming the accepted values
- **AND** it returns without blocking for `timeout_seconds`

#### Scenario: Schema and help document completed as the finish target
- Test: integration — controller MCP integration test inspecting the tool schema `status` description, the tool description and `action=help`.

- **WHEN** the tool schema, tool description and `action=help` are read
- **THEN** each names `completed` as the status for a finished task
- **AND** each states that `idle` and `exited` do not mean the task finished

#### Scenario: Completed on an untracked worker is rejected without waiting
- Test: integration — controller MCP integration test launching a worker from a controller without `COMET_WORKERS_PARENT_CHAT_ID` and calling `wait_for_status(completed)`.

- **GIVEN** a live worker launched without a parent chat binding
- **WHEN** the orchestrator calls `wait_for_status` with `status` `completed`
- **THEN** the call returns an error stating that completion is not tracked for this worker
- **AND** it returns without blocking for `timeout_seconds`

#### Scenario: Unreadable binding state is a read error, not an untracked worker
- Test: unit — `parent_notifications` test with a malformed binding state asserting the binding lookup returns an error.

- **GIVEN** an app state whose parent-binding section cannot be read
- **WHEN** the orchestrator calls `wait_for_status` with `status` `completed`
- **THEN** the call fails with the read error
- **AND** it does not claim that completion is not tracked for the worker

### Requirement: A restarted worker keeps its parent chat binding

When `restart_worker` replaces a worker's Session with a new session id, the controller SHALL register the replacement under the same parent chat as the original, with a fresh task-episode history, before returning the new id. A source without a binding, or a replacement that is already bound, SHALL be left unchanged. If the binding cannot be carried, the call SHALL fail naming the new session id so the caller can still address the restarted worker.

#### Scenario: Replacement session inherits the parent chat
- Test: unit — `parent_notifications` test carrying a binding from an old to a new session id.

- **GIVEN** a worker bound to a parent chat whose Session is replaced by `restart_worker`
- **WHEN** the replacement session id is known
- **THEN** the replacement is bound to the same parent chat
- **AND** its first tracked task starts at episode 1

#### Scenario: Unbound source and bound target are left alone
- Test: unit — `parent_notifications` test carrying from an unbound source and onto an already-bound target.

- **GIVEN** a source session without a binding, or a target session that already has one
- **WHEN** the binding is carried
- **THEN** no binding is created or overwritten

### Requirement: A lifecycle wait ends when the current episode completes

A `wait_for_status` on any accepted status other than `completed` SHALL also return, within one poll tick of completion becoming observable, when the worker's current task episode transitions to completed during the wait. The result SHALL say the episode completed and SHALL NOT claim the requested status matched. A completion that already held when the wait started SHALL NOT end a wait on another status.

#### Scenario: Waiting on exited returns when a live worker finishes
- Test: integration — controller MCP integration test with a live idle worker whose current episode gains Stop evidence during a `wait_for_status(exited)`.

- **GIVEN** a live worker with an active task episode and a pending `wait_for_status` on `exited`
- **WHEN** the current episode completes (Stop evidence and quiescent output) while the process stays alive
- **THEN** the wait returns within one poll tick with a result that marks the episode as completed
- **AND** the result does not report `exited` as matched

#### Scenario: Prior completion does not end a new lifecycle wait
- Test: integration — controller MCP integration test starting `wait_for_status(working)` on a worker whose current episode was already completed before the call.

- **GIVEN** a worker whose current episode completed before the call
- **WHEN** the orchestrator waits on `working` with a short timeout
- **THEN** the wait does not return early because of that prior completion

### Requirement: A worker notification ends the parent's pending wait

When a `[worker-task-notification]` is queued into a parent chat whose OMP turn has a pending Workers `wait_for_status` host tool call, the harness SHALL end that wait promptly instead of holding the notification until the wait times out. The wait's tool result SHALL be delivered first and SHALL state that it was interrupted by a worker notification (not a transport failure). The notification prompt SHALL then be processed exactly once. Steers that are not worker notifications, and pending host tools other than `wait_for_status`, SHALL keep the existing queue-until-result behaviour.

#### Scenario: Notification during a pending wait is delivered without waiting for the timeout
- Test: integration — harness integration test in `omp_rpc` with the fake OMP and fake Workers controller: a long `wait_for_status` is pending when a worker-notification steer arrives.

- **GIVEN** an OMP turn with a pending `wait_for_status` whose timeout is far in the future
- **WHEN** a `[worker-task-notification]` steer is queued for that chat
- **THEN** the wait's tool result, marked as interrupted by a worker notification, is delivered before the steer
- **AND** the steer prompt is delivered once, well before the wait's timeout

#### Scenario: Ordinary steer keeps queue-until-result
- Test: integration — harness integration test in `omp_rpc` sending a non-notification steer during a pending `wait_for_status`.

- **GIVEN** an OMP turn with a pending `wait_for_status`
- **WHEN** a steer that is not a worker notification arrives
- **THEN** the wait is not interrupted and the steer is delivered after the wait's own result

#### Scenario: A natural result racing the notification is preserved
- Test: integration — harness integration test in `omp_rpc` where the controller answers the pending `wait_for_status` (matched) at the same time a worker-notification steer arrives.

- **GIVEN** an OMP turn with a pending `wait_for_status` whose controller result arrives together with a worker-notification steer
- **WHEN** the harness delivers the tool result
- **THEN** the controller's real result is delivered, not replaced by the interrupted marker
- **AND** the steer is delivered once after it

### Requirement: Claude Workers expose no suggested prompt in their composer

Claude Code Workers SHALL be launched with prompt suggestions disabled (`CLAUDE_CODE_ENABLE_PROMPT_SUGGESTION=false` or an equivalent documented Claude Code control), so the idle composer captured in `read_output`, `inspect_worker` and the notification output tail carries no suggested next prompt. Permission dialogs and other real input requests SHALL remain in the captured output.

#### Scenario: Claude Worker launch disables prompt suggestions
- Test: unit — unit test on the Claude Worker launch command/environment asserting prompt suggestions are disabled.

- **WHEN** a Claude Code Worker launch command and environment are built
- **THEN** they disable Claude Code prompt suggestions

#### Scenario: Blocked Claude Worker still shows its permission prompt
- Test: unit — parent-notification or controller output test with a captured Claude permission dialog viewport, asserting the dialog text is present in the output tail.

- **GIVEN** a Claude Worker viewport showing a permission dialog
- **WHEN** the notification output tail is built
- **THEN** the dialog text is present in the tail

### Requirement: Worker bloqueado acorda o Orquestrador

O sistema SHALL emitir uma notificação ao chat pai quando um Worker com tarefa
registrada passa a bloqueado esperando input, sem esperar o fim do turno. A
notificação MUST nomear o Worker e MUST ser emitida uma única vez por episódio
de bloqueio, de modo que responder e voltar a bloquear notifica de novo.

#### Scenario: Bloqueio emite notificação ao pai
- Test: unit — motor de notificações com uma sessão que transiciona para
bloqueado.

- **WHEN** um Worker com tarefa registrada passa a bloqueado esperando input
- **THEN** uma notificação de espera por input é emitida para o chat pai
- **AND** a notificação identifica o Worker e o projeto

#### Scenario: Bloqueio contínuo não repete a notificação
- Test: unit — motor de notificações em passadas sucessivas sobre o mesmo estado.

- **WHEN** o Worker permanece bloqueado por várias reconciliações
- **THEN** apenas a primeira passada emite notificação
- **AND** responder e bloquear de novo emite uma nova notificação

### Requirement: A notificação ao pai preserva o conteúdo reportado

O sistema MUST preservar o texto visível da cauda de output ao montar a
notificação ao pai. Uma linha terminada em retorno de carro — o desenho normal
de um terminal que repinta — MUST manter o último conteúdo pintado em vez de
resultar em bloco vazio. A notificação SHALL declarar ausência de conteúdo
apenas quando não houver texto visível a reportar.

#### Scenario: Linha terminada em retorno de carro sobrevive
- Test: unit — montagem do prompt de notificação a partir de cauda com repaint.

- **WHEN** a cauda de output termina em retorno de carro após o último texto
  pintado
- **THEN** o bloco de output da notificação contém esse texto
- **AND** a notificação não declara ausência de conteúdo

#### Scenario: Repaint sucessivo reporta o último estado
- Test: unit — montagem do prompt com múltiplos repaints na mesma linha.

- **WHEN** a mesma linha é repintada várias vezes na cauda de output
- **THEN** o bloco de output contém o último conteúdo pintado
- **AND** não contém as versões anteriores empilhadas

#### Scenario: Ausência real de conteúdo continua sinalizada
- Test: unit — montagem do prompt com cauda sem texto visível.

- **WHEN** a cauda de output não tem nenhum texto visível
- **THEN** a notificação declara ausência de conteúdo

### Requirement: Não lido tem fonte de verdade local

O sistema SHALL derivar o indicador de não lido de uma sessão do estado de
notificação ao pai que o próprio aplicativo mantém, e NOT SHALL depender de um
arquivo de estado que apenas o aplicativo nativo do upstream escreve. Uma
sessão com notificação pendente de reconhecimento MUST reportar não lido.

#### Scenario: Notificação pendente marca a sessão como não lida
- Test: unit — projeção de sessão com notificação registrada e não reconhecida.

- **WHEN** uma sessão tem notificação ao pai emitida e ainda não reconhecida
- **THEN** a sessão reporta não lido para o painel e para o Orquestrador

#### Scenario: Reconhecimento limpa o não lido
- Test: unit — projeção de sessão após reconhecimento da notificação.

- **WHEN** a notificação ao pai daquela sessão é reconhecida
- **THEN** a sessão deixa de reportar não lido

### Requirement: Transcript do runtime da família pi é legível pelo controller

O sistema SHALL expor o transcript de um Worker de runtime da família pi cujo
diretório de sessão é gerenciado, de modo que o Orquestrador possa ler o
conteúdo estruturado da conversa em vez de depender da cauda crua do terminal.

#### Scenario: Runtime declara transcript e resolve adaptador
- Test: unit — resolução de provedor de transcript a partir do catálogo pinado.

- **WHEN** o catálogo é consultado para um runtime da família pi com diretório
  de sessão gerenciado
- **THEN** um adaptador de transcript é resolvido para ele

#### Scenario: Leitura de transcript devolve a conversa
- Test: integration — controller MCP lendo o transcript de uma sessão gerenciada.

- **WHEN** o Orquestrador pede o transcript de uma sessão desse runtime
- **THEN** a resposta contém as entradas da conversa
- **AND** a leitura não é recusada por runtime sem suporte

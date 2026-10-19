# Spec Delta

## REMOVED Requirements

### Requirement: A worker notification ends the parent's pending wait

**Reason:** The notification-only contract deliberately holds ordinary user messages until the wait finishes, contrary to the requested incoming-message behavior.

**Migration:** Use the replacement incoming-message contract below. Both ordinary messages and Worker notices release a pending status wait.

## ADDED Requirements

### Requirement: Incoming messages release pending Workers waits

An accepted incoming message SHALL release the Orchestrator's pending Workers `wait_for_status` before its timeout. This applies to user messages and Worker notices through the common controller used by native runtimes and the OMP bridge. The message SHALL be consumed exactly once. Releasing the wait SHALL NOT stop, restart or complete the Worker or abort unrelated tools. A natural wait result already available SHALL be preserved.

#### Scenario: Notification during a pending wait is delivered without waiting for the timeout
- Test: integration — harness subprocess tests for an incoming Worker notification during a pending wait.

- **GIVEN** an Orchestrator with a pending `wait_for_status` whose timeout is far in the future
- **WHEN** a `[worker-task-notification]` message is accepted for that Chat
- **THEN** the wait is released and the notification is consumed once, before the wait timeout
- **AND** the Worker continues under its own lifecycle

#### Scenario: Ordinary message interrupts a pending wait
- Test: integration — engine pending-message state over RPC, controller wait release and OMP subprocess delivery; native UI remains a separate smoke observation.

- **GIVEN** an Orchestrator waiting only on a pending Workers `wait_for_status`
- **WHEN** an ordinary user message is accepted for that Chat
- **THEN** the wait is released and the message is processed once without waiting for the timeout
- **AND** no Worker stop, restart or completion is requested

#### Scenario: Unrelated tools are not interrupted
- Test: integration — harness subprocess tests with an unrelated open tool and an incoming message.

- **GIVEN** an unrelated tool is still running alongside or instead of the Workers wait
- **WHEN** an ordinary message arrives
- **THEN** the unrelated tool is allowed to finish under the existing steering policy
- **AND** its result is not replaced by a wait-interruption result

#### Scenario: A natural result racing the notification is preserved
- Test: integration — `omp_rpc` subprocess fixture in which the controller answers the pending wait together with an incoming message.

- **GIVEN** an OMP turn with a pending `wait_for_status` whose controller result arrives together with an incoming message
- **WHEN** the harness delivers the tool result
- **THEN** the controller's real result is delivered, not replaced by the interrupted marker
- **AND** the message is delivered once after it

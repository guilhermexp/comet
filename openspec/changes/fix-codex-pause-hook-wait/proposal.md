# Proposal

## Why

Interromper um Codex Worker deixa o spinner de Workers ligado mesmo com "Conversation interrupted" e o prompt livre, e a tela de revisão de hooks (`t trust all · enter review · esc close`) não vira atenção. `wait_for_status(completed|exited)` só solta no timeout, então o Orquestrador não consegue agir enquanto o worker espera input.

## What Changes

- Reconhecer o footer interativo de revisão de hooks como menu que bloqueia, sem tratar aviso isolado, prosa ou lista passiva como bloqueio.
- Derrubar o spinner quando a interrupção do turno atual está visível e o CLI segue vivo, sem converter isso em conclusão e sem deixar transcript antigo derrubar um turno novo.
- Fazer `wait_for_status` de `completed` ou `exited` retornar na hora com needs-input quando a atividade é `blocked`, sem declarar matched/completed e sem aprovar hooks. Esperar `blocked` continua casando.
- Propagar os dois sinais pela mesma máquina de atividade para a UI de Workers e para o controller.

## Capabilities

### New Capabilities

- Nenhuma.

### Modified Capabilities

- `workers-host-bridge`: espera bloqueada por input real termina com needs-input; interrupção observável do turno atual tira o spinner sem concluir a tarefa; footer de revisão de hooks entra no mesmo contrato de menu que já bloqueia.

## Impact

- `third_party/unpeel` (detector de viewport, scan do host, máquina compartilhada de atividade, espelho iOS do Unpeel) e `crates/workers-unpeel` (derivação e `wait_for_status`).
- A UI de Workers e o controller consomem `activity`; não há feature nova no app iOS do Comet.
- Sem aprovação automática de hooks, sem push e sem troca de binário global.

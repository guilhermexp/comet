## Context

Ver proposal.md — Why. Estado atual relevante:

- `WorkersResourceSettings` já persiste e valida `hibernation_enabled`
  (default false), `hibernate_after_idle_minutes` (default 15, clamp
  1..10080) e `max_live_idle_workers` (default 12, clamp 1..256). Nenhum
  consumidor.
- Cada `WorkersSession` do bootstrap traz `state` (`running`/`exited`),
  `activity` (`working`/`blocked`/`idle`, derivada pelo `activity_bridge` do
  último hook), `updated_at_unix_ms`, `pinned`, `archived`,
  `active_runtime_id`, `runtime_launch_pending` e `capabilities.restart`.
- `updated_at_unix_ms` NÃO serve como clock de ociosidade: é o máximo entre
  `updated_at` do manifest (avança a cada heartbeat de 60 s do host), mtime
  de `output.bin` (avança a cada repaint da TUI) e mtime do journal. Medido
  num `omp` parado há 24 h: `updated_at` e `output.bin` com idade 0 h, enquanto
  `screen_changed_at` do manifest (hash do texto renderizado, imune a repaint
  idêntico) marcava 24,2 h — coerente com o último hook `Stop` 23,7 h antes.
- O painel roda um ciclo de refresh (`WorkersModel::refresh`) que busca o
  bootstrap completo e já aplica efeitos colaterais (notificações de parent).
- `WorkersSessionCommand::Archive` já faz Stop quando a sessão está viva e
  marca `archived`; `SessionAction::Restart` já relança com resume via
  `relaunch_command_and_pending_context`.
- Session hosts são processos desanexados: sobrevivem ao app fechado. Sem o
  app aberto ninguém hiberna, e isso é aceitável — ao abrir, o primeiro
  refresh põe tudo em dia.
- O controller MCP expõe `stop_worker` e `archive_worker`, mas não restart;
  `send_text` hoje não checa se o Worker está vivo.

## Goals / Non-Goals

**Goals:**
- Política de hibernação pura e testável sem UI, alimentada pelo snapshot
  que o refresh já busca.
- Reuso total dos verbos existentes (Archive, Restart); nenhum verbo novo no
  Host.

**Non-Goals:**
- Reaper fora do app (daemon, launchd). Se um dia fizer falta, a política
  pura pode ser chamada por `zeron workers` na CLI.
- Hibernar por pressão de memória. `MemoryPressureReducer` continua só
  trimando caches; integrar os dois é uma change futura.
- Remover sessões. Hibernar é archive; remove continua manual.

## Decisions

1. **Política pura em `zeron-workers-unpeel`, disparo em `WorkersModel::refresh`.**
   Uma função `hibernation_candidates(sessions, settings, selected, now) ->
   Vec<session_id>` no frontier, sem I/O, testada por unidade. A UI só a
   chama após cada bootstrap e emite `Archive` para cada id. Alternativa:
   um timer próprio na UI — descartada, o refresh já tem a cadência certa e
   o snapshot fresco. Alternativa: dentro do `activity_bridge` — descartada,
   o bridge é máquina de estado compartilhada com o vendorizado e não deve
   ganhar política do Comet.
2. **Clock de ociosidade = `idle_since_unix_ms`, campo novo e opcional no
   `WorkersSession`, preenchido pelo `activity_bridge` com o máximo entre
   `screen_changed_at` do manifest e o `received_at` do último hook.** É o
   instante da última mudança real de conteúdo ou de estado do agente; um
   Worker parado no prompt não o avança. Alternativa `updated_at_unix_ms` —
   descartada com medição (ver Context): nunca ficaria ocioso. Alternativa
   mtime de `last-hook-event.json` — descartada: cobre só runtimes com hooks,
   e `pi` (lifecycle por output) ficaria sem clock. O campo é local ao
   frontier in-process, sem wire novo para o Host; `None` significa "sem
   evidência" e a política trata como não elegível.
3. **Terminal = `active_runtime_id` ausente e sem capability `restart`.**
   Não existe flag de "sessão de terminal" no `WorkersSession`; a combinação
   acima é o que a UI já usa para esconder Restart. Em caso de dúvida a
   política protege (não hiberna), nunca o contrário.
4. **Teto aplica só a elegíveis e só do mais antigo para o mais novo.**
   Evita hibernar um Worker que acabou de ficar idle porque outro, pinado,
   ocupa vaga. Pinados e protegidos não contam para o teto.
5. **`restart_worker` no controller MCP + guard em `send_text`/`send_keys`.**
   Sem isso a hibernação quebra a delegação: o Orquestrador escreveria num
   PTY morto. O guard devolve erro nomeando `restart_worker`; a resposta do
   restart traz o novo `session_id` porque relançar substitui a sessão.
6. **Default continua desligado.** Ligar é decisão do usuário nas settings;
   a política nunca roda sem `hibernation_enabled`.
7. **Um archive por sessão por ciclo, idempotente.** Se o Archive falhar
   (host já saiu, lock), o próximo refresh tenta de novo; nenhum estado local
   de "já tentei" para não divergir do disco.

## Risks / Trade-offs

- [Hibernar um Worker que o Orquestrador ia usar em seguida] → prazo
  configurável e restart barato; o custo é o tempo de relançamento, não
  contexto perdido.
- [`activity` presa em `idle` por hook perdido enquanto o CLI trabalha] →
  `idle_since_unix_ms` também avança por mudança real de tela
  (`screen_changed_at`); um Worker produzindo output não fica ocioso pelo
  clock. Ainda assim `working` continua tendo precedência.
- [`screen_changed_at` ausente em manifests antigos] → `idle_since_unix_ms`
  fica `None` e o Worker não é elegível até o host publicar o campo; a
  política protege na dúvida.
- [Runtime com resume declarado mas quebrado na prática] → o Worker fica
  arquivado com a conversa no disco; o usuário vê "arquivado" e pode
  reiniciar ou inspecionar transcript. Nada é removido.
- [Refresh falha por ciclos e nada hiberna] → aceito; hibernação é
  best-effort, a garantia é "eventualmente, com o app aberto".
- [Múltiplas janelas/instâncias do app disparando Archive no mesmo Worker] →
  `stop_session` já é serializado pelo lifecycle lock por sessão e um
  segundo Archive num Worker já `exited` é no-op.

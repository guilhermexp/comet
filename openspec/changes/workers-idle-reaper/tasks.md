## 1. Clock de ociosidade e política pura

- [x] 1.0 Adicionar `idle_since_unix_ms: Option<u64>` ao `WorkersSession`,
  preenchido no `activity_bridge` com o máximo entre `screen_changed_at` do
  manifest e o `received_at` do último hook; teste RED em `activity_bridge`
  provando que heartbeat e repaint idêntico não o avançam.
- [x] 1.1 Adicionar testes RED em `crates/workers-unpeel/src/lib.rs` (mod tests)
  para `hibernation_candidates`: desligado, idle além/dentro do prazo,
  `working`, `blocked`, pinado, selecionado, terminal, lançamento pendente,
  sem capability `restart`, teto com ordenação do mais antigo.
- [x] 1.2 Implementar `hibernation_candidates(sessions, settings, selected_session_id, now_unix_ms)`
  em `zeron-workers-unpeel`, sem I/O, até GREEN.

## 2. Disparo no painel

- [x] 2.1 Chamar a política em `WorkersModel::refresh` após o bootstrap e
  emitir `WorkersSessionCommand::Archive` para cada candidato, com log
  `info` por Worker hibernado e sem estado local de retentativa.
- [x] 2.2 Expor toggle, minutos e teto na seção Resources de
  `crates/ui/src/workers/settings.rs`, persistindo via o snapshot existente.
- [ ] 2.3 (pendente de validação manual) Validar no app real (`cargo run`): ligar hibernação com prazo curto,
  observar Worker idle ir para arquivado e Restart retomar a conversa.

## 3. Controller MCP

- [x] 3.1 Adicionar testes RED em `crates/workers-unpeel/tests/controller_mcp.rs`:
  `send_text`/`send_keys` em Worker não-`running` falham nomeando
  `restart_worker`; `restart_worker` relança e devolve o novo `session_id`.
- [x] 3.2 Implementar `restart_worker` e o guard em `send_text`/`send_keys`
  em `controller_mcp.rs`; atualizar o `help` das tools.

## 3b. Portões de evidência (review pré-merge)

- [x] 3b.1 Expor `hook_confirmed_idle` na máquina de estados vendorizada
  (`third_party/unpeel/crates/unpeel-tui/src/activity.rs`): `Stop`
  confirma, o sweep de `HOOK_IDLE_TIMEOUT` não; teste com sweep e com Stop.
- [x] 3b.2 Adicionar `idle_confirmed_by_hook` e `resumable_conversation` ao
  `WorkersSession`, preenchidos pelo `activity_bridge` (o segundo por
  `unpeel_core::resume::resumed` sobre o marker de provider e o comando);
  testes de sonda com marker, com `--session-dir` e com terminal.
- [x] 3b.3 Exigir as duas evidências em `hibernation_candidates`; regressões
  para idle varrido e para conversa não retomável.
- [x] 3b.4 Adicionar `confirmed_hibernation_candidates` (segunda passada por
  interseção) e passar o `hibernate_idle_workers` da UI a rebuscar o
  bootstrap dentro da task antes de arquivar; regressões de Worker que
  voltou a trabalhar e de não-ampliação da primeira decisão.

## 4. Closeout

- [x] 4.1 DOX pass: `crates/ui/AGENTS.md` (workers: política de hibernação e
  onde dispara) e `crates/workers-unpeel/AGENTS.md` (política pura, tools
  novas do controller MCP, Test Coverage Matrix).
- [x] 4.2 Atualizar a skill `delegate` do Orquestrador (`~/.orchestrator`)
  para chamar `restart_worker` antes de `send_text` em Worker arquivado.
- [x] 4.3 `cargo fmt --all`, `cargo test -p zeron-workers-unpeel -p zeron-ui`.

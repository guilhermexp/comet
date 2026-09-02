## Context

Ver proposal.md — Why. Estado atual relevante:

- `resume::resumed(command, provider_session_id)` é THE one implementation do
  relançamento (TUI, CLI e app passam por `relaunch_command_and_pending_context`
  em `session_ops.rs`). Sem adapter registrado devolve o comando intacto.
- `runtimes/pi/adapter/resume.rs` já implementa exatamente o desenho
  desejado: strip de flags, `--session <id>` quando há id, `--continue` quando
  há `--session-dir` pinado, e `prepare_new_launch` que pina
  `<unpeel_home>/pi-sessions/<session_id>`.
- `runtimes/_shared/pi-family/adapter/mod.rs` é incluído por `omp` e
  `prime-agent` via `include!` e registra lifecycle extension e telemetria,
  mas não `with_resume_adapter`.
- O `omp` aceita `-c/--continue`, `-r/--resume=<id|path>`, `--session-dir`,
  `--no-session`. **Não** aceita `--session <id>` nem `--fork`, que o `pi`
  aceita. `prime-agent` segue o `omp`.
- A extensão de lifecycle já publica `session_id` do provider; o bridge grava
  `provider-session.json` (marker) e `relaunch_command_and_pending_context`
  prefere o marker ao manifest. Ou seja, o id existe, só o adapter falta.
- A validação do catálogo (`runtime_catalog_schema.rs`) rejeita capability
  "adapter-owned" sem adapter compilado; `resume`/`restart_agent` são desse
  grupo.

## Goals / Non-Goals

**Goals:**
- Uma receita de resume para a família pi, vivendo no adapter compartilhado,
  para que `omp` e `prime-agent` não divirjam.
- Zero código de resume novo além da diferença de flags entre `pi` e `omp`.

**Non-Goals:**
- Fork nativo (`--fork`) para a família pi: o `omp` não expõe o flag.
- Migrar sessões existentes para diretório pinado.
- Qualquer política de quando parar um Worker (é a change
  `workers-idle-reaper`).

## Decisions

1. **Adapter próprio da família pi, derivado do `pi`, em vez de incluir o
   `resume.rs` do `pi` por `include!`.** O `pi` resume por `--session <id>`;
   o `omp` só aceita `--resume <id>`. Um `include!` cego geraria um comando
   inválido. A alternativa de parametrizar o adapter do `pi` por flag foi
   descartada: adiciona indireção num arquivo que hoje é simples, e a
   família pi já tem seu próprio `adapter/` compartilhado — o lugar natural.
   O arquivo novo copia a estrutura do `pi` com `ID_FLAGS`/`RESUME_FLAGS`
   ajustados (`-r`, `--resume`, `-c`, `--continue`) e sem `--fork`/`--session`
   em `PIN_FLAGS`.
2. **Diretório gerenciado reutiliza a raiz `pi-sessions/`.** O `omp` é fork
   do `pi` e respeita o mesmo layout de `--session-dir`. Uma raiz
   `omp-sessions/` separada só adicionaria um caminho a mais para o remove
   limpar. `managed_session_dir` já valida que o diretório está sob a raiz.
3. **Sem marker e sem `--session-dir` → reinício limpo, não `--continue`.**
   O adapter do `pi` faz `--continue` nesse caso porque assume diretório
   pinado por construção. Para a família pi existem 36+ sessões legadas no
   worktree compartilhado; `--continue` ali retomaria a conversa de outro
   Worker. A receita da família devolve o comando sem flag quando não há
   nem id nem diretório pinado. Custo: sessões legadas sem marker perdem o
   histórico no restart — que é exatamente o que já acontece hoje.
4. **Capabilities declaradas no `runtime.toml` dos dois runtimes**, não
   inferidas do adapter. O schema exige a declaração explícita e a UI já lê
   `WorkersSessionCapabilities.restart/resume_agent` dali.

## Risks / Trade-offs

- [`omp` muda o nome do flag numa versão futura] → os testes de unidade do
  adapter fixam o comando gerado; a quebra aparece no primeiro `cargo test`
  após bump do CLI, não em produção silenciosa.
- [Sessões legadas continuam sem resume] → aceito; a change não piora nada
  e o marker já cobre a maioria das sessões lançadas após
  `show-worker-model-token-usage`.
- [`prime-agent` pode divergir do `omp` nos flags] → o adapter é
  compartilhado de propósito; se divergir, o teste de conformidade
  `setup_conformance_tests.rs` por runtime é o lugar de fixar a diferença.
- [Mudança em código vendorizado] → `third_party/unpeel-upstream.toml`
  atualizado no mesmo commit, conforme contrato do `AGENTS.md` do
  `workers-unpeel`.

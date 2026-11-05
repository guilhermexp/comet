# Resolução dos conflitos

153 paths medidos e resolvidos. Escolhas por bloco: `resolutions.log`; originais: `conflict-originals.json`.

| Path | Resolução |
|---|---|
| `.github/CODEOWNERS` | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `.github/actions/windows-ci-setup/action.yml` | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `.github/workflows/android.yml` | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `.github/workflows/deploy.yml` | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `.github/workflows/linux-installer.yml` | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `.github/workflows/macos.yml` | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `.github/workflows/preview-tests.yml` | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `.github/workflows/release.yml` | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `.github/workflows/testflight.yml` | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `.github/workflows/ui-tests.yml` | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `.github/workflows/voice-tests.yml` | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `.github/workflows/windows.yml` | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `.gitignore` | Contrato prévio: o fork versiona .agents/.claude/CLAUDE.md; não importar ignore que esconde customizações. |
| `Cargo.lock` | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `Cargo.toml` | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `apps/zeron/Cargo.toml` | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `apps/zeron/src/bin/zeron-engine.rs` | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `apps/zeron/src/lib.rs` | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `apps/zeron/src/main.rs` | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `crates/client/src/client.rs` | Tipos aditivos e construtores unidos ao código exclusivo do fork; resolução por bloco registrada em resolutions.log. |
| `crates/client/src/demo/fixtures.rs` | Tipos aditivos e construtores unidos ao código exclusivo do fork; resolução por bloco registrada em resolutions.log. |
| `crates/client/src/demo/transcripts.rs` | Tipos aditivos e construtores unidos ao código exclusivo do fork; resolução por bloco registrada em resolutions.log. |
| `crates/client/src/session/mod.rs` | Tipos aditivos e construtores unidos ao código exclusivo do fork; resolução por bloco registrada em resolutions.log. |
| `crates/client/src/subagents.rs` | Tipos aditivos e construtores unidos ao código exclusivo do fork; resolução por bloco registrada em resolutions.log. |
| `crates/client/tests/live.rs` | Tipos aditivos e construtores unidos ao código exclusivo do fork; resolução por bloco registrada em resolutions.log. |
| `crates/doc/examples/gen_fixture.rs` | D6/D7: resumo não-command e metadata/preview do fork conservados junto de tail/exit; tests e assinaturas SegmentWriter adaptados, childKind aditivo, rustfmt/imports. |
| `crates/doc/src/commands.rs` | D6/D7: resumo não-command e metadata/preview do fork conservados junto de tail/exit; tests e assinaturas SegmentWriter adaptados, childKind aditivo, rustfmt/imports. |
| `crates/doc/src/parts.rs` | D6/D7: resumo não-command e metadata/preview do fork conservados junto de tail/exit; tests e assinaturas SegmentWriter adaptados, childKind aditivo, rustfmt/imports. |
| `crates/doc/src/rebuild.rs` | D6/D7: resumo não-command e metadata/preview do fork conservados junto de tail/exit; tests e assinaturas SegmentWriter adaptados, childKind aditivo, rustfmt/imports. |
| `crates/doc/src/registry.rs` | D6/D7: resumo não-command e metadata/preview do fork conservados junto de tail/exit; tests e assinaturas SegmentWriter adaptados, childKind aditivo, rustfmt/imports. |
| `crates/doc/src/registry/tests.rs` | D6/D7: resumo não-command e metadata/preview do fork conservados junto de tail/exit; tests e assinaturas SegmentWriter adaptados, childKind aditivo, rustfmt/imports. |
| `crates/doc/src/schema.rs` | D6/D7: resumo não-command e metadata/preview do fork conservados junto de tail/exit; tests e assinaturas SegmentWriter adaptados, childKind aditivo, rustfmt/imports. |
| `crates/doc/src/workspace.rs` | D6/D7: resumo não-command e metadata/preview do fork conservados junto de tail/exit; tests e assinaturas SegmentWriter adaptados, childKind aditivo, rustfmt/imports. |
| `crates/doc/tests/attachments_roundtrip.rs` | D6/D7: resumo não-command e metadata/preview do fork conservados junto de tail/exit; tests e assinaturas SegmentWriter adaptados, childKind aditivo, rustfmt/imports. |
| `crates/engine/src/agent_accounts.rs` | Contratos do fork preservados: Runner limitado/leases/auth/Usage; APIs novas aceitas com tipos/imports/rustfmt. SourceControl signals usa ProcessRequest do host, sem perder limites herdados. |
| `crates/engine/src/chat_persistence.rs` | Contratos do fork preservados: Runner limitado/leases/auth/Usage; APIs novas aceitas com tipos/imports/rustfmt. SourceControl signals usa ProcessRequest do host, sem perder limites herdados. |
| `crates/engine/src/doc_host.rs` | Contratos do fork preservados: Runner limitado/leases/auth/Usage; APIs novas aceitas com tipos/imports/rustfmt. SourceControl signals usa ProcessRequest do host, sem perder limites herdados. |
| `crates/engine/src/harness_updates.rs` | Contratos do fork preservados: Runner limitado/leases/auth/Usage; APIs novas aceitas com tipos/imports/rustfmt. SourceControl signals usa ProcessRequest do host, sem perder limites herdados. |
| `crates/engine/src/lib.rs` | D1/D4/D5/D7/D8: não ativar plugins/channel/agent-token global; grants do fork, mcp/extra_mcp vazios, watch do TurnControl e publicação visual conservados. Restante rustfmt/imports/comentários. |
| `crates/engine/src/registry.rs` | Contratos do fork preservados: Runner limitado/leases/auth/Usage; APIs novas aceitas com tipos/imports/rustfmt. SourceControl signals usa ProcessRequest do host, sem perder limites herdados. |
| `crates/engine/src/rpc.rs` | D5: constantes, forwarding, local_only e deadlines portados ao registry único method.rs; Caller/relay preservados. D10: dispatch dual de criação; demais alterações são rustfmt/imports. |
| `crates/engine/src/run_journal.rs` | Contratos do fork preservados: Runner limitado/leases/auth/Usage; APIs novas aceitas com tipos/imports/rustfmt. SourceControl signals usa ProcessRequest do host, sem perder limites herdados. |
| `crates/engine/src/runner.rs` | Contratos do fork preservados: Runner limitado/leases/auth/Usage; APIs novas aceitas com tipos/imports/rustfmt. SourceControl signals usa ProcessRequest do host, sem perder limites herdados. |
| `crates/engine/src/sessions.rs` | D1/D4/D5/D7/D8: não ativar plugins/channel/agent-token global; grants do fork, mcp/extra_mcp vazios, watch do TurnControl e publicação visual conservados. Restante rustfmt/imports/comentários. |
| `crates/engine/src/titles.rs` | Contratos do fork preservados: Runner limitado/leases/auth/Usage; APIs novas aceitas com tipos/imports/rustfmt. SourceControl signals usa ProcessRequest do host, sem perder limites herdados. |
| `crates/engine/src/workspace_host.rs` | D6: origin_chat_id/parent_chat_id independentes e childKind aditivo; não filtrar kind apagando linhagem independente do fork. |
| `crates/engine/tests/acp_lifecycle.rs` | D4/D6/D7: literais aditivos, imports/rustfmt e harness/grants do fork; testes e2e de imagem/output retidos. Nenhum provedor real é usado como prova local. |
| `crates/engine/tests/born_chat2_race.rs` | D4/D6/D7: literais aditivos, imports/rustfmt e harness/grants do fork; testes e2e de imagem/output retidos. Nenhum provedor real é usado como prova local. |
| `crates/engine/tests/cloud_sleep_quiesce.rs` | D4/D6/D7: literais aditivos, imports/rustfmt e harness/grants do fork; testes e2e de imagem/output retidos. Nenhum provedor real é usado como prova local. |
| `crates/engine/tests/device_routing.rs` | D4/D6/D7: literais aditivos, imports/rustfmt e harness/grants do fork; testes e2e de imagem/output retidos. Nenhum provedor real é usado como prova local. |
| `crates/engine/tests/e2e.rs` | D4/D6/D7: literais aditivos, imports/rustfmt e harness/grants do fork; testes e2e de imagem/output retidos. Nenhum provedor real é usado como prova local. |
| `crates/engine/tests/m5_repos_diffs_terminals.rs` | D4/D6/D7: literais aditivos, imports/rustfmt e harness/grants do fork; testes e2e de imagem/output retidos. Nenhum provedor real é usado como prova local. |
| `crates/engine/tests/m5c_accounts_uploads_titles.rs` | D4/D6/D7: literais aditivos, imports/rustfmt e harness/grants do fork; testes e2e de imagem/output retidos. Nenhum provedor real é usado como prova local. |
| `crates/engine/tests/projectless.rs` | D4/D6/D7: literais aditivos, imports/rustfmt e harness/grants do fork; testes e2e de imagem/output retidos. Nenhum provedor real é usado como prova local. |
| `crates/engine/tests/queued_attachments.rs` | D4/D6/D7: literais aditivos, imports/rustfmt e harness/grants do fork; testes e2e de imagem/output retidos. Nenhum provedor real é usado como prova local. |
| `crates/engine/tests/relay_delivery.rs` | D4/D6/D7: literais aditivos, imports/rustfmt e harness/grants do fork; testes e2e de imagem/output retidos. Nenhum provedor real é usado como prova local. |
| `crates/engine/tests/restart_resume.rs` | D4/D6/D7: literais aditivos, imports/rustfmt e harness/grants do fork; testes e2e de imagem/output retidos. Nenhum provedor real é usado como prova local. |
| `crates/engine/tests/self_continued_quiesce.rs` | D4/D6/D7: literais aditivos, imports/rustfmt e harness/grants do fork; testes e2e de imagem/output retidos. Nenhum provedor real é usado como prova local. |
| `crates/engine/tests/session_publication.rs` | D4/D6/D7: literais aditivos, imports/rustfmt e harness/grants do fork; testes e2e de imagem/output retidos. Nenhum provedor real é usado como prova local. |
| `crates/engine/tests/side_chats.rs` | D4/D6/D7: literais aditivos, imports/rustfmt e harness/grants do fork; testes e2e de imagem/output retidos. Nenhum provedor real é usado como prova local. |
| `crates/engine/tests/steering_live.rs` | D4/D6/D7: literais aditivos, imports/rustfmt e harness/grants do fork; testes e2e de imagem/output retidos. Nenhum provedor real é usado como prova local. |
| `crates/engine/tests/subagent_idle_reap.rs` | D4/D6/D7: literais aditivos, imports/rustfmt e harness/grants do fork; testes e2e de imagem/output retidos. Nenhum provedor real é usado como prova local. |
| `crates/engine/tests/turn_quiesce.rs` | D4/D6/D7: literais aditivos, imports/rustfmt e harness/grants do fork; testes e2e de imagem/output retidos. Nenhum provedor real é usado como prova local. |
| `crates/engine/tests/unarchive_on_send.rs` | D4/D6/D7: literais aditivos, imports/rustfmt e harness/grants do fork; testes e2e de imagem/output retidos. Nenhum provedor real é usado como prova local. |
| `crates/engine/tests/workspace_files.rs` | D10: homônimo incompatível renomeado create_entry_mutation/CreateWorkspaceEntryMutationRequest. Seção de teste recuperada/adaptada em workspace_files_mutations; ambas APIs, no-replace e checkout stale cobertos. |
| `crates/engine/tests/workspace_sync.rs` | D4/D6/D7: literais aditivos, imports/rustfmt e harness/grants do fork; testes e2e de imagem/output retidos. Nenhum provedor real é usado como prova local. |
| `crates/engine/tests/worktree_on_run.rs` | D4/D6/D7: literais aditivos, imports/rustfmt e harness/grants do fork; testes e2e de imagem/output retidos. Nenhum provedor real é usado como prova local. |
| `crates/harness/examples/grok_subagent_probe.rs` | D4: Fx/ACP registry aceitos com OMP/Kimi, leases e grants do fork; aliases/tipos, imports e rustfmt. Novo código do registry preservado. |
| `crates/harness/examples/opencode_subagent_probe.rs` | D4: Fx/ACP registry aceitos com OMP/Kimi, leases e grants do fork; aliases/tipos, imports e rustfmt. Novo código do registry preservado. |
| `crates/harness/src/acp/mod.rs` | D4: Fx/ACP registry aceitos com OMP/Kimi, leases e grants do fork; aliases/tipos, imports e rustfmt. Novo código do registry preservado. |
| `crates/harness/src/acp/normalize.rs` | D4: Fx/ACP registry aceitos com OMP/Kimi, leases e grants do fork; aliases/tipos, imports e rustfmt. Novo código do registry preservado. |
| `crates/harness/src/acp/subagent.rs` | D4: Fx/ACP registry aceitos com OMP/Kimi, leases e grants do fork; aliases/tipos, imports e rustfmt. Novo código do registry preservado. |
| `crates/harness/src/acp/system_message.rs` | D4: Fx/ACP registry aceitos com OMP/Kimi, leases e grants do fork; aliases/tipos, imports e rustfmt. Novo código do registry preservado. |
| `crates/harness/src/claude/mod.rs` | D4/D7: configuração combina grants do fork; resultados não-command preservam texto limitado do fork. Tail/exit de Bash aceitos; imports/comentários/rustfmt adaptados. |
| `crates/harness/src/claude/normalize.rs` | D4/D7: configuração combina grants do fork; resultados não-command preservam texto limitado do fork. Tail/exit de Bash aceitos; imports/comentários/rustfmt adaptados. |
| `crates/harness/src/claude/wire.rs` | D4/D7: configuração combina grants do fork; resultados não-command preservam texto limitado do fork. Tail/exit de Bash aceitos; imports/comentários/rustfmt adaptados. |
| `crates/harness/src/codex/mod.rs` | D4/D7: grants/metadata de execução do fork coexistem com access/plan/tail/exit opcionais. Igualdade compara acesso efetivo e grants; non-command conserva resumo. Restante rustfmt. |
| `crates/harness/src/codex/normalize.rs` | D4/D7: grants/metadata de execução do fork coexistem com access/plan/tail/exit opcionais. Igualdade compara acesso efetivo e grants; non-command conserva resumo. Restante rustfmt. |
| `crates/harness/src/cursor/mod.rs` | D4: Fx/ACP registry aceitos com OMP/Kimi, leases e grants do fork; aliases/tipos, imports e rustfmt. Novo código do registry preservado. |
| `crates/harness/src/install.rs` | D4: Fx/ACP registry aceitos com OMP/Kimi, leases e grants do fork; aliases/tipos, imports e rustfmt. Novo código do registry preservado. |
| `crates/harness/src/lib.rs` | D4: Fx/ACP registry aceitos com OMP/Kimi, leases e grants do fork; aliases/tipos, imports e rustfmt. Novo código do registry preservado. |
| `crates/harness/src/mock.rs` | D4: Fx/ACP registry aceitos com OMP/Kimi, leases e grants do fork; aliases/tipos, imports e rustfmt. Novo código do registry preservado. |
| `crates/harness/src/muse/normalize.rs` | D4: Fx/ACP registry aceitos com OMP/Kimi, leases e grants do fork; aliases/tipos, imports e rustfmt. Novo código do registry preservado. |
| `crates/harness/src/opencode/mod.rs` | D4: Fx/ACP registry aceitos com OMP/Kimi, leases e grants do fork; aliases/tipos, imports e rustfmt. Novo código do registry preservado. |
| `crates/harness/src/pi/mcp.rs` | D4: ponte por servidor preserva Workers/sessions/computer-use; extra MCP de plugin fica dormente/limpo pela engine; teste direto de configure_all retido. |
| `crates/harness/src/pi/mod.rs` | D4: ponte por servidor preserva Workers/sessions/computer-use; extra MCP de plugin fica dormente/limpo pela engine; teste direto de configure_all retido. |
| `crates/harness/src/pi/normalize.rs` | D4: ponte por servidor preserva Workers/sessions/computer-use; extra MCP de plugin fica dormente/limpo pela engine; teste direto de configure_all retido. |
| `crates/harness/src/skills.rs` | D4: Fx/ACP registry aceitos com OMP/Kimi, leases e grants do fork; aliases/tipos, imports e rustfmt. Novo código do registry preservado. |
| `crates/harness/tests/acp.rs` | D4: Fx/ACP registry aceitos com OMP/Kimi, leases e grants do fork; aliases/tipos, imports e rustfmt. Novo código do registry preservado. |
| `crates/harness/tests/acp_quiet.rs` | D4: Fx/ACP registry aceitos com OMP/Kimi, leases e grants do fork; aliases/tipos, imports e rustfmt. Novo código do registry preservado. |
| `crates/harness/tests/acp_stall.rs` | D4: Fx/ACP registry aceitos com OMP/Kimi, leases e grants do fork; aliases/tipos, imports e rustfmt. Novo código do registry preservado. |
| `crates/harness/tests/claude.rs` | D4: Fx/ACP registry aceitos com OMP/Kimi, leases e grants do fork; aliases/tipos, imports e rustfmt. Novo código do registry preservado. |
| `crates/harness/tests/claude_refresh_lock.rs` | D4: Fx/ACP registry aceitos com OMP/Kimi, leases e grants do fork; aliases/tipos, imports e rustfmt. Novo código do registry preservado. |
| `crates/harness/tests/codex.rs` | D4: Fx/ACP registry aceitos com OMP/Kimi, leases e grants do fork; aliases/tipos, imports e rustfmt. Novo código do registry preservado. |
| `crates/harness/tests/cursor.rs` | D4: Fx/ACP registry aceitos com OMP/Kimi, leases e grants do fork; aliases/tipos, imports e rustfmt. Novo código do registry preservado. |
| `crates/harness/tests/managed_install.rs` | D4: Fx/ACP registry aceitos com OMP/Kimi, leases e grants do fork; aliases/tipos, imports e rustfmt. Novo código do registry preservado. |
| `crates/harness/tests/managed_install_failure.rs` | D4: Fx/ACP registry aceitos com OMP/Kimi, leases e grants do fork; aliases/tipos, imports e rustfmt. Novo código do registry preservado. |
| `crates/harness/tests/misc/acp_hardening.rs` | D4: Fx/ACP registry aceitos com OMP/Kimi, leases e grants do fork; aliases/tipos, imports e rustfmt. Novo código do registry preservado. |
| `crates/harness/tests/misc/run_env.rs` | D4: Fx/ACP registry aceitos com OMP/Kimi, leases e grants do fork; aliases/tipos, imports e rustfmt. Novo código do registry preservado. |
| `crates/harness/tests/muse.rs` | D4: Fx/ACP registry aceitos com OMP/Kimi, leases e grants do fork; aliases/tipos, imports e rustfmt. Novo código do registry preservado. |
| `crates/harness/tests/muse_account_live.rs` | D4: Fx/ACP registry aceitos com OMP/Kimi, leases e grants do fork; aliases/tipos, imports e rustfmt. Novo código do registry preservado. |
| `crates/harness/tests/opencode.rs` | D4: Fx/ACP registry aceitos com OMP/Kimi, leases e grants do fork; aliases/tipos, imports e rustfmt. Novo código do registry preservado. |
| `crates/harness/tests/pi_rpc.rs` | D4: Fx/ACP registry aceitos com OMP/Kimi, leases e grants do fork; aliases/tipos, imports e rustfmt. Novo código do registry preservado. |
| `crates/harness/tests/real_quiet_survey.rs` | D4: Fx/ACP registry aceitos com OMP/Kimi, leases e grants do fork; aliases/tipos, imports e rustfmt. Novo código do registry preservado. |
| `crates/markdown/src/lib.rs` | Tipos aditivos e construtores unidos ao código exclusivo do fork; resolução por bloco registrada em resolutions.log. |
| `crates/mcp/src/jsonrpc.rs` | D6: default side preservado, subagent explícito aceito, configured/inherit e grants do fork; plan Option para recusar override explícito no inherit. Fixtures unknown usam future-provider, pois Fx passou a ser conhecido. |
| `crates/mcp/src/tools.rs` | D6: default side preservado, subagent explícito aceito, configured/inherit e grants do fork; plan Option para recusar override explícito no inherit. Fixtures unknown usam future-provider, pois Fx passou a ser conhecido. |
| `crates/mcp/src/transcript.rs` | D6: default side preservado, subagent explícito aceito, configured/inherit e grants do fork; plan Option para recusar override explícito no inherit. Fixtures unknown usam future-provider, pois Fx passou a ser conhecido. |
| `crates/mcp/src/zeron.rs` | D5: manter connect_ipc com bearer completo do grant + bind ao engine, inclusive reconexão; escopo agent novo não cobre todo o contrato do fork. |
| `crates/mobile/src/layout/tests.rs` | Tipos aditivos e construtores unidos ao código exclusivo do fork; resolução por bloco registrada em resolutions.log. |
| `crates/preview/src/lib.rs` | Preview/sign-in/fragmentação aceitos; diferenças de imports/rustfmt e process metadata aditiva, mantendo visibilidade do fork. |
| `crates/preview/src/service.rs` | Preview/sign-in/fragmentação aceitos; diferenças de imports/rustfmt e process metadata aditiva, mantendo visibilidade do fork. |
| `crates/proto/src/agent.rs` | D4/D6/D10: ALL conserva 14 IDs com OMP/Kimi/Fx; campos aditivos com defaults, lineage independente e tipos distintos de criação. Fixtures unknown atualizadas. Restante imports/rustfmt. |
| `crates/proto/src/entities.rs` | D4/D6/D10: ALL conserva 14 IDs com OMP/Kimi/Fx; campos aditivos com defaults, lineage independente e tipos distintos de criação. Fixtures unknown atualizadas. Restante imports/rustfmt. |
| `crates/proto/src/view.rs` | D4/D6/D10: ALL conserva 14 IDs com OMP/Kimi/Fx; campos aditivos com defaults, lineage independente e tipos distintos de criação. Fixtures unknown atualizadas. Restante imports/rustfmt. |
| `crates/rpc/src/client.rs` | D5: constantes, forwarding, local_only e deadlines portados ao registry único method.rs; Caller/relay preservados. D10: dispatch dual de criação; demais alterações são rustfmt/imports. |
| `crates/rpc/src/device_room.rs` | D5: constantes, forwarding, local_only e deadlines portados ao registry único method.rs; Caller/relay preservados. D10: dispatch dual de criação; demais alterações são rustfmt/imports. |
| `crates/rpc/src/lib.rs` | D5: constantes, forwarding, local_only e deadlines portados ao registry único method.rs; Caller/relay preservados. D10: dispatch dual de criação; demais alterações são rustfmt/imports. |
| `crates/rpc/src/server.rs` | D5: constantes, forwarding, local_only e deadlines portados ao registry único method.rs; Caller/relay preservados. D10: dispatch dual de criação; demais alterações são rustfmt/imports. |
| `crates/ui/Cargo.toml` | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `crates/ui/examples/subagent-fold-fixture.rs` | D3/D4/D9: renderer/fixtures/links/Workers do fork coexistem com APIs novas; diferenças restantes imports/rustfmt/assinaturas. WPE compila em fallback GTK nesta box. |
| `crates/ui/src/account_usage.rs` | D3: arquivo de anel Usage permanece ausente; quotas e usage_lines do fork continuam em Settings/Details/composer. |
| `crates/ui/src/app_update.rs` | D8: updater/feed/RestartGate e engine in-process do fork; canais beta/managed novos não ligados. |
| `crates/ui/src/composer.rs` | D3: Usage no presenter do fork e fade do todo já existente; request leva access/plan resolvidos, intake/attachments/chips e steer conservados. |
| `crates/ui/src/composer/chip.rs` | D3/D4/D9: renderer/fixtures/links/Workers do fork coexistem com APIs novas; diferenças restantes imports/rustfmt/assinaturas. WPE compila em fallback GTK nesta box. |
| `crates/ui/src/lib.rs` | D3/D4/D9: renderer/fixtures/links/Workers do fork coexistem com APIs novas; diferenças restantes imports/rustfmt/assinaturas. WPE compila em fallback GTK nesta box. |
| `crates/ui/src/pickers.rs` | D3/D4/D9: renderer/fixtures/links/Workers do fork coexistem com APIs novas; diferenças restantes imports/rustfmt/assinaturas. WPE compila em fallback GTK nesta box. |
| `crates/ui/src/rail.rs` | D3: cache RailSnapshot do fork/caps no sync conservados; Markdown/caption portados, ranges mono derivados de TextRun em vez de flat.code_ranges. |
| `crates/ui/src/settings.rs` | D3/D8: Projects e Accounts separados de Harnesses; CommitCredit/banner/ring/beta recusados. APIs/fixtures Shell usam Workers model; ícones do catálogo único; writer central preservado. |
| `crates/ui/src/settings/accounts.rs` | D3/D8: Projects e Accounts separados de Harnesses; CommitCredit/banner/ring/beta recusados. APIs/fixtures Shell usam Workers model; ícones do catálogo único; writer central preservado. |
| `crates/ui/src/settings/cloud.rs` | D3/D8: Projects e Accounts separados de Harnesses; CommitCredit/banner/ring/beta recusados. APIs/fixtures Shell usam Workers model; ícones do catálogo único; writer central preservado. |
| `crates/ui/src/settings/shortcuts.rs` | D3/D8: Projects e Accounts separados de Harnesses; CommitCredit/banner/ring/beta recusados. APIs/fixtures Shell usam Workers model; ícones do catálogo único; writer central preservado. |
| `crates/ui/src/settings/thread_naming.rs` | D3/D8: Projects e Accounts separados de Harnesses; CommitCredit/banner/ring/beta recusados. APIs/fixtures Shell usam Workers model; ícones do catálogo único; writer central preservado. |
| `crates/ui/src/settings/worktrees.rs` | D3/D8: Projects e Accounts separados de Harnesses; CommitCredit/banner/ring/beta recusados. APIs/fixtures Shell usam Workers model; ícones do catálogo único; writer central preservado. |
| `crates/ui/src/shell.rs` | D3/D8: Projects e Accounts separados de Harnesses; CommitCredit/banner/ring/beta recusados. APIs/fixtures Shell usam Workers model; ícones do catálogo único; writer central preservado. |
| `crates/ui/src/shell/session_lanes.rs` | D3/D4/D9: renderer/fixtures/links/Workers do fork coexistem com APIs novas; diferenças restantes imports/rustfmt/assinaturas. WPE compila em fallback GTK nesta box. |
| `crates/ui/src/state.rs` | D3/D4/D9: renderer/fixtures/links/Workers do fork coexistem com APIs novas; diferenças restantes imports/rustfmt/assinaturas. WPE compila em fallback GTK nesta box. |
| `crates/ui/src/todo_panel.rs` | D3/D4/D9: renderer/fixtures/links/Workers do fork coexistem com APIs novas; diferenças restantes imports/rustfmt/assinaturas. WPE compila em fallback GTK nesta box. |
| `crates/ui/src/transcript.rs` | D3/D7: TurnSteps compacto e QuickJS Mermaid do fork; UI-code options adaptadas ao renderer existente; chips URL/menções, MIME e budgets preservados. Cloud boot palavra/status dormente. Novos testes Markdown recuperados e assinaturas/fixtures adaptadas. |
| `crates/update/src/lib.rs` | D8: updater/feed/RestartGate e engine in-process do fork; canais beta/managed novos não ligados. |
| `edge/package-lock.json` | D2: diretório/profile/card-web/plugins/Cloud e rotas/testes/bindings associados dormentes; preservar grafo/config/testes do fork. Loro e preview vivos; lock regenerado sem assets do card-web. |
| `edge/src/env.ts` | D2: diretório/profile/card-web/plugins/Cloud e rotas/testes/bindings associados dormentes; preservar grafo/config/testes do fork. Loro e preview vivos; lock regenerado sem assets do card-web. |
| `edge/src/forward.ts` | D2: diretório/profile/card-web/plugins/Cloud e rotas/testes/bindings associados dormentes; preservar grafo/config/testes do fork. Loro e preview vivos; lock regenerado sem assets do card-web. |
| `edge/src/index.ts` | D2: diretório/profile/card-web/plugins/Cloud e rotas/testes/bindings associados dormentes; preservar grafo/config/testes do fork. Loro e preview vivos; lock regenerado sem assets do card-web. |
| `edge/src/preview-room.ts` | D2: diretório/profile/card-web/plugins/Cloud e rotas/testes/bindings associados dormentes; preservar grafo/config/testes do fork. Loro e preview vivos; lock regenerado sem assets do card-web. |
| `edge/src/preview-route.ts` | D2: diretório/profile/card-web/plugins/Cloud e rotas/testes/bindings associados dormentes; preservar grafo/config/testes do fork. Loro e preview vivos; lock regenerado sem assets do card-web. |
| `edge/test/workerd/env.d.ts` | D2: diretório/profile/card-web/plugins/Cloud e rotas/testes/bindings associados dormentes; preservar grafo/config/testes do fork. Loro e preview vivos; lock regenerado sem assets do card-web. |
| `edge/test/workerd/fixture.ts` | D2: diretório/profile/card-web/plugins/Cloud e rotas/testes/bindings associados dormentes; preservar grafo/config/testes do fork. Loro e preview vivos; lock regenerado sem assets do card-web. |
| `edge/vitest.workerd.config.ts` | D2: diretório/profile/card-web/plugins/Cloud e rotas/testes/bindings associados dormentes; preservar grafo/config/testes do fork. Loro e preview vivos; lock regenerado sem assets do card-web. |
| `edge/wrangler.jsonc` | D2: diretório/profile/card-web/plugins/Cloud e rotas/testes/bindings associados dormentes; preservar grafo/config/testes do fork. Loro e preview vivos; lock regenerado sem assets do card-web. |
| `scripts/ci/test-core.sh` | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `scripts/dev-demo.sh` | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `scripts/ios/build-core.sh` | Fluxo de build do fork preserva normalização Swift canônica; refresh de header/modulemap aditivo, bindings geradas do core integrado. Link iOS pendente no Mac. |

## Follow-up F1/F2 (2026-11-05)

Auditoria de todos os `-p`/`--test` em rust.yml, ui-tests.yml e test-core.sh via cargo metadata e git show 35561ead: geometry existe em crates/orb/tests/geometry.rs (agora inclui zeron-orb); codex_subagents, claude_subagents, local_profiles, pi_resume e osc7501 existem como módulos de crates/engine/tests/misc/main.rs, já selecionado. voice_orb_geometry não existe em 35561ead; a geometria ativa está na crate orb e a UI conserva --lib. Nenhum alvo perdido no merge foi identificado. Os demais pacotes/alvos explícitos correspondem aos manifestos (GPUI macOS usa o workspace third_party/zui).

## Follow-up F5 (2026-11-05)

servers::stop chamava libc::kill/pid_t sem cfg Unix. Windows usa ToolHelp na dependência windows-sys já existente para capturar a árvore do launcher autorizado, com handles retidos e criação pai/filho validada; TerminateProcess encerra a árvore e erros são propagados. A API de Job do harness não adota uma árvore existente: ela é usada no spawn de processos próprios; servidores externos existentes não pertencem a ele. O check preview x86_64-pc-windows-gnu reproduziu E0425 antes e passou depois. Descoberta Windows já devolvia catálogo vazio/identity false: não foi ativada nem alegada como provada.

## Round2 — R3/R4/R5/R6 (2026-11-05)

A regra de exceção por baseline foi revogada. ACP registry mantém o índice/artefato loopback e versões esperadas; os probes de apply usam o registro gerenciado, pois ACP não exige CLI de versão. O schema peer sessions mantém ações/argumentos/help e tetos; somente a introdução anunciada foi resumida. Wrappers são publicados após staging único, chmod e close, sem truncar o inode executável.

R6 estende F5 ao par real de native-platform-check: o vendor e a fronteira Workers usam primitivas Unix/Win32 reais (locks, DACL/SID, rename durável e identidade de processo), com erros em operações não suportadas. OMP passa pelos tipos de processo do harness para conservar Job Object e pipes Windows. O Host PTY local continua Unix por contrato; nenhum no-op anuncia sucesso no Windows. Nenhuma versão do app, feed/updater do app, edge, grant ou MCP de plugin foi alterado nesta rodada.

R2: a UI do fork mantém Chat titles, Collapse command blocks, Show voice orb e Computer Use/Available to; catálogo e anchors acompanham esses controles. A sidebar mantém o layout do fork (4px após branch curta, branch longa ≤ metade, PR contido). O teste mede o presenter sem cache e reutiliza o roteador OpenPullRequest do Shell, sem remover asserções. O selector de URL foi restaurado; o roteamento de preferência existente foi preservado.

Rodada 2, gate workspace: `tests/cloud_rpc.rs` ainda esperava ignorar targetDeviceId. D5 exige recusar a tentativa antes do dispatch. Fixture agora exige essa recusa sem rede e mantém todas as asserções da chamada local autenticada; não habilita Cloud/runner no startup.

Falha adicional round2: fila/contexto de Stop usavam a última row (inclusive User) como turn_id. Engine/client agora referenciam Assistant; avaliação do host não considera User um turno concluído. Rejeição de Stop para Assistant antigo preservada. Isso mantém a intenção D4/Stop, sem alterar grants, wire shape ou ativar runtime dormente.

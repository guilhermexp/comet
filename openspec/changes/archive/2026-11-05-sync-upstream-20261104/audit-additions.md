# Auditoria de additions sobreviventes

Intervalo exclusivo `d5a01522..35561ead`; cada um dos 94 commits TAKE/ADAPT foi inspecionado. O comparador extrai additions por commit, conserva só linhas presentes em 35561ead e procura cada linha no path integrado (ignora indentação). O JSON local `additions-missing.json` conserva todas as linhas exatas e SHAs; a tabela abaixo cobre cada path com diferenças, incluindo ruído de rustfmt. Isto não compara o fork inteiro ao upstream.

Correções feitas durante a auditoria: regressões Markdown/geração de imagens recuperadas; APIs de criação coexistentes com teste RPC; optional fields preenchidos sem apagar metadata; grants/Usage/lineage conferidos.

| Path | Linhas únicas distintas | Justificativa |
|---|---:|---|
| `.github/actions/windows-ci-setup/action.yml` | 23 | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `.github/workflows/android.yml` | 30 | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `.github/workflows/ci-emergency.yml` | 37 | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `.github/workflows/ci.yml` | 179 | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `.github/workflows/cursor-compatibility.yml` | 19 | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `.github/workflows/deploy.yml` | 5 | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `.github/workflows/linux-installer.yml` | 13 | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `.github/workflows/macos.yml` | 67 | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `.github/workflows/preview-tests.yml` | 27 | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `.github/workflows/ui-tests.yml` | 47 | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `.github/workflows/voice-tests.yml` | 45 | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `.github/workflows/weekly-tests.yml` | 38 | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `.github/workflows/windows.yml` | 51 | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `.github/workflows/zgx-beta.yml` | 213 | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `.gitignore` | 1 | Contrato prévio: o fork versiona .agents/.claude/CLAUDE.md; não importar ignore que esconde customizações. |
| `Cargo.lock` | 7 | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `Cargo.toml` | 7 | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `apps/zeron/Cargo.toml` | 1 | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `apps/zeron/src/bin/zeron-engine.rs` | 22 | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `apps/zeron/src/lib.rs` | 15 | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `apps/zeron/src/main.rs` | 14 | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `apps/zeron/tests/cli.rs` | 44 | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `crates/doc/src/parts.rs` | 17 | D6/D7: resumo não-command e metadata/preview do fork conservados junto de tail/exit; tests e assinaturas SegmentWriter adaptados, childKind aditivo, rustfmt/imports. |
| `crates/doc/src/registry.rs` | 3 | D6/D7: resumo não-command e metadata/preview do fork conservados junto de tail/exit; tests e assinaturas SegmentWriter adaptados, childKind aditivo, rustfmt/imports. |
| `crates/doc/src/registry/tests.rs` | 5 | D6/D7: resumo não-command e metadata/preview do fork conservados junto de tail/exit; tests e assinaturas SegmentWriter adaptados, childKind aditivo, rustfmt/imports. |
| `crates/doc/src/schema.rs` | 1 | D6/D7: resumo não-command e metadata/preview do fork conservados junto de tail/exit; tests e assinaturas SegmentWriter adaptados, childKind aditivo, rustfmt/imports. |
| `crates/doc/src/workspace.rs` | 1 | D6/D7: resumo não-command e metadata/preview do fork conservados junto de tail/exit; tests e assinaturas SegmentWriter adaptados, childKind aditivo, rustfmt/imports. |
| `crates/engine/Cargo.toml` | 1 | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `crates/engine/src/acp_registry.rs` | 26 | D4: Fx/ACP registry aceitos com OMP/Kimi, leases e grants do fork; aliases/tipos, imports e rustfmt. Novo código do registry preservado. |
| `crates/engine/src/agent_accounts.rs` | 2 | Contratos do fork preservados: Runner limitado/leases/auth/Usage; APIs novas aceitas com tipos/imports/rustfmt. SourceControl signals usa ProcessRequest do host, sem perder limites herdados. |
| `crates/engine/src/agent_accounts/provider_tests.rs` | 7 | Contratos do fork preservados: Runner limitado/leases/auth/Usage; APIs novas aceitas com tipos/imports/rustfmt. SourceControl signals usa ProcessRequest do host, sem perder limites herdados. |
| `crates/engine/src/agent_accounts/registry_login.rs` | 4 | Contratos do fork preservados: Runner limitado/leases/auth/Usage; APIs novas aceitas com tipos/imports/rustfmt. SourceControl signals usa ProcessRequest do host, sem perder limites herdados. |
| `crates/engine/src/auth.rs` | 19 | Contratos do fork preservados: Runner limitado/leases/auth/Usage; APIs novas aceitas com tipos/imports/rustfmt. SourceControl signals usa ProcessRequest do host, sem perder limites herdados. |
| `crates/engine/src/chat_persistence.rs` | 3 | Contratos do fork preservados: Runner limitado/leases/auth/Usage; APIs novas aceitas com tipos/imports/rustfmt. SourceControl signals usa ProcessRequest do host, sem perder limites herdados. |
| `crates/engine/src/deferred.rs` | 6 | Contratos do fork preservados: Runner limitado/leases/auth/Usage; APIs novas aceitas com tipos/imports/rustfmt. SourceControl signals usa ProcessRequest do host, sem perder limites herdados. |
| `crates/engine/src/desktop_notify.rs` | 11 | Contratos do fork preservados: Runner limitado/leases/auth/Usage; APIs novas aceitas com tipos/imports/rustfmt. SourceControl signals usa ProcessRequest do host, sem perder limites herdados. |
| `crates/engine/src/harness_updates.rs` | 5 | Contratos do fork preservados: Runner limitado/leases/auth/Usage; APIs novas aceitas com tipos/imports/rustfmt. SourceControl signals usa ProcessRequest do host, sem perder limites herdados. |
| `crates/engine/src/lib.rs` | 17 | D1/D4/D5/D7/D8: não ativar plugins/channel/agent-token global; grants do fork, mcp/extra_mcp vazios, watch do TurnControl e publicação visual conservados. Restante rustfmt/imports/comentários. |
| `crates/engine/src/managed.rs` | 17 | D8: updater/feed/RestartGate e engine in-process do fork; canais beta/managed novos não ligados. |
| `crates/engine/src/plugins/authority_tests.rs` | 35 | D1: fonte nova retida; diferenças de rustfmt (braces de closure/return, vírgulas e quebras), sem ativação automática. Serviços exercitados por fixtures explícitas. |
| `crates/engine/src/plugins/capability.rs` | 33 | D1: fonte nova retida; diferenças de rustfmt (braces de closure/return, vírgulas e quebras), sem ativação automática. Serviços exercitados por fixtures explícitas. |
| `crates/engine/src/plugins/git_service_tests.rs` | 97 | D1: fonte nova retida; diferenças de rustfmt (braces de closure/return, vírgulas e quebras), sem ativação automática. Serviços exercitados por fixtures explícitas. |
| `crates/engine/src/plugins/guides.rs` | 1 | D1: fonte nova retida; diferenças de rustfmt (braces de closure/return, vírgulas e quebras), sem ativação automática. Serviços exercitados por fixtures explícitas. |
| `crates/engine/src/plugins/identity_tests.rs` | 102 | D1: fonte nova retida; diferenças de rustfmt (braces de closure/return, vírgulas e quebras), sem ativação automática. Serviços exercitados por fixtures explícitas. |
| `crates/engine/src/plugins/install.rs` | 72 | D1: fonte nova retida; diferenças de rustfmt (braces de closure/return, vírgulas e quebras), sem ativação automática. Serviços exercitados por fixtures explícitas. |
| `crates/engine/src/plugins/keys.rs` | 39 | D1: fonte nova retida; diferenças de rustfmt (braces de closure/return, vírgulas e quebras), sem ativação automática. Serviços exercitados por fixtures explícitas. |
| `crates/engine/src/plugins/lifecycle.rs` | 25 | D1: fonte nova retida; diferenças de rustfmt (braces de closure/return, vírgulas e quebras), sem ativação automática. Serviços exercitados por fixtures explícitas. |
| `crates/engine/src/plugins/manifest.rs` | 11 | D1: fonte nova retida; diferenças de rustfmt (braces de closure/return, vírgulas e quebras), sem ativação automática. Serviços exercitados por fixtures explícitas. |
| `crates/engine/src/plugins/mod.rs` | 96 | D1: fonte nova retida; diferenças de rustfmt (braces de closure/return, vírgulas e quebras), sem ativação automática. Serviços exercitados por fixtures explícitas. |
| `crates/engine/src/plugins/package.rs` | 15 | D1: fonte nova retida; diferenças de rustfmt (braces de closure/return, vírgulas e quebras), sem ativação automática. Serviços exercitados por fixtures explícitas. |
| `crates/engine/src/plugins/reviews.rs` | 5 | D1: fonte nova retida; diferenças de rustfmt (braces de closure/return, vírgulas e quebras), sem ativação automática. Serviços exercitados por fixtures explícitas. |
| `crates/engine/src/plugins/scan.rs` | 2 | D1: fonte nova retida; diferenças de rustfmt (braces de closure/return, vírgulas e quebras), sem ativação automática. Serviços exercitados por fixtures explícitas. |
| `crates/engine/src/plugins/service/mod.rs` | 35 | D1: fonte nova retida; diferenças de rustfmt (braces de closure/return, vírgulas e quebras), sem ativação automática. Serviços exercitados por fixtures explícitas. |
| `crates/engine/src/plugins/service/tests.rs` | 33 | D1: fonte nova retida; diferenças de rustfmt (braces de closure/return, vírgulas e quebras), sem ativação automática. Serviços exercitados por fixtures explícitas. |
| `crates/engine/src/plugins/service/vm.rs` | 27 | D1: fonte nova retida; diferenças de rustfmt (braces de closure/return, vírgulas e quebras), sem ativação automática. Serviços exercitados por fixtures explícitas. |
| `crates/engine/src/plugins/service/vm/tests.rs` | 14 | D1: fonte nova retida; diferenças de rustfmt (braces de closure/return, vírgulas e quebras), sem ativação automática. Serviços exercitados por fixtures explícitas. |
| `crates/engine/src/plugins/service_tests.rs` | 28 | D1: fonte nova retida; diferenças de rustfmt (braces de closure/return, vírgulas e quebras), sem ativação automática. Serviços exercitados por fixtures explícitas. |
| `crates/engine/src/plugins/signing.rs` | 13 | D1: fonte nova retida; diferenças de rustfmt (braces de closure/return, vírgulas e quebras), sem ativação automática. Serviços exercitados por fixtures explícitas. |
| `crates/engine/src/plugins/snapshot.rs` | 21 | D1: fonte nova retida; diferenças de rustfmt (braces de closure/return, vírgulas e quebras), sem ativação automática. Serviços exercitados por fixtures explícitas. |
| `crates/engine/src/plugins/status.rs` | 13 | D1: fonte nova retida; diferenças de rustfmt (braces de closure/return, vírgulas e quebras), sem ativação automática. Serviços exercitados por fixtures explícitas. |
| `crates/engine/src/plugins/template_packages.rs` | 115 | D1: fonte nova retida; diferenças de rustfmt (braces de closure/return, vírgulas e quebras), sem ativação automática. Serviços exercitados por fixtures explícitas. |
| `crates/engine/src/plugins/templates.rs` | 19 | D1: fonte nova retida; diferenças de rustfmt (braces de closure/return, vírgulas e quebras), sem ativação automática. Serviços exercitados por fixtures explícitas. |
| `crates/engine/src/plugins/tests.rs` | 98 | D1: fonte nova retida; diferenças de rustfmt (braces de closure/return, vírgulas e quebras), sem ativação automática. Serviços exercitados por fixtures explícitas. |
| `crates/engine/src/plugins/themes.rs` | 17 | D1: fonte nova retida; diferenças de rustfmt (braces de closure/return, vírgulas e quebras), sem ativação automática. Serviços exercitados por fixtures explícitas. |
| `crates/engine/src/registry.rs` | 1 | Contratos do fork preservados: Runner limitado/leases/auth/Usage; APIs novas aceitas com tipos/imports/rustfmt. SourceControl signals usa ProcessRequest do host, sem perder limites herdados. |
| `crates/engine/src/rpc.rs` | 61 | D5: constantes, forwarding, local_only e deadlines portados ao registry único method.rs; Caller/relay preservados. D10: dispatch dual de criação; demais alterações são rustfmt/imports. |
| `crates/engine/src/rpc/agent_gate.rs` | 29 | D5/D6: gates de Chat e acesso preservados; imports e formatting. Não ampliar o novo allowlist para o controller do fork. |
| `crates/engine/src/run_journal.rs` | 1 | Contratos do fork preservados: Runner limitado/leases/auth/Usage; APIs novas aceitas com tipos/imports/rustfmt. SourceControl signals usa ProcessRequest do host, sem perder limites herdados. |
| `crates/engine/src/sessions.rs` | 14 | D1/D4/D5/D7/D8: não ativar plugins/channel/agent-token global; grants do fork, mcp/extra_mcp vazios, watch do TurnControl e publicação visual conservados. Restante rustfmt/imports/comentários. |
| `crates/engine/src/source_control.rs` | 46 | D3: signals preserva cwd/limites e lê viewer por fetch_pr_metadata autenticada; account do fork é fingerprint de credencial/cache, não login. Restante imports/rustfmt. |
| `crates/engine/src/terminals.rs` | 2 | Contratos do fork preservados: Runner limitado/leases/auth/Usage; APIs novas aceitas com tipos/imports/rustfmt. SourceControl signals usa ProcessRequest do host, sem perder limites herdados. |
| `crates/engine/src/workspace_files/mutations.rs` | 12 | D10: homônimo incompatível renomeado create_entry_mutation/CreateWorkspaceEntryMutationRequest. Seção de teste recuperada/adaptada em workspace_files_mutations; ambas APIs, no-replace e checkout stale cobertos. |
| `crates/engine/src/workspace_host.rs` | 3 | D6: origin_chat_id/parent_chat_id independentes e childKind aditivo; não filtrar kind apagando linhagem independente do fork. |
| `crates/engine/tests/acp_registry_rpc.rs` | 42 | D4: Fx/ACP registry aceitos com OMP/Kimi, leases e grants do fork; aliases/tipos, imports e rustfmt. Novo código do registry preservado. |
| `crates/engine/tests/auth.rs` | 5 | D4/D6/D7: literais aditivos, imports/rustfmt e harness/grants do fork; testes e2e de imagem/output retidos. Nenhum provedor real é usado como prova local. |
| `crates/engine/tests/e2e.rs` | 14 | D4/D6/D7: literais aditivos, imports/rustfmt e harness/grants do fork; testes e2e de imagem/output retidos. Nenhum provedor real é usado como prova local. |
| `crates/engine/tests/m5_repos_diffs_terminals.rs` | 1 | D4/D6/D7: literais aditivos, imports/rustfmt e harness/grants do fork; testes e2e de imagem/output retidos. Nenhum provedor real é usado como prova local. |
| `crates/engine/tests/plugin_services.rs` | 31 | D1: fixture ativa serviço explicitamente e liga plugins/links no RPC de teste, pois startup normal permanece dormente; rustfmt. |
| `crates/engine/tests/registry_login_tunnel.rs` | 5 | D4/D6/D7: literais aditivos, imports/rustfmt e harness/grants do fork; testes e2e de imagem/output retidos. Nenhum provedor real é usado como prova local. |
| `crates/engine/tests/steering_live.rs` | 1 | D4/D6/D7: literais aditivos, imports/rustfmt e harness/grants do fork; testes e2e de imagem/output retidos. Nenhum provedor real é usado como prova local. |
| `crates/engine/tests/workspace_files.rs` | 23 | D10: homônimo incompatível renomeado create_entry_mutation/CreateWorkspaceEntryMutationRequest. Seção de teste recuperada/adaptada em workspace_files_mutations; ambas APIs, no-replace e checkout stale cobertos. |
| `crates/engine/tests/workspace_sync.rs` | 3 | D4/D6/D7: literais aditivos, imports/rustfmt e harness/grants do fork; testes e2e de imagem/output retidos. Nenhum provedor real é usado como prova local. |
| `crates/harness/src/acp/mod.rs` | 39 | D4: Fx/ACP registry aceitos com OMP/Kimi, leases e grants do fork; aliases/tipos, imports e rustfmt. Novo código do registry preservado. |
| `crates/harness/src/acp_registry.rs` | 70 | D4: Fx/ACP registry aceitos com OMP/Kimi, leases e grants do fork; aliases/tipos, imports e rustfmt. Novo código do registry preservado. |
| `crates/harness/src/adapter_install.rs` | 2 | D4: Fx/ACP registry aceitos com OMP/Kimi, leases e grants do fork; aliases/tipos, imports e rustfmt. Novo código do registry preservado. |
| `crates/harness/src/claude/mod.rs` | 8 | D4/D7: configuração combina grants do fork; resultados não-command preservam texto limitado do fork. Tail/exit de Bash aceitos; imports/comentários/rustfmt adaptados. |
| `crates/harness/src/claude/normalize.rs` | 4 | D4/D7: configuração combina grants do fork; resultados não-command preservam texto limitado do fork. Tail/exit de Bash aceitos; imports/comentários/rustfmt adaptados. |
| `crates/harness/src/codex/mod.rs` | 4 | D4/D7: grants/metadata de execução do fork coexistem com access/plan/tail/exit opcionais. Igualdade compara acesso efetivo e grants; non-command conserva resumo. Restante rustfmt. |
| `crates/harness/src/codex/normalize.rs` | 6 | D4/D7: grants/metadata de execução do fork coexistem com access/plan/tail/exit opcionais. Igualdade compara acesso efetivo e grants; non-command conserva resumo. Restante rustfmt. |
| `crates/harness/src/install.rs` | 2 | D4: Fx/ACP registry aceitos com OMP/Kimi, leases e grants do fork; aliases/tipos, imports e rustfmt. Novo código do registry preservado. |
| `crates/harness/src/mock.rs` | 1 | D4: Fx/ACP registry aceitos com OMP/Kimi, leases e grants do fork; aliases/tipos, imports e rustfmt. Novo código do registry preservado. |
| `crates/harness/src/opencode/mod.rs` | 1 | D4: Fx/ACP registry aceitos com OMP/Kimi, leases e grants do fork; aliases/tipos, imports e rustfmt. Novo código do registry preservado. |
| `crates/harness/src/pi/mcp.rs` | 12 | D4: ponte por servidor preserva Workers/sessions/computer-use; extra MCP de plugin fica dormente/limpo pela engine; teste direto de configure_all retido. |
| `crates/harness/src/pi/mod.rs` | 9 | D4: ponte por servidor preserva Workers/sessions/computer-use; extra MCP de plugin fica dormente/limpo pela engine; teste direto de configure_all retido. |
| `crates/harness/tests/acp_registry.rs` | 7 | D4: Fx/ACP registry aceitos com OMP/Kimi, leases e grants do fork; aliases/tipos, imports e rustfmt. Novo código do registry preservado. |
| `crates/harness/tests/acp_registry_live.rs` | 14 | D4: Fx/ACP registry aceitos com OMP/Kimi, leases e grants do fork; aliases/tipos, imports e rustfmt. Novo código do registry preservado. |
| `crates/harness/tests/acp_registry_sign_in.rs` | 20 | D4: Fx/ACP registry aceitos com OMP/Kimi, leases e grants do fork; aliases/tipos, imports e rustfmt. Novo código do registry preservado. |
| `crates/harness/tests/codex.rs` | 1 | D4: Fx/ACP registry aceitos com OMP/Kimi, leases e grants do fork; aliases/tipos, imports e rustfmt. Novo código do registry preservado. |
| `crates/mcp/src/jsonrpc.rs` | 5 | D6: default side preservado, subagent explícito aceito, configured/inherit e grants do fork; plan Option para recusar override explícito no inherit. Fixtures unknown usam future-provider, pois Fx passou a ser conhecido. |
| `crates/mcp/src/tools.rs` | 24 | D6: default side preservado, subagent explícito aceito, configured/inherit e grants do fork; plan Option para recusar override explícito no inherit. Fixtures unknown usam future-provider, pois Fx passou a ser conhecido. |
| `crates/mcp/src/tools/plugins.rs` | 36 | D1: ferramentas disponíveis como fonte; runtime normal retorna indisponível. Restante rustfmt. |
| `crates/mcp/src/tools/themes.rs` | 29 | D3: ferramentas usam catálogo único do fork; formatting e inicialização compatível. |
| `crates/mcp/src/zeron.rs` | 3 | D5: manter connect_ipc com bearer completo do grant + bind ao engine, inclusive reconexão; escopo agent novo não cobre todo o contrato do fork. |
| `crates/plugin-guard/src/commands.rs` | 77 | D1: fonte nova retida; diferenças de rustfmt (braces de closure/return, vírgulas e quebras), sem ativação automática. Serviços exercitados por fixtures explícitas. |
| `crates/plugin-guard/src/commands/tests.rs` | 48 | D1: fonte nova retida; diferenças de rustfmt (braces de closure/return, vírgulas e quebras), sem ativação automática. Serviços exercitados por fixtures explícitas. |
| `crates/plugin-guard/src/net.rs` | 26 | D1: fonte nova retida; diferenças de rustfmt (braces de closure/return, vírgulas e quebras), sem ativação automática. Serviços exercitados por fixtures explícitas. |
| `crates/plugin-guard/src/path.rs` | 9 | D1: fonte nova retida; diferenças de rustfmt (braces de closure/return, vírgulas e quebras), sem ativação automática. Serviços exercitados por fixtures explícitas. |
| `crates/preview-label/src/lib.rs` | 20 | Preview/sign-in/fragmentação aceitos; diferenças de imports/rustfmt e process metadata aditiva, mantendo visibilidade do fork. |
| `crates/preview/src/catalog.rs` | 4 | Preview/sign-in/fragmentação aceitos; diferenças de imports/rustfmt e process metadata aditiva, mantendo visibilidade do fork. |
| `crates/preview/src/discovery.rs` | 10 | Preview/sign-in/fragmentação aceitos; diferenças de imports/rustfmt e process metadata aditiva, mantendo visibilidade do fork. |
| `crates/preview/src/service.rs` | 1 | Preview/sign-in/fragmentação aceitos; diferenças de imports/rustfmt e process metadata aditiva, mantendo visibilidade do fork. |
| `crates/preview/tests/discovery.rs` | 1 | Preview/sign-in/fragmentação aceitos; diferenças de imports/rustfmt e process metadata aditiva, mantendo visibilidade do fork. |
| `crates/proto/src/agent.rs` | 7 | D4/D6/D10: ALL conserva 14 IDs com OMP/Kimi/Fx; campos aditivos com defaults, lineage independente e tipos distintos de criação. Fixtures unknown atualizadas. Restante imports/rustfmt. |
| `crates/proto/src/agent_messages.rs` | 3 | D4/D6/D10: ALL conserva 14 IDs com OMP/Kimi/Fx; campos aditivos com defaults, lineage independente e tipos distintos de criação. Fixtures unknown atualizadas. Restante imports/rustfmt. |
| `crates/proto/src/entities.rs` | 16 | D4/D6/D10: ALL conserva 14 IDs com OMP/Kimi/Fx; campos aditivos com defaults, lineage independente e tipos distintos de criação. Fixtures unknown atualizadas. Restante imports/rustfmt. |
| `crates/proto/src/providers.rs` | 1 | D4/D6/D10: ALL conserva 14 IDs com OMP/Kimi/Fx; campos aditivos com defaults, lineage independente e tipos distintos de criação. Fixtures unknown atualizadas. Restante imports/rustfmt. |
| `crates/proto/src/workspace.rs` | 2 | D1: ZGX_INTERFACE_V1 não é anunciado no fork dormente; BACKGROUND_TASKS_V1 permanece. Constante e código de compatibilidade retidos. |
| `crates/rpc/src/client.rs` | 3 | D5: constantes, forwarding, local_only e deadlines portados ao registry único method.rs; Caller/relay preservados. D10: dispatch dual de criação; demais alterações são rustfmt/imports. |
| `crates/rpc/src/device_room.rs` | 32 | D5: constantes, forwarding, local_only e deadlines portados ao registry único method.rs; Caller/relay preservados. D10: dispatch dual de criação; demais alterações são rustfmt/imports. |
| `crates/rpc/src/lib.rs` | 281 | D5: constantes, forwarding, local_only e deadlines portados ao registry único method.rs; Caller/relay preservados. D10: dispatch dual de criação; demais alterações são rustfmt/imports. |
| `crates/rpc/src/server.rs` | 6 | D5: constantes, forwarding, local_only e deadlines portados ao registry único method.rs; Caller/relay preservados. D10: dispatch dual de criação; demais alterações são rustfmt/imports. |
| `crates/theme/src/catalog.rs` | 137 | D3: catálogo compartilhado sobre biblioteca existente, papéis/frost do fork inicializados; diferenças restantes apenas rustfmt. |
| `crates/theme/src/library.rs` | 3 | D3: catálogo compartilhado sobre biblioteca existente, papéis/frost do fork inicializados; diferenças restantes apenas rustfmt. |
| `crates/theme/src/luau_data.rs` | 6 | D3: catálogo compartilhado sobre biblioteca existente, papéis/frost do fork inicializados; diferenças restantes apenas rustfmt. |
| `crates/ui/Cargo.toml` | 1 | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `crates/ui/examples/browser-fixture.rs` | 1 | D3/D4/D9: renderer/fixtures/links/Workers do fork coexistem com APIs novas; diferenças restantes imports/rustfmt/assinaturas. WPE compila em fallback GTK nesta box. |
| `crates/ui/examples/message-markdown-fixture.rs` | 19 | D3/D4/D9: renderer/fixtures/links/Workers do fork coexistem com APIs novas; diferenças restantes imports/rustfmt/assinaturas. WPE compila em fallback GTK nesta box. |
| `crates/ui/examples/right-panel-fixture.rs` | 3 | D3/D4/D9: renderer/fixtures/links/Workers do fork coexistem com APIs novas; diferenças restantes imports/rustfmt/assinaturas. WPE compila em fallback GTK nesta box. |
| `crates/ui/examples/session-card-fixture.rs` | 2 | D3/D4/D9: renderer/fixtures/links/Workers do fork coexistem com APIs novas; diferenças restantes imports/rustfmt/assinaturas. WPE compila em fallback GTK nesta box. |
| `crates/ui/examples/subagent-fold-fixture.rs` | 2 | D3/D4/D9: renderer/fixtures/links/Workers do fork coexistem com APIs novas; diferenças restantes imports/rustfmt/assinaturas. WPE compila em fallback GTK nesta box. |
| `crates/ui/src/account_usage.rs` | 70 | D3: arquivo de anel Usage permanece ausente; quotas e usage_lines do fork continuam em Settings/Details/composer. |
| `crates/ui/src/app_update.rs` | 46 | D8: updater/feed/RestartGate e engine in-process do fork; canais beta/managed novos não ligados. |
| `crates/ui/src/browser/linux/mod.rs` | 24 | D3/D4/D9: renderer/fixtures/links/Workers do fork coexistem com APIs novas; diferenças restantes imports/rustfmt/assinaturas. WPE compila em fallback GTK nesta box. |
| `crates/ui/src/browser/macos.rs` | 2 | D3/D4/D9: renderer/fixtures/links/Workers do fork coexistem com APIs novas; diferenças restantes imports/rustfmt/assinaturas. WPE compila em fallback GTK nesta box. |
| `crates/ui/src/browser/view.rs` | 16 | D3/D4/D9: renderer/fixtures/links/Workers do fork coexistem com APIs novas; diferenças restantes imports/rustfmt/assinaturas. WPE compila em fallback GTK nesta box. |
| `crates/ui/src/composer.rs` | 5 | D3: Usage no presenter do fork e fade do todo já existente; request leva access/plan resolvidos, intake/attachments/chips e steer conservados. |
| `crates/ui/src/pickers.rs` | 3 | D3/D4/D9: renderer/fixtures/links/Workers do fork coexistem com APIs novas; diferenças restantes imports/rustfmt/assinaturas. WPE compila em fallback GTK nesta box. |
| `crates/ui/src/rail.rs` | 9 | D3: cache RailSnapshot do fork/caps no sync conservados; Markdown/caption portados, ranges mono derivados de TextRun em vez de flat.code_ranges. |
| `crates/ui/src/settings/search.rs` | 8 | D3/D8: Projects e Accounts separados de Harnesses; CommitCredit/banner/ring/beta recusados. APIs/fixtures Shell usam Workers model; ícones do catálogo único; writer central preservado. |
| `crates/ui/src/settings/shortcuts.rs` | 4 | D3/D8: Projects e Accounts separados de Harnesses; CommitCredit/banner/ring/beta recusados. APIs/fixtures Shell usam Workers model; ícones do catálogo único; writer central preservado. |
| `crates/ui/src/settings/worktrees.rs` | 1 | D3/D8: Projects e Accounts separados de Harnesses; CommitCredit/banner/ring/beta recusados. APIs/fixtures Shell usam Workers model; ícones do catálogo único; writer central preservado. |
| `crates/ui/src/shell.rs` | 42 | D3/D8: Projects e Accounts separados de Harnesses; CommitCredit/banner/ring/beta recusados. APIs/fixtures Shell usam Workers model; ícones do catálogo único; writer central preservado. |
| `crates/ui/src/shell/settings_search.rs` | 1 | D3/D8: Projects e Accounts separados de Harnesses; CommitCredit/banner/ring/beta recusados. APIs/fixtures Shell usam Workers model; ícones do catálogo único; writer central preservado. |
| `crates/ui/src/shell/side_chats.rs` | 2 | D3/D4/D9: renderer/fixtures/links/Workers do fork coexistem com APIs novas; diferenças restantes imports/rustfmt/assinaturas. WPE compila em fallback GTK nesta box. |
| `crates/ui/src/state.rs` | 4 | D3/D4/D9: renderer/fixtures/links/Workers do fork coexistem com APIs novas; diferenças restantes imports/rustfmt/assinaturas. WPE compila em fallback GTK nesta box. |
| `crates/ui/src/transcript.rs` | 64 | D3/D7: TurnSteps compacto e QuickJS Mermaid do fork; UI-code options adaptadas ao renderer existente; chips URL/menções, MIME e budgets preservados. Cloud boot palavra/status dormente. Novos testes Markdown recuperados e assinaturas/fixtures adaptadas. |
| `crates/update/src/lib.rs` | 260 | D8: updater/feed/RestartGate e engine in-process do fork; canais beta/managed novos não ligados. |
| `crates/visual/src/lib.rs` | 1 | Modelo visual exclusivo do fork preservado; imports/rustfmt das adições compatíveis. |
| `crates/visual/src/tests.rs` | 3 | Modelo visual exclusivo do fork preservado; imports/rustfmt das adições compatíveis. |
| `dist/macos/Info.plist` | 5 | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `edge/package-lock.json` | 428 | D2: diretório/profile/card-web/plugins/Cloud e rotas/testes/bindings associados dormentes; preservar grafo/config/testes do fork. Loro e preview vivos; lock regenerado sem assets do card-web. |
| `edge/package.json` | 5 | D2: diretório/profile/card-web/plugins/Cloud e rotas/testes/bindings associados dormentes; preservar grafo/config/testes do fork. Loro e preview vivos; lock regenerado sem assets do card-web. |
| `edge/src/auth-routes.ts` | 581 | D2: diretório/profile/card-web/plugins/Cloud e rotas/testes/bindings associados dormentes; preservar grafo/config/testes do fork. Loro e preview vivos; lock regenerado sem assets do card-web. |
| `edge/src/device-room.ts` | 27 | D2: diretório/profile/card-web/plugins/Cloud e rotas/testes/bindings associados dormentes; preservar grafo/config/testes do fork. Loro e preview vivos; lock regenerado sem assets do card-web. |
| `edge/src/env.ts` | 19 | D2: diretório/profile/card-web/plugins/Cloud e rotas/testes/bindings associados dormentes; preservar grafo/config/testes do fork. Loro e preview vivos; lock regenerado sem assets do card-web. |
| `edge/src/forward.ts` | 6 | D2: diretório/profile/card-web/plugins/Cloud e rotas/testes/bindings associados dormentes; preservar grafo/config/testes do fork. Loro e preview vivos; lock regenerado sem assets do card-web. |
| `edge/src/index.ts` | 37 | D2: diretório/profile/card-web/plugins/Cloud e rotas/testes/bindings associados dormentes; preservar grafo/config/testes do fork. Loro e preview vivos; lock regenerado sem assets do card-web. |
| `edge/src/preview-route.ts` | 3 | D2: diretório/profile/card-web/plugins/Cloud e rotas/testes/bindings associados dormentes; preservar grafo/config/testes do fork. Loro e preview vivos; lock regenerado sem assets do card-web. |
| `edge/test/workerd/edge-security.workerd.test.ts` | 273 | D2: diretório/profile/card-web/plugins/Cloud e rotas/testes/bindings associados dormentes; preservar grafo/config/testes do fork. Loro e preview vivos; lock regenerado sem assets do card-web. |
| `edge/test/workerd/env.d.ts` | 1 | D2: diretório/profile/card-web/plugins/Cloud e rotas/testes/bindings associados dormentes; preservar grafo/config/testes do fork. Loro e preview vivos; lock regenerado sem assets do card-web. |
| `edge/test/workerd/fixture.ts` | 18 | D2: diretório/profile/card-web/plugins/Cloud e rotas/testes/bindings associados dormentes; preservar grafo/config/testes do fork. Loro e preview vivos; lock regenerado sem assets do card-web. |
| `edge/test/workerd/org-invitations.workerd.test.ts` | 127 | D2: diretório/profile/card-web/plugins/Cloud e rotas/testes/bindings associados dormentes; preservar grafo/config/testes do fork. Loro e preview vivos; lock regenerado sem assets do card-web. |
| `edge/test/workerd/org-settings.workerd.test.ts` | 285 | D2: diretório/profile/card-web/plugins/Cloud e rotas/testes/bindings associados dormentes; preservar grafo/config/testes do fork. Loro e preview vivos; lock regenerado sem assets do card-web. |
| `edge/vitest.workerd.config.ts` | 13 | D2: diretório/profile/card-web/plugins/Cloud e rotas/testes/bindings associados dormentes; preservar grafo/config/testes do fork. Loro e preview vivos; lock regenerado sem assets do card-web. |
| `edge/wrangler.jsonc` | 48 | D2: diretório/profile/card-web/plugins/Cloud e rotas/testes/bindings associados dormentes; preservar grafo/config/testes do fork. Loro e preview vivos; lock regenerado sem assets do card-web. |
| `scripts/ci/test-core.sh` | 7 | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `scripts/dev-demo.sh` | 1 | D8: versão, workspace, engine/CLI, packaging e CI do fork; imports/dependências compatíveis, Cargo lock regenerado. |
| `scripts/ios/build-core.sh` | 2 | Fluxo de build do fork preserva normalização Swift canônica; refresh de header/modulemap aditivo, bindings geradas do core integrado. Link iOS pendente no Mac. |

# Verificação da integração

Código avaliado: `7bdd0cf5efe432f875929217af71a6c7029a219b`. Logs e relatório completo no caminho absoluto do brief, dentro de `.tmp/reports/`. As duas rodadas agregadas com exit 101 na tabela conservam seu status real; scopes válidos reaproveitados e retries afetados estão discriminados.

## Verificação

Cargo nativo, target exclusivo `/home/agent/projects/comet-sync-upstream-20261104/target`; regressões com jobs=2 e sccache. Temporários finais em `.tmp/runtime`, com `GIT_CEILING_DIRECTORIES` para impedir descoberta do checkout pai. Disco conferido antes dos builds; nenhuma compilação continuou com menos de 20 GB. O target fica para o orquestrador limpar.

Bibliotecas da rodada de core (`rust-core-libs-second.log`): doc 218/0, harness 598/0, RPC 41/0, theme 41/0. Esse comando agregado retornou 101 por uma expectativa MCP de default side, depois corrigida; MCP 66/0 e proto são registrados nas rodadas finais abaixo. Não chamar o agregado antigo de verde nem repetir binários válidos apenas como ritual.

| Comando | Exit | Resultado observado / log |
|---|---:|---|
| `cargo check --workspace --all-targets` | 0 |     Finished `dev` profile [unoptimized + debuginfo] target(s) in 2m 20s |
| `cargo fmt --all --check` | 0 | pendente |
| `openspec validate sync-upstream-20261104 --strict` | 0 | Change 'sync-upstream-20261104' is valid |
| `npm -C edge run typecheck` | 0 |  |
| `npm -C edge run test` | 0 | Test Files 14 passed (14); Tests 71 passed (71); Test Files 9 passed (9); Tests 35 passed (35) |
| `cargo test -p zeron-proto --lib` | 0 | test result: ok. 150 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s |
| `cargo test -p zeron-proto -p zeron-mcp --lib --no-fail-fast (MCP 66/0; proto repetido após ajuste da capability)` | 101 | test result: ok. 66 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.43s; test result: FAILED. 149 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s |
| `cargo test -p zeron-engine --test spawn_chat --test worker_wait_state --test runtime_survival --test workspace_files_mutations --test workspace_files_hardening --test workspace_files --test session_publication --test message_queue --test plugin_services --no-fail-fast` | 0 | test result: ok. 38 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 14.10s; test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 19.77s; test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.80s; test result: ok. 6 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 10.22s; test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.13s; test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.56s; test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.29s; test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s; test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.07s |
| `cargo test -p zeron-engine --lib rpc::` | 0 | test result: ok. 37 passed; 0 failed; 1 ignored; 0 measured; 1000 filtered out; finished in 9.95s |
| `cargo test -p zeron-engine --lib source_control` | 0 | test result: ok. 69 passed; 0 failed; 0 ignored; 0 measured; 969 filtered out; finished in 2.03s |
| `cargo test -p zeron-engine --lib tool_output` | 0 | test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 1030 filtered out; finished in 20.25s |
| `cargo test -p zeron-engine --lib fork_startup_keeps_plugin_rpc_dormant` | 0 | test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1037 filtered out; finished in 0.02s |
| `cargo test -p zeron-engine --test e2e whole_output` | 0 | test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 45 filtered out; finished in 0.06s |
| `cargo test -p zeron-engine --test e2e image` | 0 | test result: ok. 3 passed; 0 failed; 2 ignored; 0 measured; 41 filtered out; finished in 3.05s |
| `cargo test -p zeron-engine --lib cursor_usage` | 0 | test result: ok. 12 passed; 0 failed; 1 ignored; 0 measured; 1025 filtered out; finished in 0.03s |
| `cargo test -p zeron-engine --lib agent_accounts` | 0 | test result: ok. 94 passed; 0 failed; 3 ignored; 0 measured; 941 filtered out; finished in 5.54s |
| `cargo test -p zeron-harness --test omp_rpc --test acp_registry --test acp_tool_fixture --no-fail-fast` | 0 | test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.44s; test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.48s; test result: ok. 60 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 1.70s |
| `cargo test -p zeron-workers-unpeel --test worker_initial_briefing --test controller_ipc_credentials --test controller_mcp --no-fail-fast` | 0 | test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s; test result: ok. 60 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 9.67s; test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.11s |
| `cargo test -p zeron-sync --lib` | 0 | test result: ok. 82 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.63s |
| `cargo test -p zeron-sync --test registry_edge -- --ignored (edge local isolado, AUTH_MODE=dev)` | 0 | test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 8.00s |
| `cargo test -p zeron-preview --lib` | 0 | test result: ok. 36 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 2.26s |
| `cargo test -p zeron-plugin-guard -p zeron-preview-label --lib --no-fail-fast` | 0 | test result: ok. 19 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.24s; test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `CARGO_PROFILE_DEV_DEBUG=line-tables-only CARGO_PROFILE_DEV_BUILD_OVERRIDE_DEBUG=false cargo test --manifest-path third_party/zui/Cargo.toml --locked -p gpui --lib closing_punctuation_attached` | 0 | test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 227 filtered out; finished in 0.00s |
| `cargo test -p zeron-ui --lib transcript::tests (222 casos concluídos, rodada interrompida)` | 101 |   process didn't exit successfully: `/home/agent/projects/comet-sync-upstream-20261104/target/debug/deps/zeron_ui-288e0a291a890eaa 'transcript::tests'` (signal: 2, SIGINT: terminal interrupt signal) |
| `target/debug/deps/zeron_ui-288e0a291a890eaa --exact transcript::tests::user_fold_scroll::conversation_width_reflows_streaming_text_without_restarting_animations --nocapture` | 0 | test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 2837 filtered out; finished in 0.25s |
| `target/debug/deps/zeron_ui-288e0a291a890eaa --exact transcript::tests::user_fold_scroll::runway_resizes_in_the_same_layout_without_a_provisional_gap --nocapture` | 0 | test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 2837 filtered out; finished in 0.05s |
| `cargo test -p zeron-ui --lib settings::search` | 0 | test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 2815 filtered out; finished in 0.01s |
| `cargo test -p zeron-ui --lib rail::tests` | 0 | test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 2821 filtered out; finished in 0.00s |
| `target/debug/deps/zeron_ui-288e0a291a890eaa appshots::shortcut::tests --nocapture` | 0 | test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 2835 filtered out; finished in 0.00s |
| `target/debug/deps/zeron_ui-288e0a291a890eaa settings_section_persistence --nocapture` | 0 | test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 2825 filtered out; finished in 0.08s |
| `cargo build -p zeron-mobile --lib (host Linux, não link iOS)` | 0 |     Finished `dev` profile [unoptimized + debuginfo] target(s) in 3m 44s |
| `cargo run -p zeron-mobile --bin uniffi-bindgen --features bindgen -- generate --library target/debug/libzeron_mobile.so --language swift --out-dir .tmp/mobile-bindgen` | 0 | Warning: Unable to auto-format .tmp/mobile-bindgen/zeron_core.swift using swift-format. Please make sure it is installed. |

### Falhas intermediárias e correções

- Checks de integração `check-1` a `check-7`, `check8` a `check10` retornaram 101 durante a reconciliação de tipos/construtores, duplicações de UI e assinaturas. O all-targets final é registrado separadamente.
- Primeiro core: o teste upstream que apagava output de non-command conflitou com o resumo limitado do fork. Expectativas de doc/Codex foram adaptadas; Claude voltou a preservar o texto limitado do fork. Fixtures que usavam Fx como harness desconhecido passaram a usar future-provider. Testes de default subagent foram adaptados ao default side do fork. Igualdade Codex usa acesso efetivo mais grants.
- O teste de fila publicou finish antes da inscrição do harness seguinte; passou a observar o prompt real. Um teste pausado que simulava horas com todos os heartbeats foi interrompido e convertido em avanço explícito de relógio; a suíte runtime_survival depois passou inteira. As fontes das fixtures foram confrontadas com a base; não se atribuiu preexistência de falha sem executar uma baseline.
- A primeira rodada Files com temporários dentro do worktree descobriu o Git externo. Foi repetida com GIT_CEILING_DIRECTORIES; os arquivos vazios de teste criados na raiz foram removidos. As duas APIs de criação e as demais suítes Files passaram depois.
- SourceControl e ACP/OMP tiveram expectativas/wiring corrigidos: cwd e viewer via metadata, extra_mcp nos drivers diretos e exit_code OMP junto da execution metadata. Resultados finais acima.
- UI: a rodada de 224 casos teve a expectativa upstream de gutter 48 px contra 16 px do fork e um fixture montando o mesmo ListState em duas janelas. A rodada foi interrompida; casos concluídos permanecem evidência, e apenas os dois afetados foram repetidos após adaptação das fixtures. Não chamar a rodada interrompida de verde.
- Edge: a primeira execução teve 71 unit passes e 34/35 workerd, com timeout de 5s no drain de 300 wakes. Retry focado passou 4/4; a execução completa posterior passou 71 + 35. A primeira execução continua registrada como falha, sem rótulo de preexistência.
- `cargo test -p gpui` pela raiz retornou 101 por gpui não ser membro do workspace; a tentativa com manifesto próprio e a preservação dos dois fontes estão registradas separadamente.

## Limitações e riscos

- `cargo test --workspace` completo pertence ao CI da PR; não foi executado localmente. Runs do CI são consultados por SHA e jobs/etapas realmente executados; este documento registra a prova local, sem declarar CI verde.
- Linux não prova AppKit, macOS headed/browser focus, packaging/update instalado nem Xcode/Simulator/UIKit. Nenhum app real, dev-demo, provider autenticado ou sessão paga foi lançado como prova.
- WPE WebKit 2.50+ não está instalado nesta box; o build Linux usa fallback WebKitGTK. Não há prova nativa de apresentação dma-buf/WPE.
- zgx/cli/launch e serviços edge adicionais são dormentes/excluídos; a compilação do fork não prova esses produtos alternativos. Plugins retidos são exercitados por fixtures explícitas, sem ativação de produção.
- Bindings geradas em host Linux verificam consistência de interface, não linking iOS. Swift, header e modulemap regenerados são byte a byte iguais aos integrados após a normalização canônica do script (evidência local em `.tmp/reports/bindings-copied.json`). swift-format não está instalado, mas a normalização de trailing whitespace do Swift foi aplicada. Não inferir QA nativa desses bytes.
- `git diff --check` do reparo retorna 0. No intervalo integrado retorna 2 somente pelo whitespace emitido no header FFI gerado; o header conserva os bytes canônicos do gerador, sem edição manual (`diff-check-integrated.log`).
- Há warnings Linux de código dormente/deprecações; não se declarou ausência de warnings nem se atribuiu baseline sem prova.
- `trail` e seu cache não estavam disponíveis; contexto/auditoria usaram a cadeia DOX, Git e fontes exatas. Não houve refresh do grafo.
- A OpenSpec permanece aberta, sem archive; a PR permanece draft, sem merge.

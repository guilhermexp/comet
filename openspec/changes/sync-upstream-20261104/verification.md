# Verificação da integração

Código avaliado: `7bdd0cf5efe432f875929217af71a6c7029a219b`. Logs e relatório completo no caminho absoluto do brief, dentro de `.tmp/reports/`. As duas rodadas agregadas com exit 101 na tabela conservam seu status real; scopes válidos reaproveitados e retries afetados estão discriminados.

## Verificação original (anterior ao D1a)

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

## Follow-up CI e D1a — 2026-11-05

Brief fechado: `.tmp/briefs/comet-sync-ci-d1a-20261105.md`; relatório novo `.tmp/reports/comet-sync-ci-d1a-20261105.md`. O teste histórico `fork_startup_keeps_plugin_rpc_dormant` acima documenta o contrato anterior: D1a agora exige ListPlugins no startup e mantém zgx/Inset/split e serviços adicionais do edge dormentes.

- F1/F2: `git show 35561ead:<path>` e cargo metadata confirmam que geometry pertence a orb, cinco nomes de testes da engine são módulos de misc/main.rs, e voice_orb_geometry não existe no upstream. Adicionado zeron-orb ao core e removidos os alvos inválidos, preservando misc e UI --lib. Auditoria expandida: 31 argumentos --test válidos; nenhum alvo perdido no merge. Os erros de seleção anteriores retornaram 101.
- F3: RED próprio 3c80e897f (`[]` versus `[FullAccess]`), correção 7bcfba666. Descriptor/driver OMP FullAccess e sem plan; teste upstream não foi afrouxado e Kimi continua não executável.
- F4: RED próprio 662250fea e reprodução do teste original (`Join` versus `Seed`). create_chat havia semeado epoch 2 local antes da fixture reescrever apenas room_gen. Correção 49c963597 importa row legacy sem snapshot moderno; precedência do epoch e asserções dos wakes/outbox mantidas, sem ignore/retry/sleep.
- D1a: RED próprio 75df06609 (`Plugins aren't available here`), correção 847e42233 liga start/auth/links/RPC. Templates conservam assets locais como precondição; não se instala a shell zgx. Não há gate separado de capability nos clientes de plugin/template; ZGX_INTERFACE_V1 anuncia a interface beta completa e continua ausente. set_plugins apenas retém o seam MCP sem consumidor e fica desligado. drive_run e grants não mudaram. Registro /plugin-keys já degrada com log; novo teste local cobre 404/501/rede preservando auth dev e listagem local.
- GREEN agregado: 43/43 no nextest de registry + sync_lifecycle + startup/falhas de identidade; exit 0. Grants e serviço entre engines: cargo test --test spawn_chat --test plugin_services, 3 + 1 passaram, exit 0. A fixture de serviço agora usa startup normal, sem wiring duplicado.
- F5: E0425 kill/pid_t reproduzido em preview Windows, exit 101. Correção 42262b356 mantém SIGTERM só em Unix e usa ToolHelp + handles/creation times + TerminateProcess para árvore de servidores existentes no Windows. A API pública de Job do harness só cobre processos atribuídos no spawn, sem adoção de árvore externa existente. Cross-check preview no target final: exit 0. Execução nativa e descoberta Windows não foram provadas (descoberta já inativa na baseline).

### Exceções com prova da baseline 3f866aeb9

`git archive 3f866aeb9` gerou cópia isolada dentro de .tmp, sem alterar outro checkout. Todos os builds dela usam target-baseline-3f866aeb9 exclusivo; jobs=2/sccache e espaço livre conferidos.

- Check Windows completo dos pacotes do job native-platform-check (`-p zeron-harness -p zeron-engine`): exit 101 por 44 erros Unix do vendor Unpeel. Baseline `cargo check --locked --target x86_64-pc-windows-gnu -p unpeel-core`: exit 101 com as mesmas 44 mensagens. Tree hash idêntico 525110c5b873a71c1e39ba64e7a748dfaf1c2c80. Logs f5-final.log, f5-unpeel-baseline.log e comparação. O primeiro ensaio também registrou g++ ausente; toolchain C/C++ foi extraída em .tmp/windows-toolchain e resolveu esse obstáculo sem instalação no sistema.
- Comando exato core-tests de ui-tests.yml, `scripts/ci/test-core.sh`: exit 100, 2640 executados, 2638 passed (3 leaky), 2 failed e 27 skipped. Ambos failed são Codex Voice: external_voice_keeps_canonical_events_without_any_local_helper e idle_voice_runtime_has_no_initial_turn_and_preserves_mcp, Unsupported versus NativeRuntimeUnavailable. Baseline `cargo test --locked -p zeron-harness --features native-fixture --test codex voice_`: exit 101, os mesmos dois failed nas linhas 2693/2569. Logs core-final.log e core-codex-voice-baseline.log. Nenhum novo ignore/retry/skip.

- Comando exato ui-tests: `cargo nextest run --config-file scripts/ci/nextest.toml --locked --no-fail-fast -p zeron-ui --lib`: exit 100, 2826 executados, 2823 passed, 3 failed, 12 skipped. Os failed são user_message_url_chips_follow_the_chat_link_preference (panel_interaction_tests.rs:1879, “the user bubble's text”), the_index_and_the_pages_agree (settings_search.rs:660, General com labels ausentes da busca) e sidebar_metadata_short_branch_is_adjacent_to_projects (spaces.rs:548, “row”). Baseline 3f866aeb9, nextest --lib com filtro dos três: exit 100, os mesmos 3 failed, mesmas linhas/mensagens. Logs ui-final.log e ui-baseline.log. O seletor F2 foi corrigido, mas a suíte ainda tem essas falhas anteriores ao follow-up; nenhum código UI foi alterado.

- `cargo test --workspace`: exit 101 ao chegar a `tests/acp_registry_rpc.rs::registry_agents_are_listed_added_as_harnesses_and_removed`, linha 269: updated automatically, left 2.0.0, right 2.1.0. Antes disso a lib da engine passou 1034 testes, 0 failed, 7 ignored. Baseline 3f866aeb9 `cargo test --locked -p zeron-engine --test acp_registry_rpc`: exit 101, mesma falha/linha/valores. Logs workspace-final.log e workspace-acp-baseline.log. Nenhum updater/feed foi editado; a execução fail-fast não prova os testes posteriores do workspace.

- Passo vendorizado exato: `cargo test --manifest-path third_party/unpeel/crates/Cargo.toml -p unpeel-core`, exit 101: 735 passed, 2 failed, 2 ignored. Schema de sessions tem 4118 bytes (mcp_host.rs:3139); baseline com comando idêntico retorna 101, 736 passed/1 failed/2 ignored pelo mesmo schema. Grok light falhou com ETXTBSY em setup_conformance_tests.rs:2467:14. Cinco execuções integrais adicionais da baseline mantiveram o schema e não reproduziram o Grok; a reprodução focada do binário compilado da baseline (filtro grok_command_wrapper) mostrou ETXTBSY em outro wrapper na invocação 5 e no mesmo Grok light na invocação 9 da série dedicada. Logs preservam todas as execuções/exit; nenhum retry foi acrescentado ao teste ou workflow e o gate original permanece 101. Vendor/tree e lock não foram editados. Logs unpeel-final.log, unpeel-baseline.log, unpeel-baseline-repro-*.log e unpeel-grok-light-baseline-reproductions.log.

`cargo check --workspace --all-targets --locked`: exit 0, Finished dev profile em 7m 57s (check-final.log). Código funcional avaliado: 42262b356; o fechamento posterior só altera documentação e comentários do script, sem alterar argumentos de teste. Fmt do workflow e OpenSpec strict no closeout: exit 0 (fmt-workflow-final.log e openspec-closeout.log). Não houve dispatch, push, merge, tags ou alteração da PR; OpenSpec permanece aberta. Não há clippy nos jobs rust.yml/ui-tests.yml solicitados. Não há trail executável/índice nesta box; DOX e Git forneceram o contexto.

## CI integral — rodada 2 (2026-11-05)

A exceção de baseline anterior foi revogada. Gates agregados ainda pendentes nesta rodada.

### R1 — Codex Voice

`35561ead` resolve somente o helper adjacente (`installed_helper`); o fork documenta `media_helper`, incluindo instalações oficiais ativas em CODEX_HOME. Pacote incompatível deve retornar Unsupported, ausência deve retornar NativeRuntimeUnavailable. Os testes de ausência herdavam o ambiente real. RED `b5b6d9e6d` cria um pacote incompatível em HOME/CODEX_HOME privados e executa ambos os fixtures: exit 101, Unsupported. GREEN isola cada fixture em subprocesso com HOME/CODEX_HOME vazios e remove apenas do filho o override de helper: os três casos passam, mantendo todas as asserções originais. Logs r1-red.log/r1-green.log no relatório round2; nenhuma mudança de produto.

### R4 — orçamento do schema

RED no binário da árvore vendor original: sessions 4118 bytes, exit 101. Introdução anunciada reduzida; actions, propriedades, vocabulário/limites de keys, help e tetos (<4 KiB/domínio e <11 KiB total) intactos. GREEN: `cargo test --manifest-path third_party/unpeel/crates/Cargo.toml -p unpeel-core --lib mcp_host::tests::`, exit 0, 37 passed. Prova downstream e suíte completa integram o gate final após os demais patches do mesmo vendor.

### R5 — publicação dos wrappers

RED f84793e97: o helper truncava o inode publicado; leitor aberto passou a enxergar o novo script (exit 101). ETXTBSY também foi reproduzido pela série antiga de Grok na invocação 9 (log da rodada anterior). GREEN: staging UUID no mesmo diretório, escrita/chmod pelo descritor, close antes de rename e cleanup em erro. Regressão do inode passa; 30/30 invocações consecutivas do mesmo filtro `grok_command_wrapper --nocapture` passaram (120 testes, sem retry/sleep), exit 0. Logs r5-red.log/r5-green.log/r5-grok-green-series.log.

### R3 — ACP registry

A fixture já era loopback: não havia dependência do registry vivo. Três probes do caminho de apply (espera de lease, posse exclusiva e owner gate) chamavam o CLI diretamente, embora ACP não prometa CLI de versão. `installed_version_of` já resolve pacotes ACP pelo registro gerenciado e continua delegando os demais CLIs ao probe anterior; os três pontos agora usam esse resolvedor. RED original exit 101 (2.0.0 após espera); RED fe0631e06 usa watch RPC com o mesmo orçamento de 10s e falha por não concluir. GREEN final `cargo test --locked -p zeron-engine --test acp_registry_rpc -- --nocapture`: exit 0, atualização automática 2.0.0 → 2.1.0, 1 passed. A tripwire cobre o coordenador; uma probe opcional da descoberta de catálogo é eliminada do marcador antes dessa fronteira (observada no log). Asserções originais de versão, fase, arquivos, checksum e remoção preservadas. Nenhuma mudança no updater/feed do app.

### R2 — UI

RED 7f8f85520: 3 falhas na seleção de quatro testes (exit 100); série adicional reproduziu a sidebar na invocação 2. O índice não incluía títulos do fork e havia anchors ausentes (Compact mode, depois Voice/Computer Use revelados pela checagem completa). Busca agora usa os títulos visíveis e âncoras reais, com alias de Thread naming preservado. Os títulos duplicados na captura provinham de dois frames, não de duas rows no produto. Chip URL já roteava corretamente: faltava seu debug selector, restaurado sem alterar o clique/preferência.

Sidebar: todos os selectors do subtree desapareciam juntos após replay da cena cacheada; a ordem visível ainda continha normal. Refresh isolado não resolveu (série falhou na invocação 5). O fixture agora usa o renderer real sem cache, observer do Shell e o mesmo handler OpenPullRequest do Shell (extraído sem alterar a lógica). Todas as asserções originais de largura/gap/containment/clique preservadas. `cargo nextest run --config-file scripts/ci/nextest.toml --locked --no-fail-fast -p zeron-ui --lib`: exit 0, 2827 passed, 12 ignores anteriores, sem retry overrides. Logs gate-ui-first.log e r2-*; a série final mede 40 invocações completas, sem retry/sleep.

### R6 — Windows do par harness/engine

RED: comando exato do native-platform-check acrescido do target GNU (`cargo check --locked --target x86_64-pc-windows-gnu -p zeron-harness -p zeron-engine`), exit 101, 44 erros do vendor. Após corrigi-los, o mesmo comando revelou 30 erros da fronteira Workers e incompatibilidades OMP de Command/Child/pipes; todos corrigidos, sem remover pacote/alvo.

Core e consumidor usam o seam nativo `unpeel_core::platform`: flock/0600 Unix; LockFileEx, DACL/SID do processo, reparse-point refusal, write-through rename, hostname e handles/creation time Win32. APIs POSIX de hooks/PTY/socket/FIFO recebem cfg; onde o Host local não existe a chamada retorna erro explícito (não sucesso). OMP usa os tipos do harness e Windows kill termina o Job e espera o root. Arquivos Cargo lock incluem windows-sys para o core e a resolução exigida por --locked, sem bump do app/MSRV/registro de versões. Base upstream preservada.

GREEN r6-green-fourth.log: mesmo par GNU, exit 0, 52.70s. Linux `cargo test --manifest-path third_party/unpeel/crates/Cargo.toml -p unpeel-core --lib platform::tests`: exit 0 (três testes novos + um parser existente selecionado pelo nome). Downstream real `cargo test --locked -p zeron-workers-unpeel`: exit 0, 542 testes Rust + quatro etapas do runner worker_restart_identity, zero ignores. Isso também prova o consumidor exigido pelos patches R4/R5. Execução, GUI/Direct3D/instalador Windows não foram provados nesta box Linux.


### Falha adicional do gate — pilha dos probes

O primeiro `scripts/ci/test-core.sh` da rodada saiu 100: 2638 passaram e três testes de substituição/cancelamento de atualização abortaram por stack overflow. A chamada ao resolvedor de versão gerenciado revelou buffers de leitura de 16 KiB mantidos através de await, contrários ao contrato da engine. RED `de45d7a45`: `version_probe_futures_hold_no_bulk_buffer` sai 101 e mede read_tail=16448, run_command_output_env=33728, installed_version_of=33936 bytes. O teto é o mesmo 8 KiB das capturas diff existentes. Correção coloca o chunk no heap, mantendo tamanho, cap de saída, timeout e pilha; GREEN: 61 testes de atualização passaram, incluindo os três aborts anteriores e o budget dos futures (exit 0, gate-probe-stack-green.log). Gates agregados em execução.

Os três casos leaky do core foram reportados como PASS pelo próprio nextest; a configuração do workflow não reprova leaks. O exit 100 veio dos três aborts de pilha, não desses casos. Removidos os dois retry overrides antigos de nextest: nenhum resultado desta rodada é aceito por retry.


### Falhas adicionais do workspace — sem exceção de baseline

`cargo test --workspace --locked --no-fail-fast` (gate-workspace-second.log) executou todos os alvos e saiu 101: cloud_rpc (1), cloud_runner (2), e2e (1), runtime_survival (1), cursor (2). O log ficou aberto pelo stderr de um Python órfão da fixture discovery; depois de conferir cwd privado e identidade do pipe, apenas esse servidor foi encerrado, preservando o exit 101 do gate.

- Git broker: RED f8ffa83f1 sai 101 por modificar configuração de outro HOME privado via GIT_CONFIG_GLOBAL. GREEN ab63f0377 compartilha a política de HOME/XDG/global entre helper e identidade; overrides legítimos do próprio HOME permanecem. Leitores dos testes também são privados. 11 testes unitários e 24 cloud_runner passaram, exit 0; grants/serviços dormentes preservados.
- Cloud RPC: 6502990c7 corrige expectativa incompatível com D5 local-only. Exige recusa de targetDeviceId antes da rede e mantém Ready/vault/bearer nas chamadas locais. Sete testes passaram, exit 0.
- Interrupt: 7769e61c9 observa mensagem Aborted e comando Applied via eventos do doc, dois commits distintos, antes de verificar journal/Idle. Mesmas asserções e deadline 10s; 44 e2e passaram, 2 ignores antigos, exit 0.
- Cursor: da3dea064 remove dependência de ordem de chaves JSON na fixture. RED direto: imagem válida em ordem alfabética sai 0, em ordem de inserção sai 1. GREEN com serde_json/preserve_order: 23/23 passaram. Matriz de duas ordens × seis payloads conserva recusas por conteúdo/tipo/mime/cardinalidade inválidos; 12 verificações passaram. Nenhuma mudança de driver ou orçamento de imagens.
- Discovery: RED bacce884e exige EOF ao soltar launcher e sai 101; shell morria e Python ficava com o pipe. GREEN b8ea6075a dá grupo Unix próprio aos guards e encerra shell + descendentes. Os cinco testes passaram (30,21s), exit 0, sem orfandade do servidor scrubbed.
- Stop: eae0c7e4e troca o atraso fixo por outcomes duráveis e revela Superseded indevido (série falha na invocação 4; captura ampliada na 150). RED determinístico e0e50e40d sai 101: mensagem User não é turno Assistant. Produtores engine/client e contexto do host agora escolhem Assistant; row User legada não vira turno concluído. Três testes de fence/contexto passaram, 18 de runtime passaram e série pós-correção passou 200/200, sem retry/sleep. O teste também conserva Superseded para Assistant realmente anterior. Cliente completo: exit 0.

Logs gate-broker-home-*, gate-cloud-and-interrupt-green.log, gate-cursor-json-order-*, gate-discovery-leak-*, gate-stop-context-*, gate-stop-runtime-green.log, gate-stop-client-green.log, gate-steer-stop-*.log. Gates agregados serão repetidos após essas correções; o exit 101 acima não é aceito.

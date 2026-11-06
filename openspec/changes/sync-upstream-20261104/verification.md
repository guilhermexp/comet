# Verificação da integração

Histórico da integração e dos follow-ups. Código da verificação original: `7bdd0cf5efe432f875929217af71a6c7029a219b`. O fechamento da rodada 2, ao fim deste documento, substitui as exceções e pendências das rodadas anteriores; seus exits históricos continuam preservados. Logs e relatório completo no caminho absoluto do respectivo brief, dentro de `.tmp/reports/`.

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

- Na verificação original, `cargo test --workspace` completo ainda não havia sido executado localmente; as execuções completas da rodada 2 estão registradas abaixo. A prova local não declara CI remoto verde.
- Linux não prova AppKit, macOS headed/browser focus, packaging/update instalado nem Xcode/Simulator/UIKit. Nenhum app real, dev-demo, provider autenticado ou sessão paga foi lançado como prova.
- WPE WebKit 2.50+ não está instalado nesta box; o build Linux usa fallback WebKitGTK. Não há prova nativa de apresentação dma-buf/WPE.
- zgx/cli/launch e serviços edge adicionais são dormentes/excluídos; a compilação do fork não prova esses produtos alternativos. D1a ativou plugins/templates no startup normal durante o follow-up abaixo, preservando grants e a ausência de MCP de plugin nas runs.
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

A exceção de baseline anterior foi revogada. Falhas intermediárias não são aceitas; resultados finais e limites de plataforma estão no fechamento desta rodada abaixo.

### R1 — Codex Voice

`35561ead` resolve somente o helper adjacente (`installed_helper`); o fork documenta `media_helper`, incluindo instalações oficiais ativas em CODEX_HOME. Pacote incompatível deve retornar Unsupported, ausência deve retornar NativeRuntimeUnavailable. Os testes de ausência herdavam o ambiente real. RED `b5b6d9e6d` cria um pacote incompatível em HOME/CODEX_HOME privados e executa ambos os fixtures: exit 101, Unsupported. GREEN isola cada fixture em subprocesso com HOME/CODEX_HOME vazios e remove apenas do filho o override de helper: os três casos passam, mantendo todas as asserções originais. Logs r1-red.log/r1-green.log no relatório round2; nenhuma mudança de produto.

### R4 — orçamento do schema

RED no binário da árvore vendor original: sessions 4118 bytes, exit 101. Introdução anunciada reduzida; actions, propriedades, vocabulário/limites de keys, help e tetos (<4 KiB/domínio e <11 KiB total) intactos. GREEN: `cargo test --manifest-path third_party/unpeel/crates/Cargo.toml -p unpeel-core --lib mcp_host::tests::`, exit 0, 37 passed. Prova downstream e suíte completa do vendor passaram, registradas em R6 e no fechamento abaixo.

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

O primeiro `scripts/ci/test-core.sh` da rodada saiu 100: 2638 passaram e três testes de substituição/cancelamento de atualização abortaram por stack overflow. A chamada ao resolvedor de versão gerenciado revelou buffers de leitura de 16 KiB mantidos através de await, contrários ao contrato da engine. RED `de45d7a45`: `version_probe_futures_hold_no_bulk_buffer` sai 101 e mede read_tail=16448, run_command_output_env=33728, installed_version_of=33936 bytes. O teto é o mesmo 8 KiB das capturas diff existentes. Correção coloca o chunk no heap, mantendo tamanho, cap de saída, timeout e pilha; GREEN: 61 testes de atualização passaram, incluindo os três aborts anteriores e o budget dos futures (exit 0, gate-probe-stack-green.log). Resultados dos gates agregados constam do fechamento abaixo.

Os três casos leaky do core foram reportados como PASS pelo próprio nextest; a configuração do workflow não reprova leaks. O exit 100 veio dos três aborts de pilha, não desses casos. Removidos os dois retry overrides antigos de nextest: nenhum resultado desta rodada é aceito por retry.


### Falhas adicionais do workspace — sem exceção de baseline

`cargo test --workspace --locked --no-fail-fast` (gate-workspace-second.log) executou todos os alvos e saiu 101: cloud_rpc (1), cloud_runner (2), e2e (1), runtime_survival (1), cursor (2). O log ficou aberto pelo stderr de um Python órfão da fixture discovery; depois de conferir cwd privado e identidade do pipe, apenas esse servidor foi encerrado, preservando o exit 101 do gate.

- Git broker: RED f8ffa83f1 sai 101 por modificar configuração de outro HOME privado via GIT_CONFIG_GLOBAL. GREEN ab63f0377 compartilha a política de HOME/XDG/global entre helper e identidade; overrides legítimos do próprio HOME permanecem. Leitores dos testes também são privados. 11 testes unitários e 24 cloud_runner passaram, exit 0; grants/serviços dormentes preservados.
- Cloud RPC: 6502990c7 corrige expectativa incompatível com D5 local-only. Exige recusa de targetDeviceId antes da rede e mantém Ready/vault/bearer nas chamadas locais. Sete testes passaram, exit 0.
- Interrupt: 7769e61c9 observa mensagem Aborted e comando Applied via eventos do doc, dois commits distintos, antes de verificar journal/Idle. Mesmas asserções e deadline 10s; 44 e2e passaram, 2 ignores antigos, exit 0.
- Cursor: da3dea064 remove dependência de ordem de chaves JSON na fixture. RED direto: imagem válida em ordem alfabética sai 0, em ordem de inserção sai 1. GREEN com serde_json/preserve_order: 23/23 passaram. Matriz de duas ordens × seis payloads conserva recusas por conteúdo/tipo/mime/cardinalidade inválidos; 12 verificações passaram. Nenhuma mudança de driver ou orçamento de imagens.
- Discovery: RED bacce884e exige EOF ao soltar launcher e sai 101; shell morria e Python ficava com o pipe. GREEN b8ea6075a dá grupo Unix próprio aos guards e encerra shell + descendentes. Os cinco testes passaram (30,21s), exit 0, sem orfandade do servidor scrubbed.
- Stop: eae0c7e4e troca o atraso fixo por outcomes duráveis e revela Superseded indevido (série falha na invocação 4; captura ampliada na 150). RED determinístico e0e50e40d sai 101: mensagem User não é turno Assistant. Produtores engine/client e contexto do host agora escolhem Assistant; row User legada não vira turno concluído. Três testes de fence/contexto passaram, 18 de runtime passaram e série pós-correção passou 200/200, sem retry/sleep. O teste também conserva Superseded para Assistant realmente anterior. Cliente completo: exit 0.

Logs gate-broker-home-*, gate-cloud-and-interrupt-green.log, gate-cursor-json-order-*, gate-discovery-leak-*, gate-stop-context-*, gate-stop-runtime-green.log, gate-stop-client-green.log, gate-steer-stop-*.log. As reexecuções agregadas estão discriminadas no fechamento abaixo; o exit 101 acima não é aceito.

Nova falha do workspace (terceira execução, exit 101): somente `zeron --bin zeron`, `second_launch_never_rotates_a_live_processes_log`, assertion rotation resumes. O teste compartilha processo com installer que faz fork: flock pertence ao open-file-description, retido no filho até exec/exit mesmo com CLOEXEC. Prova sincronizada por pipes, sem sleep/retry: gate-log-fork-contract.json. Fixture agora roda uma vez em subprocesso privado, preserva todas as asserções e exige CLOEXEC no descritor real. Na quarta execução do workspace os 16 testes do binário e o gate completo passaram; as reexecuções posteriores constam do fechamento abaixo. Logger de produto, orçamento e comportamento de rotação intactos.

Isolamento do browser: o exemplo upstream chamava test_shell, que fora de cfg(test) cria WorkersModel vivo. Antes de qualquer lançamento, a fixture foi ligada ao detached_fixture_shell já existente (monitor desabilitado, sem engine/polling de Workers), também exportado sob browser-fixture. Comando do workflow e todas as verificações nativas preservados. Primeiro build foi interrompido para essa correção; segundo revelou cfg ausente em WorkersModel::detached (E0599), corrigido no terceiro. A rodada after-ipc de browser/storage Linux passou, incluindo X11/Wayland e OpenGL forçado; os resultados finais após o reparo da fila constam do fechamento abaixo. DOX passa a documentar a apresentação explícita Linux refresh/draw/present_if_needed, que permite esse exemplo com gpui/bench.

UI final após remover retries: exit 100, 2826 passed / 1 failed (bootstrap local) / 12 ignores anteriores. O shutdown já aguardava JoinHandle; diagnóstico confirmou TCP real (portas distintas). Strace mostrou fork em thread de warm-up; a reprodução controlada mantém o descritor no filho antes de exec. RED cb39c33d1: cancelar e aguardar um acceptor ainda permite conexão, exit 101. O primeiro rascunho do teste não compilou por tipo JSON não qualificado; foi corrigido antes do RED válido. GREEN de RPC: guard exclusivo construído antes de retornar o future, com shutdown do socket Linux antes do close; cobre scoped e compat mesmo sem primeiro poll. Os 42 testes RPC passaram, inclusive bearer/Origin/agent e mensagens grandes, exit 0 (gate-ipc-fork-green.log). Não afrouxa a asserção UI nem aumenta timeout. Bootstrap isolado e 200 execuções consecutivas passaram (exit 0), assim como a UI completa after-ipc; os gates finais constam do fechamento abaixo.


Skill Usage: a execução completa após IPC revelou uma falha em `project_skill_usage_review_f3_normal_import_after_scan`: somente `workedProjects.historyVersion` mudou, de forma legítima pelo produtor independente. `c8294f6bf` encerra e aguarda WorkedProjectsSync antes de capturar o registry para a prova de somente leitura de Skill Usage; conserva igualdade integral de Chats/Sessions e import real. RED extraído de gate-workspace-after-ipc.log em gate-skill-usage-read-only-red.log; GREEN `cargo test --locked -p zeron-engine --test project_skill_usage --test worked_projects`: exit 0, 9 + 4 passed. Os demais testes e a suíte Worked Projects conservam o startup completo. Não há mudança de comportamento de produto.


Publicação da fila: `cargo test --workspace` após IPC saiu 101 em `message_queue::cancelling_a_turn_freezes_the_queue_until_an_explicit_send`, primeiro queue_message com ContainerDeleted. RED `97f59f01a` observa append com attachments=0/hold=false antes do fim da escrita (exit 101, gate-queue-publication-red.log). GREEN `73db07cb3` preenche maps destacados antes de append/restore, mantendo shape sincronizado e políticas de entrega. Os 14 testes da fila passaram (gate-queue-publication-green.log, exit 0), incluindo leitura concorrente nos dois caminhos. Novo Scenario unit na change e DOX da crate doc registram o contrato; os resultados dos gates sobre essa correção constam do fechamento abaixo.

## Fechamento da rodada 2 — recibos finais

Última alteração de código/testes avaliada: `73db07cb3`. O fechamento em `8f4f86ec0` alterou apenas documentação; a revisão posterior de teste está registrada ao fim deste documento. R1–R6 e as falhas locais adicionais foram corrigidos; nenhuma falha é dispensada por baseline. Logs em `/home/agent/projects/comet-sync-upstream-20261104/.tmp/reports/comet-sync-ci-round2-20261105-logs/`; relatório `/home/agent/projects/comet-sync-upstream-20261104/.tmp/reports/comet-sync-ci-round2-20261105.md`.

| Comando | Exit | Log |
|---|---:|---|
| `cargo test --workspace` | 0 | gate-rust-workflow-final-queue.log |
| `cargo test --workspace --locked --no-fail-fast` | 0 | gate-workspace-final-queue.log |
| `scripts/ci/test-core.sh` | 0 | gate-core-final-queue.log |
| `cargo nextest run --config-file scripts/ci/nextest.toml --locked --no-fail-fast -p zeron-ui --lib` | 0 | gate-ui-final-queue.log |
| `cargo test --manifest-path third_party/unpeel/crates/Cargo.toml -p unpeel-core` | 0 | gate-vendor-core-exact.log |
| `cargo test --manifest-path third_party/unpeel/crates/Cargo.toml --no-fail-fast` | 0 | gate-vendor-workspace-second.log |
| `cargo check --workspace --all-targets --locked` | 0 | gate-check-workspace-final-queue.log |
| `cargo fmt --all -- --check` | 0 | gate-fmt-final.log |
| `cargo check --locked --target x86_64-pc-windows-gnu -p zeron-harness -p zeron-engine` | 0 | gate-windows-gnu-final-queue.log |
| `cargo check --locked -p zeron-harness -p zeron-engine` | 0 | gate-native-linux-final-queue.log |
| `cargo test --locked -p zeron-voice-media --lib` | 0 | gate-native-resolution-after-ipc.log |
| `cargo test --locked -p zeron-text -p zeron-markdown -p zeron-client -p zeron-mobile` | 0 | gate-mobile-rust-final-queue.log |
| `cargo build --locked -p zeron-ui --example browser-fixture --features browser-fixture` | 0 | gate-browser-build-final-queue.log |
| `xvfb-run -a python3 scripts/test-browser-storage.py --fixture target/debug/examples/browser-fixture` | 0 | gate-browser-storage-final-queue.log |
| `scripts/test-linux-browser.sh "$RUNNER_TEMP/linux-browser/default"` | 0 | gate-browser-default-final-queue.log |
| `VK_DRIVER_FILES=/dev/null scripts/test-linux-browser.sh "$RUNNER_TEMP/linux-browser/opengl"` | 0 | gate-browser-opengl-final-queue.log |
| `python3 -m unittest scripts/ci/test_select_ui_tests.py` | 0 | gate-selector-final.log |
| `node --test scripts/ci/release-profile-gate.test.cjs` | 0 | gate-release-profile-final.log |
| `python3 scripts/ci/select-ui-tests.py --all --github-output "$GITHUB_OUTPUT"` | 0 | gate-selector-all-final.log |
| `openspec validate sync-upstream-20261104 --strict` | 0 | gate-openspec-final.log |

UI: 2827 passed/12 ignores anteriores. Core: 2646 passed/27 ignores anteriores, 2 leaky PASS; o CI configurado não reprova leaks. Discovery não é mais leaky; os dois casos jsonrpc restantes são discriminados no relatório. Os dois retry overrides antigos foram removidos. Os dois comandos de workspace executaram os testes, sem se limitar a compilá-los. Vendor completo: sete membros, 1047 passed/2 ignores anteriores; comando exato core: 741 passed/2 ignores anteriores. Árvore vendor 3ccbca1ed6fe31b903a3de54f33df05cc292684c idêntica à proveniência; sua suíte completa é prova reutilizável pois nenhuma alteração posterior toca esse workspace independente.

Browser final: build/storage e default/OpenGL × X11/Wayland passaram, com Shell detached e inputs/DOM/clipboard/IME/resize/sidebar reais. Todos os booleanos dos quatro linux-results.json são true; screenshots, vídeos e result.txt PASS preservados em linux-browser-final-queue/. Renderer llvmpipe, sem inferir GPU física, WPE ou AppKit. Rust puro mobile passou no Linux, sem inferir linking/QA iOS.

**Aceite global de 6.7 continua incompleto:** windows.yml (UI/fontes/shader, engine/auth/paths, app/packaging/instalador/startup e GUI condicional) e a projeção/testes de runtime Voice em Windows nativo não foram executados. O par GNU harness+engine saiu 0, mas não substitui runtime Windows/MSVC/GUI. Consulta berth somente leitura encontrou apenas a box agents-us, Linux amd64; nenhum runner Windows disponível e nenhum dispatch permitido/executado. Esses comandos ficam N/E, sem exit 0 atribuído. Jobs macOS/iOS ficam “provado só no CI remoto”, conforme brief; não existe recibo remoto novo para estes commits. Inventário literal dos 52 shell steps e status por plataforma em workflow-commands.md e workflow-local-coverage.json.

DOX donos e matrizes, design/specs/tasks/conflicts reconciliados. App 1.0.16, root Cargo.toml, edge/, updater/feed do app e grants/MCP preservados; nenhum novo ignore/skip/retry/sleep ou alvo de workflow removido. git diff --check da rodada sai 0. Nenhum push, merge, tag, alteração de PR ou dispatch. OpenSpec permanece aberta, sem archive por instrução do dono. Trail continua indisponível nesta box (127), sem refresh fictício de grafo.


## Revisão OpenQodex — gate Unix da regressão R5 (2026-11-05)

`executable_script_replacement_preserves_the_published_inode` precisava de `cfg(unix)` por usar PermissionsExt e verificar bits POSIX. Gate aplicado à função inteira, mantendo todas as asserções e os testes neutros ativos. A auditoria de std::os::unix/std::os::fd/libc nas adições e testes alterados da rodada (d710ad591..8f4f86ec0) encontrou apenas essa omissão: os demais acessos têm cfg de arquivo, módulo, função ou bloco; matriz em review-unix-test-audit.json nos logs round2.

Teste Linux filtrado: exit 0, 1 passed. Check GNU `cargo check --manifest-path third_party/unpeel/crates/Cargo.toml --locked --target x86_64-pc-windows-gnu -p unpeel-core --tests`: exit 101 antes (127 diagnósticos) e depois (125). Os dois diagnósticos dessa função desapareceram. Os restantes são fixtures de hooks (110), Host (3), session_ops (5), browser_mcp (3), relay_uplink (2) e helpers de transcripts (2), que ainda importam símbolos/transportes Unix indisponíveis nessa configuração. O check idêntico com --lib no lugar de --tests saiu 0. Não se alega compilação dos testes Windows nem execução nativa. Logs review-unix-windows-before-toolchain.log, review-unix-windows-after.log, review-unix-windows-lib.log e review-unix-linux-test.log; primeiro ensaio sem o PATH do toolchain local falhou por gcc ausente, resolvido usando o toolchain já extraído nesta tarefa.

Nova árvore vendor b71f4031eb8288056b88df54edb51c3661c2bbfc, base upstream e contador histórico preservados; nenhum fonte de produção ou workflow mudou. O relatório round2 recebe apêndice com o commit desta revisão e a prova downstream, sem reescrever os recibos históricos acima.


## Revisão de quatro achados — snapshot 17dabf6a0 (2026-11-05)

- Drain: RED a7fb8e349 falha ao devolver Pending de uma claim de fundo como conclusão. GREEN e5adb1dae observa status terminal via watch assinado antes da leitura, com deadline de 3 s já usado nos testes doc_host. O caso real depende dos drains de fundo, conclui early antes do cenário seguinte e preserva Applied para Stop no Assistant atual/Superseded para Assistant antigo. Quatro testes focados passaram; nenhuma lógica de produção ou timeout agregado foi alterado.
- ACP: RED f4f8dca33 tem dois controles negativos falhando (marker 2.1.0 ignorado e header ausente aceito), enquanto o controle positivo já confirma que os dois artefatos reais retornam 64 e escrevem marker. GREEN 86f053213 exige o shebang LF, verifica markers de 2.0.0 e 2.1.0 após update e após recheck Current, e mantém todos os asserts de versão/arquivos/checksum/remoção. Quatro testes passaram. O shebang do arquivo existente era correto; agora essa precondição é explícita.
- Voice: RED 0d1370c73 captura CODEX_HOME diferente do selecionado pelo pai, expondo a segunda isolação. GREEN be9aad878 passa o marker de isolamento, gateia o controle externo em Unix e executa cada fixture nas instalações incompatível e ausente. O filho exige o CODEX_HOME exato e a presença/versão do pacote incompatível antes de cobrar estritamente Unsupported; ausência continua NativeRuntimeUnavailable. Três testes passaram, incluindo quatro filhos da matriz. MCP/canonical events/steering e ausência de turn inicial continuam exigidos; não se aceita conjunto frouxo de rejeições.
- session_ops: 2db773142 aplica cfg(unix) a std::io e SOCKET_IO_TIMEOUT, como socket_command. Nenhum fonte de runtime tem lógica alterada. Proveniência vendor ed6b1eac31ddd261194b342d15e99737331bd9a0, base/MIT/contador histórico preservados.

Logs review2-{drain,acp,codex}-{red,green}.log e review2-*-final.log no diretório round2. Gates locais desta revisão concluídos sobre be9aad878, todos exit 0, conforme a tabela abaixo. As limitações de execução Windows nativa e de testes globais do vendor registradas nas seções anteriores não são convertidas em prova nativa.

| Comando da revisão de quatro achados | Exit | Log |
|---|---:|---|
| `cargo test --locked -p zeron-engine --lib stop_fenced_hold_tests -- --nocapture` | 0 | review2-drain-green.log |
| `cargo test --locked -p zeron-engine --test acp_registry_rpc -- --nocapture` | 0 | review2-acp-green.log |
| `cargo test --locked -p zeron-harness --test codex voice_ -- --nocapture` | 0 | review2-codex-green.log |
| `cargo test --manifest-path third_party/unpeel/crates/Cargo.toml --locked -p unpeel-core --lib session_ops::tests` | 0 | review2-session-ops-final.log |
| `cargo test --locked -p zeron-workers-unpeel` | 0 | review2-downstream-final.log |
| `scripts/ci/test-core.sh` | 0 | review2-core-final.log |
| `cargo nextest run --config-file scripts/ci/nextest.toml --locked --no-fail-fast -p zeron-ui --lib` | 0 | review2-ui-final.log |
| `cargo check --workspace --all-targets --locked` | 0 | review2-workspace-check-final.log |
| `cargo check --locked --target x86_64-pc-windows-gnu -p zeron-harness -p zeron-engine` | 0 | review2-windows-final.log |
| `cargo fmt --all -- --check` | 0 | review2-fmt-final.log |

Core: 2647 passed, 27 ignores anteriores e 2 leaky PASS; UI: 2827 passed/12 ignores anteriores. session_ops: 14 passed; downstream: 542 testes Rust e runner worker_restart_identity. Nenhum novo ignore/skip/retry/sleep ou timeout ampliado; markers ACP e HOME selecionado Voice têm controles de sensibilidade RED. Windows GNU não emite os diagnósticos de SOCKET_IO_TIMEOUT/std::io de session_ops apontados na revisão. Proveniência vendor conferida contra o Git. Sem push/merge/tag/PR/archive.


## Revisão rodada 3 — término de processo ausente no Browser (2026-11-05)

RED f1031c2b8: regressões de singleton e socket/PID falharam por ESRCH (exit 101); os arquivos privados permaneceram após a tentativa de término. A extração dos passos de aposentadoria mantém a lógica anterior e permite testar os mesmos passos chamados por resolve_profile_dir sem depender de uma corrida do agendador. PID positivo i32::MAX é comprovado fora do intervalo alocável do kernel Linux antes de qualquer sinal; os testes não tocam processos de outras sessões.

GREEN 9690c28b0: platform::terminate_process retorna NotFound só para ESRCH no Unix e ERROR_INVALID_PARAMETER de OpenProcess no Windows; pid <= 0 permanece InvalidInput e outros erros, inclusive TerminateProcess/access denied, não são normalizados. Browser tolera NotFound e limpa seus artefatos sem aguardar/escalar um PID desaparecido; negação preserva todos os arquivos e chega ao trace/retorno antecipado do chamador. Checagens de argv/ownership e graça/escalada após sinal bem-sucedido permanecem. Auditoria das quatro chamadas em review3-caller-audit.json: três Browser (força, TERM, escalada) adotam a política local; local_urls::stop_server_for_url continua Err para qualquer erro, incluindo NotFound, com lógica e texto de erro preservados.

Suíte core completa: 746 passed, 2 ignores anteriores; Browser: 18 passed. As regressões novas cobrem ausência com symlink/socket reais, arquivos preservados em PermissionDenied, ausência sem escalada e classificação/mensagem/errno preservados. Downstream: 542 testes Rust, zero falhas/ignores e runner worker_restart_identity. Sem novos sleeps ou retries: as duas esperas existentes do fluxo vivo foram movidas sem mudar seus valores; não há espera/escalada adicional após observar NotFound.

| Comando desta revisão | Exit | Log |
|---|---:|---|
| `cargo test --manifest-path third_party/unpeel/crates/Cargo.toml --locked -p unpeel-core --lib browser_mcp::tests -- --nocapture` | 0 | review3-browser-green.log |
| `cargo test --manifest-path third_party/unpeel/crates/Cargo.toml --locked -p unpeel-core --no-fail-fast` | 0 | review3-core-final.log |
| `cargo check --locked --target x86_64-pc-windows-gnu -p zeron-harness -p zeron-engine` | 0 | review3-windows-pair-final.log |
| `cargo check --manifest-path third_party/unpeel/crates/Cargo.toml --locked --target x86_64-pc-windows-gnu -p unpeel-core --lib` | 0 | review3-windows-lib-final.log |
| `cargo test --locked -p zeron-workers-unpeel` | 0 | review3-downstream-final.log |
| `cargo fmt --all -- --check` | 0 | review3-fmt-final.log |
| `rustfmt --edition 2021 --check third_party/unpeel/crates/unpeel-core/src/browser_mcp.rs third_party/unpeel/crates/unpeel-core/src/platform.rs third_party/unpeel/crates/unpeel-core/src/local_urls.rs` | 0 | review3-vendor-fmt-final.log |

Proveniência vendor b9a4a25440b79a165a59259b77601c030e8b6534 conferida no Git; base, MIT e contador histórico preservados. Check GNU não é execução Windows nativa; não houve QA nativa de Browser/AppKit nesta box Linux. Sem app/profile real, push, merge, tags, PR, dispatch ou archive.


## Revisão rodada 4 — árvore de servidores Windows (2026-11-05)

RED e0711c693: a lógica real de coleta/terminação foi extraída para um seam privado com operações de handles substituíveis, mantendo os retornos antecipados. No Linux, o comando filtrado tree_stop_ saiu 101: cinco falhas de asserção e um controle aprovado. Erros open/creation deixavam a lista de tentativas vazia, TerminateProcess interrompia na raiz e a combinação de falhas não chegava aos handles seguros. A asserção adicional de criação da raiz também demonstrou que seu handle não era tentado. O controle de criação pai/filho e deduplicação já passava.

GREEN 9a3f5830a: sete testes focados passam, exercitando exatamente o seam chamado pela implementação Win32. Um RED adicional durante a revisão do adapter mostrou que ErrorKind::NotFound genérico era amplo demais (exit 101); ausência agora é um resultado Option explícito, produzido apenas para OpenProcess/ERROR_INVALID_PARAMETER no adapter nativo. O teste final mantém outros NotFound como erro e confirma as outras tentativas antes da devolução. Logs review4-tree-red-final.log, review4-missing-classification-red.log e review4-tree-green-final.log; fonte/patch intermediário e hashes em review4-source-audit.json.

| Comando desta revisão (código 9a3f5830a) | Exit | Log |
|---|---:|---|
| `cargo test --locked -p zeron-preview --lib tree_stop_ -- --nocapture` | 0 | review4-tree-green-final.log |
| `cargo test --locked -p zeron-preview --no-fail-fast` | 0 | review4-preview-final.log |
| `cargo check --locked --target x86_64-pc-windows-gnu -p zeron-preview` | 0 | review4-windows-preview-final.log |
| `cargo check --locked --target x86_64-pc-windows-gnu -p zeron-harness -p zeron-engine` | 0 | review4-windows-pair-final.log |
| `cargo fmt --all -- --check` | 0 | review4-fmt-final.log |
| `openspec validate sync-upstream-20261104 --strict` | 0 | review4-openspec-final.log |

Preview completo: 43 unitários + 9 integrações aprovados; 2 benchmarks e 1 integração com Worker local já ignorados antes desta rodada, sem novos ignores/skips. O teste de leak passou. O fake não sinaliza processos reais e não usa sleep/retry; o caminho Win32 continua retendo OwnedHandle, verificando criação e tolerando TerminateProcess falho somente quando GetExitCodeProcess comprova saída. Compilar Windows GNU não prova execução Win32 nativa, permissão real ou descoberta Windows. Plugins::start e vendor permanecem inalterados; nenhum push, merge, tag, PR, dispatch ou archive. A pendência histórica 6.7 não é encerrada por estes checks.


## Integração de origin/main após CI do merge ref (2026-11-05)

Fetch trouxe origin/main=3f10dc3a0 (feature bf1d08d4d e seus docs/OpenSpec/trail). Merge real 0f939003f tem dois pais: 8ec7ab6e6 e 3f10dc3a0, sem conflitos textuais pendentes. OmpHarness::run_session permanece exatamente como no main; seu único delta de produção para o main são as comparações access/plan em same_runtime. Registro em main-merge-source-audit.json. Os dois novos arquivos de teste do main diferem dele somente pelos três campos defaultados de RunRequest; nenhuma asserção foi removida ou afrouxada.

REDs de compilação: omp_live_cache e omp_warm_runtime reproduziram E0063, exit 101 (main-merge-harness-red.log e main-merge-engine-red.log). RED a65ecca48 protege separadamente access e plan: ambas as decisões de reuso estavam incorretas, duas falhas de asserção e exit 101 (main-merge-runtime-red-final.log). GREEN 13d93db4a: omp_rpc 73 passed/2 ignores anteriores; as duas funções live-cache compilam e retornam sem opt-in real (não se alega prova do cache Anthropic); omp_warm_runtime 5 passed. O teste de resume exige set_host_tools antes de switch_session e o de duas mensagens exige um processo/registro e um Done por turno. F3, comparação dos descriptors, grants de host tools e limpeza de MCP seguem estritos.

O primeiro workspace completo executou todos os alvos, mas saiu 101 somente em zeron-workers-unpeel --lib: probe_rejects_malformed_reply recebeu 401 antes de exercitar a resposta inválida. O helper da fixture instalava o bearer dentro da thread do servidor, depois que o probe podia ler ambiente vazio. Fix dfbc24b60 injeta token no ambiente dos dois subprocessos isolados antes de iniciar servidor/cliente e transforma o helper em função pura. O texto estrito malformed WorkerWaitState reply e is_err permanecem; a nova asserção do pai propaga a falha do filho. Nenhum caminho de auth de produção foi alterado. GREEN focado: sete testes, exit 0, incluindo o filho que exige a resposta inválida e o dispatch autenticado. Logs main-merge-workspace-final.log (RED completo) e main-merge-probe-green.log (GREEN).

A única diferença Rust posterior a 13d93db4a é inteiramente no módulo cfg(test) wait_core_tests. Core e UI não selecionam os testes unitários de zeron-workers-unpeel; fora desse módulo, cada byte Rust coincide com o snapshot que passou. Suas provas são reutilizadas por escopo conforme o AGENTS.md; workspace completo e all-targets check são repetidos após a correção. Não há nova exceção por baseline, retry, ignore, sleep de teste ou timeout ampliado.

| Gate desta integração | Exit | Log |
|---|---:|---|
| `scripts/ci/test-core.sh` | 0 | main-merge-core-final.log |
| `cargo nextest run --config-file scripts/ci/nextest.toml --locked --no-fail-fast -p zeron-ui --lib` | 0 | main-merge-ui-final.log |
| `cargo test --workspace --locked --no-fail-fast` | 0 | main-merge-workspace-after-probe.log |
| `cargo check --workspace --all-targets --locked` | 0 | main-merge-check-after-probe.log |
| `cargo fmt --all -- --check` | 0 | main-merge-fmt-after-probe.log |
| `cargo check --locked --target x86_64-pc-windows-gnu -p zeron-harness -p zeron-engine` | 0 | main-merge-windows-final.log |
| `openspec validate sync-upstream-20261104 --strict` | 0 | main-merge-openspec-final.log |

Todos os sete gates exigidos têm recibos válidos exit 0. Core: 2669 passed/27 skips anteriores (2 leaky PASS); UI nextest: 2827 passed/12 skips anteriores. O workspace completo após o fix executou todos os alvos sem falha, incluindo 336/336 unitários Workers e spawn_chat_inherits_config_and_stamps_sessions_only_on_the_parent. Check all-targets e fmt foram repetidos após a correção e passaram. As etapas normais GNU compilam o mesmo código de produção, pois o único delta posterior ao GREEN OMP é cfg(test). Logs/recibos e auditoria de escopo ficam no diretório round2. Nenhuma falha inicial foi promovida a aceite.

Runner: voice-tests.yml só muda dois labels macos-14 → macos-15, preservando jobs, plataformas restantes e etapas. A API confirmou no head remoto 8ec7ab6e6 os 14 checks verdes (13 jobs Actions e CodeRabbit), duas falhas, dois cancelamentos e um skip. O Windows nativo terminou suas etapas de compilação, resolução de runtime e inicialização projetada; recibo main-merge-remote-windows-before.json. Essa prova é do head publicado anterior, sem estendê-la aos novos commits. O novo agendamento macOS e o CI da integração dependem da publicação pelo dono. A pendência histórica 6.7 continua aberta. Nenhum push, merge de PR, tag, edição de PR, dispatch ou archive da change sync foi feito.


## Revisão rodada 6 — prompt antes dos steers e evidência live (2026-11-05)

RED cb0dfaefb: cargo test --locked -p zeron-harness --lib warm_prompt_preflight_and_ack_precede_ordered_steers -- --nocapture saiu 101 na comparação dos frames reais recebidos pelo child: prompt first, steer one, steer two, prompt opening, steer three; a liberação do ACK só veio depois de three. Sem sleep: o attachment passa por gate cfg(test) controlado por Notify, e a fixture retém o ACK; transições esperam eventos positivos da fila ou do child. O timeout de 3 s é somente guarda de fixture quebrada. Duas tentativas iniciais de construção da fixture (tipos do RunControls e id da resposta de controle no shell) estão preservadas em review6-order-initial-{compile,fixture}.log, sem serem usadas como prova RED. O RED válido é review6-order-red.log.

GREEN 39bf6e7b0: mensagens completas são enfileiradas em FIFO, sem dreno durante prompt_pending, steer_pending ou host tool bloqueante. Um future de steer por vez serializa preflight/RPC sem bloquear eventos/cancelamento; o consumo carrega o message_id original. Mailbox fechado aguarda as mensagens da fila local antes de aposentar o runtime ocioso. Refinamento 08373c8bc mantém também o prompt de turno seguinte atrás de resultados de host tools ainda pendentes; a primeira bateria foi interrompida durante a compilação do workspace (143, sem aceite) e os gates afetados foram reiniciados. A mesma prova agora exige a sequência prompt first → prompt opening → steer one/two/three, todos os steers após release_prompt_ack, IDs opening-id/id-one/id-two/id-three e bytes inline do attachment. Hooks de gate/observação existem somente sob cfg(test); o teste com shell child é cfg(unix), sem no-op Windows de produto.

Provas focadas: novo caso 1 passed; harness --lib omp:: 36 passed; omp_rpc 73 passed/2 ignores anteriores; omp_warm_runtime 5 passed e runtime_survival 18 passed. Logs review6-order-green.log, review6-harness-lib-final.log, review6-harness-integration-final.log e review6-engine-omp-final.log, todos exit 0.

98f062ccd dá um subdiretório por nome de teste sob o output base: a_resumed_omp_process_keeps_the_prompt_cache e a_warm_omp_process_takes_every_next_message_as_a_prompt. cwd/capture, report, session copy e boundaries derivam dessa raiz, sem serializar os testes nem mudar as asserções de cache/thinking/roster. A auditoria das chamadas e dos caminhos está em review6-source-audit.json. Os dois testes live conservam o opt-in real; não se alega execução Anthropic ou prova de cache real por seu retorno normal sem opt-in.

| Comando sobre o código final 08373c8bc | Exit | Log |
|---|---:|---|
| `cargo test --locked -p zeron-harness --lib omp::` | 0 | review6-harness-lib-final.log |
| `cargo test --locked -p zeron-harness --test omp_rpc --test omp_live_cache --no-fail-fast` | 0 | review6-harness-integration-final.log |
| `cargo test --locked -p zeron-engine --test omp_warm_runtime --test runtime_survival --no-fail-fast` | 0 | review6-engine-omp-final.log |
| `scripts/ci/test-core.sh` | 0 | review6-core-after-host-order.log |
| `cargo test --workspace --locked --no-fail-fast` | 0 | review6-workspace-after-host-order.log |
| `cargo check --workspace --all-targets --locked` | 0 | review6-check-final.log |
| `cargo fmt --all -- --check` | 0 | review6-fmt-final.log |
| `cargo check --locked --target x86_64-pc-windows-gnu -p zeron-harness -p zeron-engine` | 0 | review6-windows-final.log |
| `openspec validate sync-upstream-20261104 --strict` | 0 | review6-openspec-final.log |

Todos os recibos exigidos saíram 0 no código final, após o refinamento do dreno. Core: 2670 passed/27 skips anteriores, incluindo o novo caso e 2 leaky PASS anteriores. Workspace completo executou todos os alvos e doc-tests sem falha; o novo caso de frames, F3, access/plan, grants/limpeza de MCP e a regressão de credencial do controller passaram. Check GNU prova compilação, sem execução nativa Windows. Logs e .exit em review6-final-gates.json; o workspace interrompido em 98f062ccd não é usado para aceite. test-core.sh, workflows, cli/launch excluídos, notify no workspace, F3/drive_run e vendor permanecem sem alteração nesta rodada. A pendência histórica 6.7 de CI permanece distinta. Sem push, merge, tags, PR, dispatch ou archive da change sync.

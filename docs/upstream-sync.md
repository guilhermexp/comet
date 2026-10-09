# Sync com o upstream (`zeronsh/zeron`)

Registro vivo de **como** sincronizamos com o upstream e **o que decidimos** a cada vez. As decisões detalhadas de cada sync ficam na OpenSpec dele (tabela no fim). Este arquivo guarda o que precisa sobreviver entre syncs: a receita, o que recusamos e por quê, e as armadilhas que já custaram tempo.

Atualizar este arquivo é parte do closeout de todo sync.

## Estado atual

| | |
|---|---|
| Último commit do upstream mergeado | `1074bc54` (v0.2.107, 2026-10-09) |
| Merge | `5facb832`, incorporado por fast-forward na `main` a partir de `sync/upstream-v0.2.107` |
| OpenSpec | `openspec/changes/archive/2026-10-09-sync-upstream-v0-2-107/` |
| Próximo sync começa de | `1074bc54`, merge-base natural |

## Receita

1. **Buscar o upstream** sem remote fixo e sem tags locais `v*`, que o `release.yml` publicaria:
   ```sh
   git fetch https://github.com/zeronsh/zeron.git '+main:refs/upstream/zeron-main' --no-tags
   git log --oneline --no-merges $(git merge-base origin/main refs/upstream/zeron-main)..refs/upstream/zeron-main
   ```
2. **Medir antes de mexer.**
   - `git merge-tree --write-tree --name-only origin/main refs/upstream/zeron-main` lista os arquivos que vão conflitar.
   - Auditar por área (engine/sync, contas/settings, UI, Explorer/MCP, iOS, infra), classificando cada commit como TAKE, ADAPT ou SKIP e dizendo onde encosta no fork.
3. **Worktree e branch próprias**, a partir da revisão do fork autorizada pelo dono. Use `origin/main` quando ela estiver atualizada; se o checkout principal tiver commits locais já aceitos, use seu `HEAD` para não perdê-los. O checkout principal costuma estar ocupado por outra sessão.
   ```sh
   git worktree add -b sync/upstream-vX.Y.Z ../comet-sync-vX.Y.Z origin/main
   ```
4. **Abrir a OpenSpec** `sync-upstream-vX-Y-Z` com as decisões antes de resolver os conflitos.
5. **`cargo fmt --all` do nosso lado antes do merge.** Sem isso, o merge conflita em ruído de formatação.
6. **Merge de verdade**, sem cherry-pick:
   ```sh
   git merge --no-commit --no-ff refs/upstream/zeron-main
   ```
   - O que recusamos é resolvido a favor do nosso lado **dentro** do merge. Assim o próximo sync não vê de novo o que já foi decidido.
   - Se a ancestralidade quebrar (como na reassinatura de set/2026), use o graft temporário descrito em `sync-upstream-v0-2-83` D1.
7. **Resolver os conflitos por arquivo.** Dá para paralelizar entre agentes, desde que cada um fique com arquivos disjuntos, não rode `git checkout`/`stash`/`commit` nem compile. A compilação e a correção do que ela quebrar ficam com quem coordena.
8. **Compilar e testar conforme o escopo e o `AGENTS.md` raiz:**
   - Checks locais Cargo no macOS/Linux passam por uma única rodada de `scripts/cargo-verify.py`; agrupar comandos e reaproveitar dependências, sem targets concorrentes.
   - Validar APIs entre crates com `cargo check --workspace --all-targets` quando a integração atravessar essas fronteiras; executar regressões focadas das áreas alteradas e provas nativas exigidas.
   - A regressão ampla `cargo test --workspace --no-fail-fast` fica preferencialmente no CI existente; registrar a revisão e os jobs realmente executados. Não declarar CI verde para commits locais ainda não publicados nem repetir bateria pesada só para espelhar CI.
   - Se tocar a Edge: `npm -C edge run typecheck && npm -C edge run test`.
9. **Comparar linha a linha.** Para cada commit do upstream que foi aceito, procurar as linhas que ele acrescentou, que ainda existem no upstream final e que faltam no nosso código. Isso pega o que se perdeu sem querer na resolução.
10. **DOX:** atualizar os `AGENTS.md` donos, a OpenSpec e **este arquivo**.
11. **Publicação:** push só no `origin` (`guilhermexp/comet`), só quando o dono pedir, e com `gh -R guilhermexp/comet`. Nunca pushar para o upstream. Nunca pushar tag `v*`.

## Contratos do fork que sempre vencem

- **Workers e harnesses:** Workers (`crates/workers-unpeel`, `WorkflowTask`, aba Workers), os harnesses OMP e Kimi. A integração Live Voice OMP foi aposentada em 2026-10-08; merges não devem restaurá-la. A Trajectory foi removida do fork em 2026-10-05 (`openspec/changes/remove-chat-trajectory/`); merges antigos que a citem não a restauram.
- **Notificações genuínas de Workers:** entram diretamente no mailbox do run parent steerable, inclusive com update pendente e harness TurnBoundary; mensagens comuns continuam sujeitas ao gate. A lease de execução continua bloqueando o instalador.
- **Steering:** Enter com run ativo faz steer. A fila do upstream coexiste, mas não substitui (v0.2.83 D3).
- **Modelo de grant MCP:**
  - `comet-workers` vai para quem pode lançar Workers.
  - `comet-sessions` e `zeron` (o `zeron mcp`) vão **só** para o orquestrador raiz com IPC ativo.
  - `RunRequest.mcp` fica sempre vazio: não entra a injeção do upstream em todo run.
- **Voz:** Codex voice usa o helper standalone instalado e o lifecycle host-owned; substitui a integração antiga de Live Voice OMP, aposentada em 2026-10-08. A autorização de tools do seu orquestrador usa o grant raiz do fork, não injeção global por `RunRequest.mcp`. Consulta opcional de conta tem deadline e não bloqueia indefinidamente o início de texto.
- **Contas e uso:** o painel Usage da Details, os medidores do fork (Grok gerenciado, Cursor, Kimi, Antigravity) e `usage_lines`.
- **Visual:** Settings no layout do upstream #449, mas com as seções Projects e Accounts do fork; Changes dentro do Files, transcript compacto, sem anel de contexto e sem terminal de rodapé.
- **Painel direito:** só conta como aberto se tiver aba viva.
- **Publicação e update:**
  - versão 0.2.18, `release.yml` do fork, zui vendorizado em `third_party/zui`; deltas de API do pin upstream são aplicados numa fonte separada e copiados com proveniência, sem trocar para git dependency;
  - o updater não usa o feed `zeron.sh`: vale o `has_release_feed`, e um feed próprio exige `ZERON_RELEASES_URL`.

## Recusado: continua a versão do fork

| O que | Upstream | Sync | Por quê |
|---|---|---|---|
| Fold compacto de tools (`TOOL_FOLD`), tools em árvore, ícones Symbols | #250/#253/#254/#354 | v0.2.83 | O fork tem apresentação própria |
| Anel de contexto (`context_usage.rs`) | — | v0.2.83 | O fork usa o indicador de contexto do rodapé do composer |
| Live preview de markdown no composer, seletor de modelo dentro do pill | — | v0.2.83 | Apresentação do fork |
| Terminal no rodapé | — | v0.2.83 | O terminal vive no painel direito |
| Navegação por passos do New project | #403 | v0.2.83 | Apresentação do fork |
| Faixa de update | — | v0.2.83 | Fork sem feed próprio |
| Sempre enfileirar em vez de steer | #284 | v0.2.83 | Steering é contrato do fork |
| Anel de uso do plano ao lado do anel de contexto | #547 | v0.2.94 | Depende do anel removido e duplica o Usage |
| Paleta do New project igual à do Cmd+K, breadcrumbs dobrados | #549 | v0.2.94 | Bate no `spaces.rs` do fork |
| Badge de PR sem `#` | f8f9c97f | v0.2.94 | Estilo |
| OpenCode aprovando todas as permissões | #533 | v0.2.94 | Regressão de segurança |
| Checar update sem sync de workspace; link "unmanaged" para as releases do zeron | #535 | v0.2.94 | Faria o comet atualizar para o Zeron |
| Rodapé Subagents/Chats do Explorer (`files/sections.rs`) | #498 | v0.2.94 | Duplica Workers › Subagents |
| Botão "descartar árvore inteira" no diff | #81 (UI) | v0.2.94 | O descarte fica no Changes do Files. **As proteções entraram** |
| Windows: build ARM64, arrastar abas (sobe zui), cwd do ConPTY, lock de identidade | #545/#536/#528/#527 | v0.2.94 | Fork só roda em macOS |
| Updates duráveis do app desktop (Check for Updates, download em background, instalar ao sair, instalador Windows, faixa de update) | #595 | v0.2.96 | Fork sem feed de release próprio. Revertido pelo commit local `a65f309e` em cima do upstream, antes do merge; o `UpdateStatus`/faixa da sidebar que ainda vinham junto também ficaram de fora |
| Banner "Star on GitHub" no rodapé da sidebar (e `github_star_banner_dismissed`) | #586 | pós-v0.2.96 | Removido a pedido: o fork não quer o convite na sidebar. No próximo sync, resolver para o lado do fork |
| Terminal de rodapé e sua geometria (`terminal/dock.rs`, `reserve_terminal`, abas/rail no `terminal/panel.rs`) | #620/#628 | v0.2.102 | O terminal vive no painel direito. `dock.rs` entrou dormente (`allow(dead_code)`) só para os próximos merges não conflitarem |
| Explorer: subagentes rodando primeiro | #638 | v0.2.102 | Mexe no `files/sections.rs`, que o fork não tem |
| Mermaid no chat do upstream (`markdown/mermaid_cache.rs`) | #760 | v0.2.102 | O fork já desenha Mermaid no chat com engine própria (QuickJS). Entraram só as partes fora do chat; o cache ficou dormente (`allow(dead_code)`) |
| Rótulo de branch no rodapé do composer | #744 | v0.2.102 | O fork tirou a branch do rodapé (vai no Details, `01920115`) |
| Caminhos de drive Windows no seletor de pasta | #727 | v0.2.102 | Fork só roda em macOS; revertido dentro do merge |
| Ignorar `.agents`, `.claude` e `CLAUDE.md` no `.gitignore` | 7d454cfe | v0.2.102 | O fork versiona os três (skills, comandos OpenSpec, CLAUDE.md); a regra escondia arquivos novos ali |
| Workflow macOS separado, pins de action por SHA, Dependabot | CI | v0.2.102 | O fork mantém os próprios workflows (gate por path). `macos.yml`, `dependabot.yml`, `run-macos-fixture.sh`, `cursor-sdk-update.yml` e `testflight.yml` ficam fora |
| Instalador Linux com updater em `~/.zeron/app` | #627 | v0.2.102 | Fork sem feed próprio. Entraram só as licenças das fontes e do Parakeet |

## Aceito com adaptação

| O que | Upstream | Sync | Como ficou |
|---|---|---|---|
| Orçamento de sockets/dials/HTTP, `sync_jobs`, ACK de nudge | #544 | v0.2.94 | Em cima do outbox durável do fork. O `MAX_CONCURRENT_DIALS` do fork **saiu**: travava joins |
| `FocusChat` e rotação justa por foco | #550 | v0.2.94 | Registrado no `crates/rpc/src/method.rs` (não forwardable) |
| Side chats (fork da conversa) | #498/#568/#567 | v0.2.94 | Aba do painel direito, com o divisor `MessagePart::Fork` |
| Tools de chat do `zeron mcp` (+ `create_chats`/`send_messages`) | #498 | v0.2.94 | Só no orquestrador raiz, via `workers_mcp`. No OMP, bridge multi-tool |
| Contas | #542/#546/#449 | v0.2.94 | Entraram Pi, Devin, OpenCode e Hermes; o Grok do upstream ficou desligado. Entraram o re-login da conta ativa e o cache de uso |
| Proteções do descarte | #81 | v0.2.94 | No `DiscardFiles` do fork: agente ativo, submódulo, `git clean` que preserva ignorados |
| Terminal no canvas de nova chat | #474 | v0.2.94 | Mantida a regra `chatId` xor `cwd` do fork, com exceção só para `space-canvas:` |
| Última seção dos Settings | #541 | v0.2.94 | Gravada por `apply_shell_settings`; vale para ⌘, / Cmd+K / menu |
| Redesign dos Settings (página que toma a janela, `section_tab`, switches, selects contidos, Providers com contas/completion embutidos, Devices refeito) | #449 (UI) | v0.2.94 | Páginas do upstream com os deltas do fork: arms OMP/Kimi, textos "Chat", `origin_chat_id`, a11y do Shortcuts. Ficam do fork: Projects (restilizado com os widgets novos), Accounts como seção própria (`ACCOUNT_PROVIDERS` com Kimi, `provider_can_add` sem Add para Kimi/Grok, toggle do widget Usage por conta), rótulos Agents/Accounts, o switcher Orchestrator/Workers no topo da coluna, `nav.push` e o `close_settings` do fork. O tema do Appearance é o do upstream sobre o registro do fork (`zeron-theme`) |
| Seção General nos Settings | #449/#541 | v0.2.94 | Seção default; `settings/general`/`conversations` abrem nela. Títulos de Chat (`thread_naming.rs`, sem `targetDeviceId`) + comportamento do composer |
| Escape em camadas nos Settings | #541 | v0.2.94 | `dismiss_settings_escape_surface` + `dismiss_on_escape` das páginas (Devices, Agents, Accounts, Appearance); só na página do Orchestrator — Settings dos Workers mantém o comportamento antigo |
| App iOS | #570 | v0.2.94 | Reescrita do upstream aceita inteira. As adaptações antigas (OMP, streaming) precisam ser refeitas |
| Parser de markdown em `crates/markdown` | #570 | v0.2.94 | A heurística de path do fork foi para `zeron_markdown::file_path` |
| Monitor e controles de update das CLIs de agente (`harness_updates.rs`, leases de execução no registry, `WatchHarnessUpdates`/`CheckHarnessUpdates`/`ApplyHarnessUpdate`/…, política por agente no Providers, notificação) | #389/#596 | v0.2.96 | RPCs registrados no `method.rs` (forwardable; deadlines 4 min/20 min do upstream). A lease entra na estrutura do fork: título, recap e commit message pegam a lease uma vez e a passam para `discover_models_with_lease` e para o run (orçamento `with_retry_budget` mantido); o steer roteado usa `while_update_clear` dentro do loop `activity_reservation`. É o único updater desde `unify-agent-cli-updates` (a manutenção de Workers foi apagada). Deltas do fork a preservar em merges: Pi com `update --all` (pi + pacotes), OMP monitorado (só Kimi fora de `monitored`), `UpdatePlan::PackageManager` para Codex via npm/cask (`codex_package_manager_update`), **Update all** na ilha (`update_all_targets`) e os Presets de Workers lendo `AppState::harness_updates`. Controles na página Agents (Providers) do fork; `agent_update_notifications` gravado por `apply_shell_settings` |
| Pi via RPC nativo | #630 | v0.2.102 | O grant MCP do fork (`comet-workers`/`comet-sessions`/`zeron`) vai por uma extensão-ponte por servidor (`pi/mcp.rs`) |
| Move/Delete na árvore de arquivos | #514 | v0.2.102 | Ficou o modelo do upstream (revisão + `WorkspaceMutationOutcome`). Create continua do fork, usado pelo New File/New Folder do explorer. A árvore Files própria do Details e os RPCs `RenameWorkspaceEntry`/`CopyWorkspaceEntry` foram removidos em 2026-10-05 (`openspec/changes/remove-details-files-tab/`): só existe o explorer do upstream |
| Updates de CLI via Homebrew e Antigravity | #661/#617 | v0.2.102 | Um mecanismo só, `UpdatePlan::PackageManager { program, args, env }`: brew primeiro, depois o npm prefix/Caskroom do fork. Antigravity entrou no monitor |
| Ditado local no composer | #591 | v0.2.102 | Ditado separado, atalho `mod-d`; Live Voice OMP aposentado em 2026-10-08 |
| Seletor compacto de modelo/effort | #471 | v0.2.102 | Opt-in, padrão desligado (o upstream liga por padrão) |
| `core-tests` (um nextest) no lugar de `session-sync-regressions` | CI | v0.2.102 | Mantido o gate por path do fork, agora incluindo `crates/preview/` e `scripts/ci/` |
| Renomear chat inline em vez de diálogo | c78bb1c1 | v0.2.102 | Na sidebar como no upstream. Side chat do fork só existe como aba: o rename inline acontece na aba (duplo clique ou menu); com a aba fechada, a sidebar pede para abri-la |
| Painel de checklist do agente (`TodoStatus`, `inProgress`) | #707 | v0.2.102 | O OMP também mapeia `in_progress` das fases para `InProgress` |
| Anexos BMP → PNG em background | #739 | v0.2.102 | A classificação de drop do fork (imagem, menção de projeto, arquivo externo) continua síncrona; só o staging lento vai para o background |
| Inline code com nome de arquivo real vira link | #606/#633 | v0.2.102 | Substitui o chip do fork só nesses spans; os demais seguem chip |
| Codex voice local/remoto, stage, iOS Live Activities, helper 0.161 | #819/#834 | v0.2.106 | Cinco crates novas e helper standalone do usuário; sem runtime Codex/GStreamer no app. OMP Live foi aposentado em 2026-10-08; preserva grants do fork e limita o warmup opcional de conta |
| Identidade Git, filtros multi-device, ícones e rolling labels | #799/#811 | v0.2.106 | Grupos não substituem Checkout/device nem identidade de Workers; zui recebe a API de glifo transformado mantendo os patches locais |
| Chips de anexos e pin font | #775/#816 | v0.2.106 | Referências no caret e undo com classificação de menções, long paste e Appshots do fork |
| Recovery de Chat iOS e snapshots Claude | #826/#835 | v0.2.106 | Recovery real sem fixture; snapshots conhecidos usam linha curada, desconhecidos permanecem |
| Startup e persistência compactos | #818 | v0.2.107 | Compactação local preserva tombstones/outbox sincronizado; `running_subagents` é defaultado separadamente de `context_usage`; clocks atualizam presença no prazo observável |
| Runtime persistente e Stop explícito | #748 | v0.2.107 | Controles por turno mantêm grants raiz/OMP/steering do fork; Worker genuíno passa direto pela mailbox disponível, inclusive update/TurnBoundary; Stop ordenado encerra a árvore de processos |
| Objetos privados dos snapshots de Turn | #820 | v0.2.107 | `TreeSnapshot` acompanha captura/leitura/recaptura do diff; armazenamento privado fica fora do checkout, respeitando limites e ownership do executor |
| Entrega remota durável | #777 | v0.2.107 | Handoff autenticado antes do ACK e retry por alarme limitado a 1.440 tentativas; expirar wake não apaga comandos aceitos. iOS usa background task limitada e cancela no foreground/signout |
| Imagens, seleção e badges de arquivo nas tools | #813/#522 | v0.2.107 | Portados ao transcript compacto do fork; previews limitados/liberados no collapse, texto selecionável e abertura de arquivo sem toggle; editor padrão só para arquivo local |
| Subagentes concluídos e contagem ativa | #647 | v0.2.107 | Dentro de Details › Workers › Subagents: Running visível, Finished recolhido, Completed/Failed/Cancelled separados e páginas independentes de 10. Preserva idade dos Workers e telemetria do provider primário |
| Queue compartilhada | — | v0.2.83 | Coexiste com o steer. Steers de harness que só lê no fim do turno ficam retidos |

## Armadilhas conhecidas

- **Campos a mais no proto do fork quebram literais do upstream:**
  - `RunRequest` tem `enable_workers_mcp`, `workers_parent_chat_id` e `sessions`;
  - `Chat` tem `origin_chat_id`;
  - `Session` tem `context_usage` e `error`;
  - `MessagePart` tem `completed`, `duration_ms`, `execution` e `file_preview`;
  - `AgentAccount` tem `usage_lines`.

  Crates novos do upstream (`client`, `mobile`) e testes não compilam até completar esses campos. Um script que acha literais `RunRequest {` sem esses campos poupa horas.
- **`serde_json` `preserve_order`** só é ativado no `cargo test --workspace` (unificação de features). Fixtures que comparam JSON como string passam em `-p` e falham no workspace. As fixtures precisam aceitar as duas ordens.
- **`cargo test` paralelo vs nextest.** O upstream roda nextest (um processo por teste). Testes que dividem estado global no mesmo processo podem falhar só no nosso `cargo test --workspace`: `executable::tests::version_probe_bounds_hangs_and_rejects_nonzero` perde a corrida pelo lock de `VERSION_CACHE` para o probe lento de `opencode::tests::cold_first_version_probe_still_injects_mcp`. Passa sozinho e em `--test-threads=1`.
- **Testes WebRTC do `zeron-preview`** (`peer::tests::*`, `tests/leak.rs`) falham às vezes nesta máquina, igual no `origin/main`. Antes de culpar o merge, compare rodando no `origin/main` puro.
- **Settings do fork** gravam pelo `apply_shell_settings`. Campo novo do upstream em `UiSettings` precisa entrar ali, senão nunca é salvo.
- **Testes do upstream que usam `Shell::new`** passam a usar `shell::test_shell` (o fork tem o `WorkersModel`).
- **`crates/ui` com markdown:** o parser mora no crate compartilhado. Qualquer referência a `crate::file_preview` dentro dele quebra.
- **zui vendorizado fica atrás do pin do upstream.** O upstream sobe o rev do `zui` no `Cargo.toml`, e o fork ignora o pin. Código novo do upstream que usa API nova do gpui não compila (`EdgeFade { band_left }` no v0.2.102). Aplicar como patch em `third_party/zui` os commits do zui entre o `base_revision` do `zui-upstream.toml` e o novo pin, e registrar lá.
- **Testes do fork que o upstream reescreveu por cima.** Quando o upstream acrescenta testes num arquivo que o fork reescreveu (ex.: `engine/tests/workspace_files.rs`), os helpers dele não existem do nosso lado. Mover os testes novos para um arquivo próprio (`workspace_entry_mutations.rs`) com o cabeçalho de helpers do upstream.
- **Auditoria de additions:** compare sempre o intervalo incoming do merge-base até o pin upstream. Comparar o fork inteiro contra o upstream confunde divergências antigas com omissões e pode apagar código exclusivo do fork.
- **Resolver com agentes:** eles podem apagar código "morto" que outro arquivo ainda usa (por exemplo `motion::fast_tier`). A etapa de compilação precisa reconciliar isso.

## Integração v0.2.107

Intervalo incoming: `916cb1ccb1342c2061963a5f6ecf5c977ba46f2c..1074bc540b995d6b7e5a6fa5431f1a2420df8d91`, oito commits. Branch `sync/upstream-v0.2.107`, base local autorizada `49ada659`, planning `a643cbe3`, worktree `../comet-sync-v0-2-107`. Todas as funcionalidades foram aceitas e adaptadas: startup compacto, browser Windows/imagens/seleção de tools, snapshots privados de turno, runtimes persistentes/Stop ordenado, entrega remota durável, contagem/grupos de subagentes e badges de arquivo/editor local. A versão do fork permanece `0.2.18`; não restauramos listas duplicadas no Explorer. O zui foi importado a partir de fonte separada sobre `dce5c1f`, árvore `7e0cdc6185ee4f71274883b686010f2283dac6ed`, preservando os patches do fork e as duas regras de pontuação.

A resolução preserva OMP/Kimi, grants MCP raiz, avisos diretos dos Workers mesmo com fila comum pausada, contexto/erro/usage e o transcript compacto. Revisão e testes também motivaram teardown de descendentes/limite de saída no runner Git, invalidação de runtime por grants, imagens inline no steer OMP, manutenção SQLite sem espera por lock e propagação de erro/timeout completo no POST OpenCode. A auditoria compara somente o intervalo incoming e classifica adaptações intencionais.

Prova e limites: `openspec/changes/archive/2026-10-09-sync-upstream-v0-2-107/verification.md`. A build nativa, checks de API, regressões focadas de UI/runtime/harness/sync, Edge/workerd e round-trip iOS têm evidência local. Permanecem limites explícitos no timing Kimi inalterado, prazo intermitente do Stop saturado, benchmark iOS e automação de paste; não se declara suite ampla ou CI verde. O merge `5facb832` tem pais `a643cbe3` e `1074bc54`; main incorporada por fast-forward e Graft reconstruído (1.628 arquivos, 52.194 nós). Target temporário removido. Nenhum push, deploy, tag, instalação ou reinício do dev normal integra este sync.

## Integração v0.2.106

Intervalo completo: `9e1a1115..916cb1ccb1342c2061963a5f6ecf5c977ba46f2c`, 15 commits. Branch `sync/upstream-v0.2.106`, base local autorizada `b01350d6`, worktree `../comet-sync-v0-2-106`. OpenSpec `sync-upstream-v0-2-106` registra decisões e prova. Toda a funcionalidade foi selecionada; bumps de versão e detalhes de release/navegação foram adaptados aos contratos acima. O zui vendorizado vai a `0966d065`, árvore `6e4082751218f31cc40f3f2ace860c0556ba5baa`. A fonte reutilizável fica no `.tmp/zui-source` ignorado do worktree.

A integração foi revisada com preservação dos contratos do fork. Workspace/all-targets, build nativa, UI desktop completa (2.232 testes), Codex, anexos enfileirados, Edge e packaging passaram; as provas nativas de intake/undo/send/echo e Files usaram perfis isolados. A execução inicial do workspace teve 5.051 passes, 21 falhas e 44 ignores já existentes; grupos corrigidos foram reexecutados, sem declarar uma suite final toda verde. O RPC CreateWorktree ainda excedeu o prazo original de quatro segundos na repetição corrigida, e preview churn falhou na drenagem de conexões em três execuções isoladas. Causas não estabelecidas, limites preservados. O core iOS e os bindings foram regenerados, e a rodada final passou com 12 testes unitários ligados e 4 testes de interface, sem falha ou skip. O merge `cd38b6dc084ac32b76538926285ccf22c85c3b10` tem os pais `b01350d6` e `916cb1cc` e foi incorporado por fast-forward na main limpa. OpenSpec arquivada, Graft reconstruído na main, target Cargo temporário removido. As duas pendências de aceitação permanecem explícitas na prova arquivada. Nenhum push, tag ou troca do app instalado foi feito.

## Histórico

| Data | Intervalo do upstream | Onde | Notas |
|---|---|---|---|
| 2026-08-30 | até v0.2.29 | `openspec/changes/archive/2026-08-30-sync-upstream-v0-2-29/` | Marco de proveniência em v0.2.18; merge conservador e porte por capability |
| 2026-09-10 | correções pontuais | `openspec/changes/archive/2026-09-10-adopt-upstream-followup-fixes/` | 7 correções, sem merge de ancestralidade |
| 2026-09-17 | melhorias de setembro | `openspec/changes/archive/2026-09-17-integrate-upstream-september-updates/` | Cmd+K, fontes, appshots, imagens geradas |
| 2026-09-22 | até v0.2.83 (`d721f301`) | `openspec/changes/sync-upstream-v0-2-83/` | Merge `55395013` via graft (upstream reassinado); v0.2.84 em `010e02d2` |
| 2026-09-27 | até `433aa148` (v0.2.94+) | `openspec/changes/sync-upstream-v0-2-94/` | Merge `1065d252`; iOS reescrito; o fork segura o grant MCP |
| 2026-09-27 | até `9d3cc8b2` (v0.2.96) | `openspec/changes/sync-upstream-v0-2-94/` | Segundo merge na mesma branch; #595 recusado via revert local `a65f309e`; entram #389/#596, #588, #586, #592 e iOS |
| 2026-09-28 | `e13b18de` (#599) | porte pontual | Cmd/Ctrl+C copia a seleção do transcript com o foco fora do composer; o próximo sync resolve para o lado do upstream |
| 2026-10-02 | até `9e1a1115` (v0.2.102 + 21) | `openspec/changes/sync-upstream-v0-2-102/` | 56 commits; Pi nativo, ditado, seletor compacto opt-in, file tree actions; zui recebe 3 commits por patch |
| 2026-10-07 | até `916cb1cc` (v0.2.106) | `openspec/changes/archive/2026-10-07-sync-upstream-v0-2-106/` | Merge `cd38b6dc`; 15 commits, Codex voice local/remoto/iOS, chips, identidade Git e catálogos; contratos do fork preservados. Prova desktop/iOS passou; RPC worktree e preview churn permanecem pendentes |
| 2026-10-09 | `916cb1cc..1074bc54` (v0.2.107) | `openspec/changes/archive/2026-10-09-sync-upstream-v0-2-107/` | Oito commits: memória, runtimes/Stop, snapshots privados, entrega remota, imagens/badges e subagentes concluídos; contratos do fork preservados. Provas e limites registrados no arquivo verification.md |

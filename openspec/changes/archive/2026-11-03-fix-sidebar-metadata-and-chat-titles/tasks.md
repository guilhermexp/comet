# Tasks

> Execução após aprovação: Worker separado via `/delegate`, seguindo `/implement`. Subagentes de pesquisa são somente leitura. Este arquivo é o plano único, sem cópia em docs/superpowers.

**Goal:** corrigir adjacência da metadata, recuperação da titulação e apresentação do topo sem alterar dados de Chats existentes.
**Architecture:** presenters/layout atuais, aplicação condicional no workspace e fixture detached; sem novo schema ou provedor.
**Tech Stack:** Rust 2024, GPUI, Tokio e Loro existentes.
**Spec:** `design.md`, `specs/chat-navigation-and-sidebar/spec.md` e `specs/upstream-followup-correctness/spec.md` desta change.

## Global Constraints

- Base inspecionada `4556e3a9faf9b777cc2edcb03d105da6665a440c`; preservar WIP de voz, checkouts, perfis, serviços e recursos de outras tarefas.
- Não renomear Chats antigos, inferir proveniência, trocar contas/modelos, acrescentar retry/timer, modificar wire/RPC ou alterar consumidores fora do topo solicitado.
- Frontend segue `/Users/guilhermevarela/.agents/skills/impeccable/SKILL.md` e a implementação visual atual; ler craft-floor imediatamente antes de editar UI. Sem init/redesign ou reparo lateral de DESIGN.md.
- Cargo nativo, jobs=2+sccache e target exclusivo/reutilizado por checkout; preflight de disco. Sem cargo-verify/fila global/no-mistakes, sem repetição de provas válidas por ritual.
- Destino proposto: main do fork após provas e revisão. Nenhuma tag, release ou instalação autorizada por este plano.

## Review Focus

- RF1: branch longa, múltiplos projetos, PR e larguras 242/256/400px devem preservar espaço útil, clique e modos/toggles.
- RF2: menções completas/cortadas, Unicode e colchetes comuns devem ficar legíveis sem modificar título durável ou rename.
- RF3: saída vazia/erro/configuração indisponível não deve congelar título nem renomear worktree; um gatilho normal posterior deve conseguir aplicar sucesso.
- RF4: título manual anterior/concorrente, título legado igual ao prompt e Chat removido não podem ser sobrescritos/recriados.
- RF5: a fixture nativa não pode iniciar Workers vivos, engine, providers ou escrever nas configurações reais.

## 1. Preservação e ownership

- [x] 1.1 Registrar base/estado e transferir esta change integralmente para checkout de implementação isolado, sem absorver o plano de voz. Um Worker possui a integração dos três itens. Verificar paths/refs e unidade preservada antes de escrever; LSP não está configurado, portanto levantar referências por busca textual se continuar indisponível. Não usar conversas reais como fixtures.

  Evidência: base `4556e3a9faf9b777cc2edcb03d105da6665a440c`; branch `fix/sidebar-metadata-chat-titles-20261103`; worktree `/Users/guilhermevarela/.zeron/worktrees/comet-f24c1972e3999c0e/fix-sidebar-metadata-chat-titles-20261103`. A cópia integral foi verificada com `diff -rq` contra a change da origem (exit 0, sem diferenças) antes de editar; a origem e a change de voz não foram alteradas. `.cursor/hooks.json` já estava modificado e será preservado fora do commit. `xd://lsp` status: nenhum servidor configurado. `openspec status` e `instructions apply` executados neste worktree.

## 2. Item 1: branch e projetos próximos

- [x] 2.1 Em `crates/ui/src/shell/spaces.rs`, adicionar `sidebar_metadata_short_branch_is_adjacent_to_projects` e observar RED por bounds do renderer real com `main`+projeto e Branch/Projects worked ligados: início da primeira pasta imediatamente após o fim do label, gap de 4px, sem vazio que cresce com a largura. Em `shell.rs::render_chat_row`, retirar crescimento da branch, manter shrink/fade e limitar sua largura máxima à metade da linha somente com projetos; a branch curta usa largura natural e projetos recebem o restante. Não mudar `sidebar_faded_label` globalmente. Observar GREEN em 242/256/400px e cobrir RF1 com branch longa, múltiplos projetos, PR clicável, Branch/Location off, Compact sem metadata e Archived. Preservar os testes úteis existentes e atualizar o contrato dono em `crates/ui/AGENTS.md`.

  Evidência: RED `sidebar_metadata_short_branch_is_adjacent_to_projects`: branch `main` ocupava 95,5px em 242px; GREEN no mesmo renderer para 242/256/400, normal e Archived, e branch longa com dois projetos e clique real de PR. O filtro `sidebar_` passou 69 testes; cobre também Branch/Location off, Show e Compact.

## 3. Item 2: geração automática sem fallback definitivo

- [x] 3.1 Em `crates/engine/src/titles.rs`, acrescentar `automatic_title_failure_does_not_block_later_generation`, usando workspace temporário e `RecordingTitleHarness` controlado: primeira geração sem texto útil deixa título ausente; geração seguinte com texto válido persiste o resultado. Observar RED pelo fallback gravado atual, não por falha de setup/compilação. Alterar `TitleGenerator::generate` para não gravar fallback nem renomear branch em ausência de resultado válido. Manter harness/modelo, idioma, isolamento, budgets e gatilhos existentes. Remover helper/imports/testes exclusivos de fallback morto após levantar seus consumidores. Observar GREEN e casos RF3 sem provider/conta real.

  Evidência: RED `automatic_title_failure_does_not_block_later_generation` gravou palavras do prompt; GREEN após retirada do fallback, incluindo branch Git sintética inalterada na falha e geração válida posterior. `trail grep` encontrou `fallback_title` e `mention_link_len` apenas no próprio mecanismo removido e seu teste.

- [x] 3.2 Em `workspace_host.rs`, implementar `pub(crate) fn rename_chat_if_untitled(&self, chat_id: &str, title: &str) -> Result<bool, EngineError>`: checar existência e título vazio/whitespace na mesma mutação que escreve; false para Chat ausente/já nomeado. Usar esse resultado no gerador antes dos efeitos existentes de renome de worktree, sem alterar o rename manual. Testar RF4 no pipeline real com harness pendente e workspace temporário: nome manual anterior não chama modelo; rename durante geração vence; legado igual ao prompt fica intacto; remoção não recria row. Registrar evidência e atualizar as linhas de titulação em `crates/engine/AGENTS.md`, preservando idioma e restrições de execução. Não afirmar que a causa operacional inicial dos Chats da imagem foi identificada sem logs específicos.

  Evidência: `titles::tests` passou 11 testes, incluindo harness bloqueado durante rename/exclusão, título manual e legado, e guard para ausente, whitespace, nomeado e removido. Nenhuma conta real ou Chat real foi consultado.

## 4. Item 3: título superior legível

- [x] 4.1 Acrescentar uma prova GPUI `title_bar_compacts_mentions_without_changing_title` no módulo de testes dono do Shell, cobrindo o consumidor de produção, não só o helper. Observar RED para `[studio](zeron-project:/fixture/studio/)` aparecendo cru. Em `shell/tabs.rs::render_session_title_bar`, aplicar `composer::title_mention_plain` antes de `single_line`, emprestando o raw via `as_deref`; manter 12px/medium, ícone de 14px, gap 6px, truncamento e controles. Observar GREEN para menção completa, legado cortado, Unicode, colchetes comuns, texto manual e largura estreita/larga. Comparar o valor durável antes/depois e verificar que rename continua usando o original. Atualizar o contrato UI correspondente; não corrigir captions/paleta/export fora do escopo.

  Evidência: RED do consumidor GPUI mediu 288px no Markdown cru contra 44px em `studio`; GREEN para menção completa/cortada, Unicode, colchetes, texto manual e título longo em 650/1400px, sem sobrepor controle Terminal, com valor e editor de rename originais.

## 5. Prova integrada e revisão

- [x] 5.1 Preparar smoke nativo seguro: disponibilizar `WorkersModel::detached` em `workers/model.rs` sob `cfg(any(test, feature = "appshots-fixture"))`, mantendo visibilidade interna; acrescentar `detached_fixture_shell(state: Entity<AppState>, boot: EngineBootConfig, cx: &mut Context<Shell>) -> Shell` sob essa feature em `shell.rs`, reutilizando o setup de resource global e `Shell::new` com o modelo detached. Não alterar silenciosamente o helper legado `test_shell`. Criar `examples/sidebar-title-fixture.rs` e sua entrada Cargo requerida por `appshots-fixture`, com estado/settings temporários. Demonstrar RF5 por inspeção do caminho de inicialização antes de rodar: nenhum daemon/engine/provider ou configuração real. Sem isolamento, não lançar nem marcar a prova concluída. Capturar a superfície real em lote dark/light, estreita/larga, branch curta/longa+PR, topo com menção e Compact/Archived representativo; conferir proximidade, texto compacto, fade e controles. No máximo um lote corretivo e uma confirmação. Guardar capturas/recibo; remover probes temporários e documentar a fixture no DOX dono.

  Evidência pré-launch: `.tmp/verify/fix-sidebar-metadata-and-chat-titles/ISOLATION.md`; o cliente detached ainda cria listener loopback/registro isolado, mas não faz refresh, descoberta ou boot; monitor de recursos desativado. A captura da NSWindow por `screencapture` falhou; o segundo lote capturou seis estados reais do renderer GPUI, dark/light 242/400 normal e 256 Compact, com Archived e PR sintéticos. Após corrigir os argumentos da fixture houve uma confirmação dark/242, também capturada e inspecionada. Sem prova de composição WindowServer/frost; imagens em `.tmp/verify/fix-sidebar-metadata-and-chat-titles/screenshots/`.

- [x] 5.2 Na fonte final combinada, agrupar `cargo test -p zeron-ui --lib sidebar_`, `cargo test -p zeron-ui --lib title_bar_`, a regressão existente `chat_titles_render_mentions_as_chips_and_repair_cut_links`, `cargo test -p zeron-engine --lib titles::tests` e os testes do novo guard de workspace. Executar formatação/check de diff somente no escopo e `openspec validate fix-sidebar-metadata-and-chat-titles --strict`. Cada filtro deve executar os casos previstos, sem aprovar filtro vazio. Registrar comando, revisão, exit e limitações; não repetir checks da mesma fonte. Acrescentar ao fork_changelog os três resultados reais.

  Evidência: Cargo nativo `--locked`, jobs=2, target exclusivo: `sidebar_` 69/69 (após corrigir setup do clique), `title_bar_` 1/1, `chat_titles_render_mentions_as_chips_and_repair_cut_links` 1/1 e `titles::tests` 11/11 (guard incluído); `rustfmt --check` escopado, `openspec validate --strict` e detector layout exit 0. Primeiro `sidebar_` terminou 68/69 por setup sem Chat ativo; corrigido e repetido. Após a revisão, a asserção do PR passou a verificar a pane aberta e a view ativa: sem clicar falha (RED), clicando passa (GREEN); o binário final passou `sidebar_` 69/69, `title_bar_` 1/1 e presenter 1/1. Não foi executada suíte de workspace nem instalado app.
- [x] 5.3 Antes do commit final, executar `/Users/guilhermevarela/orchestrator.v5/scripts/review-pair.sh --repo <checkout> --base <base> --out <projeto>/.tmp/verify/fix-sidebar-metadata-and-chat-titles/review-pair/<rodada> --gate`, incluindo novos arquivos próprios no snapshot. Não publicar WIP/hook de setup alheio. Tratar achados com evidência. Exit4/incompletude comprovada exige contingência manual completa imediata e scanner no snapshot exato, sem espera/reset/retries cegos; nunca --agent/--use-credits. Registrar modalidade, árvore/blobs e summary reais aqui antes do commit final.

  Evidência: round-1 exit 0, ambos revisores completos, zero achados, gitleaks clean; base `4556e3a9faf9b777cc2edcb03d105da6665a440c`, árvore `ad03a09c60d7e49d6fd5dce4269f442a567fcb90`. Round-2 árvore `51ca61881c8ab1a62ee761924958066efeec38ee`, exit 6: após pedido de aceite, a prova de clique verifica pane aberta e view ativa, RED sem clique e GREEN com clique; a refutação anterior foi retirada. Round-3 árvore `746ea02e9b9e157abadb07a7d93a5e3a701ee69c`, exit 4: CodeRabbit pulado por quota; recibo `review-pair/round-3/summary.md`. Contingência manual pelo implementador, não aprovação automática final: `MANUAL_REVIEW.md` sob `.tmp/verify/fix-sidebar-metadata-and-chat-titles/` registra paths, contratos, testes, achados e limites. A possibilidade de trabalho de título em gatilhos normais repetidos é trade-off do plano aprovado, que proíbe nova escada/backoff; argumento inválido da fixture foi corrigido e testado. Scanner independente gitleaks staged preliminar exit 0, sem leaks; recibo e árvore final do scan em `DELIVERY.md` no mesmo diretório. Hook alheio restaurado com `--no-skip-worktree`, conteúdo preservado.
- [x] 5.4 Aceitar a entrega e integrar somente esta change na main atual do fork, preservando trabalhos concorrentes. Conferir identidade da árvore aceita/integrada e push para origin sem force conforme o fluxo nativo; registrar commit, destino, provas e eventual bloqueio concreto. Sem tag/release/install por inferência.

  Evidência de aceite: commit `b6e24930b718b86ae4b12d24723971aaef5a4030`, árvore `27b791116c92127ea04f3787a733a7afbd0e08ad`, integrado por fast-forward sobre `4556e3a9faf9b777cc2edcb03d105da6665a440c`; `git push origin HEAD:main` concluído no fork. Revisões independentes de UI e engine sem bloqueios; aceite funcional do orquestrador 13/13, sem recompilação, em `.tmp/verify/fix-sidebar-metadata-and-chat-titles/parent-smoke/results.json`. Revisão final é manual de contingência por quota do CodeRabbit, registrada em `.tmp/verify/fix-sidebar-metadata-and-chat-titles/manual-review/acceptance/summary.md`, com scanner próprio do commit exato: exit 0, nenhum segredo. Provas integrais preservadas em `worker-delivery/`; plano original inteiro e manifesto em `approved-plan/` e `approved-plan-manifest.json`. Change de voz e hook alheio preservados. Sync conferiu os contratos UI/engine já atualizados, sem duplicar texto; raiz inalterada. Sem tag, release ou instalação.

## Workflow follow-up

- Com todos os critérios cumpridos, arquivar pelo CLI e validar as duas specs canônicas sem editá-las manualmente.
- Executar sync dos AGENTS donos e cleanup de todos os recursos próprios, preservando provas e alterações alheias; relatar removidos/retidos e tamanhos lógico/alocado/variação do volume sem atribuir consumo concorrente à tarefa.

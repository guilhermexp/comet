# Tasks

Plano da quinta correção solicitada pelo usuário. Planejamento concluído não autoriza aplicação; apresentar desenho e escolher executor antes de construir. Base `5176ebf0643efa1d56d6b217ca71bb31fee8a893`, worktree existente das correções. Não reabrir ou reduzir as pendências anteriores de Context/Blaze.

## 1. Reproduzir e corrigir a autoridade de conclusão

- [x] 1.1 Em `crates/workers-unpeel/tests/parent_notifications.rs` e no seam de testes existente de `src/parent_notifications.rs`, acrescentar regressão sintética `pi_completion_` para Stop primário atual e TUI repintando após a resposta. Exercitar o cálculo real da evidência e a elegibilidade de entrega; não injetar sempre `quiescent()` nem copiar o cálculo para o teste. Provar RED comportamental com `cargo test --locked -p zeron-workers-unpeel --test parent_notifications pi_completion_` ou o filtro co-localizado equivalente se necessário. Conferir os callbacks suportados Pi/OMP como em D1, sem alterar hooks reais.
- [x] 1.2 Corrigir a decisão compartilhada por notificação, `completed` e latch após ACK para a autoridade Pi comprovada, sem falsificar silêncio da PTY nem mudar os demais runtimes. Provar GREEN e negativos: idle sem Stop, blocked/working ou async real, Stop de geração/episódio anterior, novo Start após Stop, ACK sem duplicação e invalidação após novo pedido. A leitura de async não pode ser pulada por repaint; preservar a graça existente para serviços sem novo temporizador.

## 2. Provar retorno ao pai e compatibilidade

- [x] 2.1 Estender o fixture existente para verificar resposta final sintética reconhecível na notificação do pai e entrega única após ACK; cobrir `completed` no mesmo episódio antes/depois do ACK. Se a extração existente perder a resposta devido a repaints, corrigir somente esse seam comprovadamente necessário, mantendo sanitização/limites. Rodar `cargo test --locked -p zeron-workers-unpeel --test parent_notifications` e o filtro existente de wait no controller; registrar OMP/Codex e os guards de episódios sem regressão. Não é prova suficiente um callback ou eco de mock isolado.
- [x] 2.2 Atualizar `crates/workers-unpeel/AGENTS.md` e `fork_changelog.md` com o contrato real; rodar `cargo fmt --all -- --check` e `openspec validate fix-pi-worker-parent-return --strict`. Registrar comandos, revisão exata e limites em `.tmp/verify/fix-pi-worker-parent-return/`. Sem gpui, workspace amplo, API paga ou app dev/instalado; usar target privado novo, jobs=2 e sccache, sem reconstruir o target de 45 GiB removido.

## 3. Revisão e entrega delimitada

- [x] 3.1 Antes do commit final executar `/Users/guilhermevarela/orchestrator.v5/scripts/review-pair.sh --repo <checkout> --base 5176ebf0643efa1d56d6b217ca71bb31fee8a893 --out <checkout>/.tmp/verify/fix-pi-worker-parent-return/review-pair/1 --include-untracked --gate` com saída nova/vazia e snapshot sem o hook alheio. Exit 6: tratar cada achado com evidência. Exit 4 por indisponibilidade real, inclusive cota já comprovada: contingência manual completa imediata em `.tmp/verify/fix-pi-worker-parent-return/manual-review/1/summary.md`, sem esperar reset nem repetir só para obter o mesmo erro. Registrar base/árvore, todos os caminhos, contratos, testes, scanner e achados/tratamento. Nunca `--agent` no OpenQodex nem `--use-credits` CodeRabbit; segredo pendente continua bloqueando.
- [x] 3.2 Entregar commit, RED/GREEN e recibos de cobertura da árvore final; limpar apenas target/temporários privados próprios, com espaço lógico e físico separados, preservando fontes, branch e evidências até aceite. Não marcar checkboxes oficiais, arquivar, integrar, publicar ou instalar por conta própria; o orquestrador confere e executa o fechamento autorizado.

## Aceite local do orquestrador — 2026-11-09

As seis tarefas foram conferidas e aceitas na entrega `c72a752c3946c4b09ff5f743f0117e852d0a8b9d`, árvore `17c9563d744f5b9791929b6f19352202725613b1`. Os 11 SHA256 conferem com a revisão final. A regressão inicial falhou por comportamento (`red-pi-completion.log`, exit 101); a rodada final passou 54 testes (`green-fresh-http-start.log`) e a espera do controller (`green-controller-wait-3.log`).

Revisão por contingência manual após exit 4 real por cota CodeRabbit, não gate 0: `.tmp/verify/fix-pi-worker-parent-return/review-pair/3/summary.md` e `manual-review/1/summary.md`. OpenQodex sem achados na árvore final e scanner clean. Guarda de Start HTTP fresco corrigida antes do snapshot final.

/sync: o AGENTS dono já registra autoridade Pi, leitura de async e invalidação do latch; nada adicional a editar. Target Cargo e clone privados estão ausentes, conferidos pelo orquestrador. Fontes, branch, recibos e hook alheio preservados; a principal divergente não mudou. A variação de espaço informada pelo worker (77→78 GB livres) é relato de medição dele, não nova medição própria do orquestrador.

Destinos: código local e fechamento de contratos. Sem instalação, push, merge ou release; nenhum worker/Chat real afetado foi manipulado. Context3.1 e Blaze continuam com seus bloqueios separados.

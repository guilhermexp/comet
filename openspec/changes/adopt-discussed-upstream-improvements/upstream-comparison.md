# Comparação do Comet com o upstream — 2026-09-10

## Escopo e evidência

- Fork analisado: `main@b667dd79`, árvore limpa.
- Upstream atualizado via `git fetch --no-tags upstream`: `zeronsh/comet`, `main@a1adfde2`, versão `0.2.59`.
- O último checkpoint de revisão registrado em `fork_sync_report.md` é `b3fa5187`, associado à v0.2.29. Há 170 commits alcançáveis no intervalo `b3fa5187..a1adfde2`, incluindo merges e releases. Esse número não equivale a 170 funcionalidades faltantes.
- A ancestralidade comum calculada pelo Git é mais antiga (`c1a925d9`); o histórico contém integrações e reaplicações. A seleção abaixo compara comportamento e implementação atuais, não somente presença de hashes.
- Investigação de código e histórico, sem merge, cherry-pick, alteração de código, build, benchmark local ou publicação. Os resultados de performance citados são do upstream e não demonstram o ganho deste fork.

## Prioridades para trazer

### P1 — Trabalho redundante no Chat e syntax highlighting

Fontes: [PR #255](https://github.com/zeronsh/comet/pull/255), especialmente `4902c792`, `33825167`, `9294a95b` e `89790372`.

O upstream compartilha blocos Markdown imutáveis, compila cada configuração de gramática uma vez e carrega linguagens embutidas sob demanda. Também evita derivar o transcript quando apenas estado não relacionado mudou, empresta entradas em vez de copiar todo o histórico, reutiliza cenas e limita caches de pintura à região visível e margem de virtualização.

Confirmado no fork:

- `crates/ui/src/transcript.rs:4513`: `sync` ainda usa `s.transcript.clone()` e `s.sub_transcript(doc_id).to_vec()`.
- `crates/ui/src/markdown/parser.rs:90`: `BlockTree` ainda contém `Vec<TopBlock>`; upstream usa blocos compartilhados por `Arc`.
- `crates/syntax/src/lib.rs:314`: cada highlight recompila a configuração primária e prepara as linguagens embutidas.
- O fork já tem cache de documentos destacados, highlight em segundo plano, diffs de linhas e preview Markdown virtualizado. Preservar essas correções; o cache de gramáticas é outra camada.

Recomendação: primeira integração, com adaptação ao modelo de tool rows, perguntas, subagentes e decoração Gray. Comparar latência de abrir/fechar blocos e previews durante replay idêntico antes/depois; verificar seleção e links com as cenas reutilizadas.

### P2 — Repinturas ociosas e backend gráfico no macOS

Fontes: [PR #263](https://github.com/zeronsh/comet/pull/263), [PR #265](https://github.com/zeronsh/comet/pull/265), [PR #273](https://github.com/zeronsh/comet/pull/273), [PR #241](https://github.com/zeronsh/comet/pull/241).

O upstream mantém timestamps de presença frescos, mas só invalida a UI quando muda algo exibido. Estaciona o display link de janelas ociosas e permite retomar atualizações após falha de inscrição e wake do macOS. Migrou o GPUI para `zeronsh/zui`, removeu dependências de tracing GPL desse grafo e selecionou mimalloc v2 somente no macOS.

Confirmado no fork:

- `crates/ui/src/state.rs:804`: `apply_sessions` substitui a lista sem comparar apresentação.
- `crates/ui/src/state.rs:2090`: `spawn_watch` chama `cx.notify()` em todo frame válido.
- `Cargo.toml:71`: GPUI permanece em `wingleeio/zed@e2ddcc6`; upstream atual usa `zeronsh/zui@07fd941`.
- `Cargo.toml:106` e `apps/zeron/src/main.rs:102`: mimalloc não seleciona a feature v2 e o allocator não está limitado ao macOS.

O upstream reportou CPU média ociosa de 2,114% para 0,749% num M4 Pro em sua comparação. Esse experimento não mede nosso binário e não demonstrou queda equivalente de custo durante streaming.

Recomendação: trazer o filtro de presença primeiro; tratar o backend gráfico como integração própria, verificando nossos patches de quebra de linha, transparência, blur, janelas, WebKit, foco e retorno de sleep. Não é uma simples troca de rev.

### P3 — Composer, anexos, seleção e foco de atalhos

Fontes: [PR #271](https://github.com/zeronsh/comet/pull/271), [PR #274](https://github.com/zeronsh/comet/pull/274), [PR #285](https://github.com/zeronsh/comet/pull/285), trecho de seleção do [PR #265](https://github.com/zeronsh/comet/pull/265).

Há cache do layout de texto do input, notificação apenas após geometria final, anexos que quebram linha e recuperação de foco baseada em controles realmente montados. A seleção durante streaming suspende o acompanhamento automático antes que ele remova a âncora selecionada da região visível.

Confirmado no fork: `ComposerInput::layout_text` não contém a chave de reutilização nova; o transcript usa uma faixa de anexos de altura fixa (`ATT_STRIP_H`); `Shell::render` ainda recupera foco imediatamente pelo handle do composer. Nosso conserto do primeiro clique de links e o retorno de foco do HTML já existem e devem permanecer.

Recomendação: adaptar cache/foco/wrapping. Não importar toda a animação do composer por promessa de eficiência: o relatório upstream registra maior CPU total durante digitação por causa dos frames adicionais, embora a atividade seja limitada. Validar cliques rápidos, abrir/fechar preview, atalhos, IME e texto longo mantendo nosso posicionamento do indicador.

### P4 — Recuperação de sincronização com histórico causal faltante

Fonte: [PR #259](https://github.com/zeronsh/comet/pull/259), commit `6614b324`, parte de `chat_client`/`chat2_host`.

Hoje `EngineChatSink::apply_row` (`crates/engine/src/chat2_host.rs:76`) detecta operações pendentes por dependências faltantes, mas mesmo assim persiste o cursor. Essas operações ainda não estão representadas no snapshot. Upstream retorna `PendingDependencies`, segura o cursor e força recuperação por checkpoint, com proteção contra catch-ups concorrentes.

Recomendação: alta prioridade de correção, portando também os testes de ordem causal, persistência/restart, checkpoint e transporte HTTP/WebSocket. Não se trata de mudar a geração do protocolo ou migrar todo o backend.

### P5 — Turnos ACP encerrados por silêncio

Fonte: [PR #298](https://github.com/zeronsh/comet/pull/298), commit `32fd7070`.

O fork ainda tem `ZERON_ACP_QUIET_SETTLE_MS` com padrão de 30 segundos (`crates/harness/src/acp/mod.rs:2094`). Com conteúdo já emitido e sem tool/pergunta aberta, silêncio pode fechar artificialmente um prompt que continua em execução. Upstream remove essa inferência e acrescenta `authoritative_prompt_end` para impedir também o encerramento pelo watchdog da engine. Erros JSON-RPC passam a preservar código e detalhes.

Recomendação: alta prioridade para harnesses ACP, incluindo testes de modelo lento, ferramentas concluídas antes da resposta e interrupção. A falha não deve ser atribuída indistintamente a todo harness; o OMP tem caminho próprio no fork. Preservar nossas extensões de host tools, perguntas e steering.

### P6 — Files por RPC e carregamento por diretório

Fontes: [PR #130](https://github.com/zeronsh/comet/pull/130), [PR #287](https://github.com/zeronsh/comet/pull/287).

Upstream acrescentou árvore remota por RPC, paginação de diretórios, watcher, busca limitada e reconciliação incremental que preserva expansão e scroll. Também trouxe editor completo baseado em `gpui-component`, salvamento e reconciliação de alterações externas.

Nosso `crates/ui/src/details_sidebar/view.rs:907` recusa Files de outro device e usa `scan_checkout` para revarrer a árvore local. Já mantém linhas visíveis durante refresh silencioso; portanto essa parte do comportamento não é novidade faltante. A diferença útil é acesso remoto e atualizações menores por diretório.

Recomendação: aproveitar contratos RPC, paginação e reconciliação dentro de nossa árvore/preview. Importar o editor inteiro acrescenta outra biblioteca e altera a experiência que o usuário acabou de padronizar. Preservar o resolvedor de links que aceita arquivos locais absolutos fora do checkout; não substituí-lo por uma política restrita ao workspace.

## Novidades úteis para uma segunda etapa

- **P7 — Browser e preview de servidor de desenvolvimento:** [#282](https://github.com/zeronsh/comet/pull/282), [#283](https://github.com/zeronsh/comet/pull/283), [#290](https://github.com/zeronsh/comet/pull/290), [#295](https://github.com/zeronsh/comet/pull/295), [#297](https://github.com/zeronsh/comet/pull/297). Descoberta automática de Vite/Next/HTTP, URLs estáveis, HMR, browser nativo e acesso entre devices. Integração grande: crate `preview`, WebRTC, RPC e PreviewRoom no Worker. Acesso remoto depende de conectividade ICE e não inclui fallback TURN; não confundir isso com abrir um relatório HTML local, que já temos.
- **P8 — Histórico Git interativo:** [#80](https://github.com/zeronsh/comet/pull/80). Busca, navegação no grafo, modo de pontas de branches e colunas configuráveis. Nosso histórico já tem paginação/grafo, mas não esse conjunto de controles. Adaptar os RPCs ao nosso `ProcessRunner` e ao painel existente.
- **P9 — iOS:** [#266](https://github.com/zeronsh/comet/pull/266), [#268](https://github.com/zeronsh/comet/pull/268), [#269](https://github.com/zeronsh/comet/pull/269), [#270](https://github.com/zeronsh/comet/pull/270). Correções de streaming, teclado, rolagem, limpeza do composer e abertura de grupos de tools. Úteis se o cliente iOS for prioridade; não corrigem diretamente o desktop.

## Mudanças que não recomendo importar integralmente

- [#284](https://github.com/zeronsh/comet/pull/284): obriga fila durante turno ativo e remove steering do composer. Conflita com o fluxo atual de intervenção no OMP e Live Voice. Uma fila editável pode ser considerada separadamente, preservando steering.
- [#244](https://github.com/zeronsh/comet/pull/244): introduz scroll horizontal e toggle em codeblocks do Chat. Preservar o requisito explícito deste fork de quebra de linha sem scroll lateral. O conserto de scroll de diffs do painel Changes (#279) é outra surface.
- Tema, tool labels, arranjo do painel lateral e editor completo: conservar as decisões locais; transportar o mecanismo útil sem sobrescrever o desenho do fork.
- Anel de contexto não é simplesmente uma feature faltante: já temos `context_usage` persistido/sincronizado nas Session rows e ação de compactação. O upstream usa outra representação no documento do Chat; uma substituição exige reconciliação de contratos, não duplicação.
- Bumps de versão, telemetry da landing e workflows de deploy/release não acrescentam as melhorias de uso priorizadas aqui.

## Sequência sugerida

Primeiro P1 e a parte de cache/foco de P3; depois P4/P5. Isolar P2 por envolver o renderer e suas provas nativas. P6 vem após estabilizar essa base; P7–P9 são iniciativas independentes. Cada integração deve partir de um diff direcionado com os testes relevantes, mantendo Workers, OMP, Live Voice, temas e previews do fork.

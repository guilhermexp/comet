# Design

## Context

Ver `proposal.md`. O relato e a imagem são a reprodução do usuário; a inspeção foi somente leitura, sem enviar mensagem aos workers afetados.

Observado em 2026-11-09:
- Worker Pi `083b099a-3c69-4280-b5dd-2402ae0bc67c`: resposta final visível em `inspect_worker`; journal contém Start/Stop da geração 1 e episódio 1. O Stop ocorreu em 1794223686050.
- A extensão compartilhada usa `agent_settled` para Pi e `agent_end` com guards de continuação para OMP. Não são callbacks intercambiáveis; o Stop Pi já chegou ao journal.
- `parent_notifications.rs:729-744` exige 2.000 ms sem alteração de `output.bin`; a medição em 1794224524916 encontrou saída com idade 7,1 ms, após o Stop. A saída também cresceu de 69.124.110 para 69.301.485 bytes entre observações durante a investigação.
- O mesmo arquivo protege episódios/gerações em `hook_events`, e consulta evidência tanto em `pending_from_state` quanto em `episode_completed_from_state` e no latch após ACK.
- Durante a inspeção foi submetido um episódio 2 ao worker (1794224488521). O Stop do episódio 1 NÃO pode ser usado para concluir esse novo pedido. Não alterar o estado real para tentar recuperar a notificação antiga.

[INFERENCE] A exigência de silêncio da PTY pode impedir indefinidamente a conclusão Pi apesar do Stop primário; confirmar com regressão no cálculo real de evidência antes de editar o comportamento. O timestamp da PTY não prova atividade semântica do agente.

## Goals / Non-Goals

**Goals:** corrigir a decisão e o retorno ao pai no seam existente; preservar isolamento, conteúdo da resposta, fronteiras e ACK.

**Non-Goals:** reescrever extensão por semelhança com OMP; mudar outros providers, protocolo/CRDT, cadências, watchers ou retries; editar perfis/hooks/estado instalado; alterar projetos dos workers reais; resolver Blaze, Context externo ou publicar o app nesta change.

## Decisions

### D1 — Término primário validado é a autoridade, não o repaint

Separar o repaint incidental da TUI da evidência semântica aceita para Pi, no menor helper local compartilhado necessário. Notificação pendente, `completed` e latch pós-ACK devem concordar; não marcar `output_quiescent=true` artificialmente nem inferir sucesso de `idle`. Manter contratos atuais de outros runtimes. Mudança de helper público exige referências e migração dos consumidores; preferir seam privado existente.

### D2 — Guards existentes continuam antes da conclusão

Runtime/geração/episódio atuais, task efetivamente submetida, atividade blocked/working e async real continuam bloqueando conclusão quando aplicável. A leitura de background tasks não pode ser omitida por curto-circuito só porque a PTY ainda escreve. Manter o mecanismo existente de graça para serviços sem ampliá-lo ou criar outro temporizador. Um novo Start/pedido invalida o Stop anterior.

### D3 — Provar o seam de produção e a resposta entregue

Usar temporários sintéticos: Start/Stop atual, repaints depois do Stop e resposta final conhecida. Exercitar a derivação real de evidência junto da elegibilidade, ACK e espera; injetar sempre `WorkerCompletionEvidence::quiescent()` não reproduz o defeito. Conferir também conteúdo da notificação por caminho existente, sem cópia de código nem dados dos projetos reais como fixture. Se o tail perder a resposta devido a repaint, corrigir apenas o seam existente de extração necessário para essa reprodução, preservando sanitização e limites.

### D4 — Entrega incremental na worktree já existente

Base `5176ebf0643efa1d56d6b217ca71bb31fee8a893`; preservar código anterior, hook alheio e principal divergente. Alvos principais: `crates/workers-unpeel/src/parent_notifications.rs`, seus testes, regressão do controller se necessária, AGENTS dono e `fork_changelog.md`. Extensão, catálogo e engine ficam somente leitura salvo evidência concreta que exija revisão do plano. Não reconstruir o target de 45 GiB removido: Cargo nativo, jobs=2, sccache e target privado da nova tarefa, sem gpui/workspace amplo.

## Risks / Trade-offs

- R1: relaxar o gate global faria Codex ou outro runtime concluir cedo; limitar a autoridade comprovada do Pi e manter regressões dos demais.
- R2: confiar em Stop antigo entregaria resposta para tarefa errada; negativos de geração/episódio e novo Start são obrigatórios.
- R3: testar só payload/formatter esconderia o bug; exigir cálculo real de evidência e uma única notificação consumível pelo pai.

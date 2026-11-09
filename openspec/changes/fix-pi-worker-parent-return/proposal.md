# Proposal

## Why

O usuário relata que workers Pi terminam e exibem resposta, mas não retornam automaticamente ao Chat pai, enquanto os outros runtimes funcionam. O journal real contém Start e Stop válidos; portanto a correção deve alcançar a decisão de conclusão e entrega, não apenas reinstalar hooks.

## What Changes

- Garantir que conclusão primária válida do Pi possa notificar o pai mesmo com repaints da TUI, sem tratar `idle` sozinho como conclusão.
- Preservar runtime, geração, episódio, bloqueio/pergunta, trabalho assíncrono real, ACK e entrega única; um Stop antigo não pode concluir uma tarefa nova.
- Manter o comportamento dos demais runtimes e comparar explicitamente Pi/OMP antes de alterar a integração compartilhada.
- Provar o caminho entre evidência de término, notificação/espera `completed` e conteúdo entregue ao pai com dados sintéticos isolados.

## Capabilities

### New Capabilities

Nenhuma.

### Modified Capabilities

- `pi-worker-lifecycle-hooks`: explicitar que o término primário notificável não pode depender de silêncio incidental da TUI.

## Impact

Foco em `crates/workers-unpeel/src/parent_notifications.rs`, `tests/parent_notifications.rs`, regressão do controller quando necessária, AGENTS dono e changelog. A extensão compartilhada e o catálogo Pi/OMP são referências; só serão alterados se uma regressão comprovar defeito neles. Sem wire/CRDT novo, dependência nova, polling adicional, recuperação artificial de tarefa, edição dos workers reais ou alteração do app instalado.

Base local: `5176ebf0643efa1d56d6b217ca71bb31fee8a893`, na worktree existente das correções. Preservar as entregas anteriores, o hook preexistente e a principal divergente. Este artefato registra planejamento; aplicação e executor serão confirmados após apresentação do plano.

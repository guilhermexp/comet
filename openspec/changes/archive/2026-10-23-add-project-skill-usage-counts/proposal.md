# Proposal

## Why

A lista de Context sources mostra quais skills estão disponíveis no projeto, mas não permite saber quais o harness realmente vem acionando. Um total histórico discreto na mesma linha permite identificar skills frequentes e skills sem uso registrado sem abrir cada Chat.

## What Changes

- Acrescentar `xN` à direita de cada skill já listada em Details → Session → Project → Context sources, preservando ordem, abertura do arquivo, colapso e rolagem existentes.
- Somar invocações detectadas no Chat Transcript de todos os Chats associados ao mesmo Project, incluindo Chats arquivados e checkouts vinculados, não apenas o Chat selecionado.
- Recuperar os usos identificáveis no histórico existente e atualizar o total quando novos usos forem registrados, sem duplicar snapshots, resultados ou replays da mesma invocação.
- Exibir `x0` somente quando a cobertura do histórico estiver completa e não houver uso registrado. Carregamento, histórico parcial e indisponibilidade não serão apresentados como zero confirmado.
- Explicar em tooltip que a medida é de usos registrados no projeto, não de execução correta das instruções da skill.

## Capabilities

### New Capabilities

Nenhuma.

### Modified Capabilities

- `details-sidebar`: acrescentar a agregação histórica por Project e sua apresentação compacta nas linhas de Context sources.

## Impact

- UI: `crates/ui/src/details_sidebar/session_card.rs` e estado/assinaturas do painel; reutilizar a semântica de skill já usada nas activities do transcript.
- Backend e contrato: projeção do Chat Transcript, associação pelo registro único de Projects e um snapshot/stream tipado engine → UI para contagens e cobertura, sem novo serviço de analytics.
- Não consumir Run Journal nem arquivos privados dos providers para reconstruir histórico; não criar dependências externas, upload de telemetria, ranking, filtros, botão de reset ou nova tela.
- CLI Workers independentes e seus transcripts não entram neste total; o universo é o dos Chats do Project. Não somar registros de outros Projects por nome ou prefixo de pasta.
- Execução futura: implementar e verificar na box `agents-us`, em branch/worktree isolada; depois conferir a entrega, integrar na `main` do fork e publicar uma nova release macOS pelo fluxo existente de atualização automática. A etapa atual cria somente os artefatos para revisão, sem iniciar agente na box, merge, tag ou build.

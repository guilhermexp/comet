# Proposal

## Why

O dono aprovou seis correções desktop observadas no uso diário (IDs estáveis W1, W2, C1, B1, B2, U1). Cada uma tem causa de fonte identificada no mapa de baseline `0d7261cb`; nenhuma pede capability nova, só correção de comportamento das existentes.

## What Changes

- **W1 — worktree location transacional.** Settings → General → Custom worktree location nunca apresenta um "on" pendente como salvo. Ligar pela primeira vez escolhe/valida uma pasta e persiste um estado utilizável; cancelar/falhar mantém o estado persistido anterior. Pasta escolhida e escolha ligada/desligada sobrevivem a reload/restart/update. A raiz efetiva de criação de worktrees de Chat e de Workers segue a preferência salva, preservando a precedência documentada do override de ambiente. Nunca mover/apagar worktrees nem escolher pasta arbitrária.
- **W2 — linhas estáveis no menu bar de Workers ativos.** Dentro de cada seção semântica, ordem de criação com desempate por identificador; Workers novos entram no fim; atualizações de atividade não reordenam. Transições reais de seção/status podem mover a linha. All recent e a sidebar de Workers mantêm a ordenação por atividade.
- **C1 — card de Pasted Text dentro do composer.** O strip de anexos staged passa a ser filho da mesma pílula do composer (borda/fundo), acima do texto editável, com altura reservada pelo layout natural. Conteúdo, remoção, envio, limiares de paste e anexos normais permanecem.
- **B1 — Browser fecha Details.** Ativar o painel Browser (criação, nova aba, navegação, seleção de aba existente) fecha Details pela sequência existente de fechar/persistir. Previews de arquivo (PDF, imagem, CSV etc.) não herdam. Reabrir Details manualmente continua possível.
- **B2 — prioridade do browser interno.** Links HTTP(S) comuns do Chat já preferem o browser interno; o link de servidor em Details → Servers passa a abrir pelo mesmo roteador de links (contexto do Chat/host, proxy de preview remoto), não por `open_url` externo direto. Preferência explícita do usuário, ações explícitas Externo/Copiar, PR e esquemas especiais/OAuth permanecem. Comparação dirigida com o upstream identificado registrada no design.
- **U1 — reconciliação do update OMP em espera.** Enquanto um update aguarda leases de execução (WaitingForIdle), a verificação de binário externo reconcilia uma versão já instalada externamente: cancela a espera pendente, libera o estado e marca Current sem matar o runtime ativo. Timeout/falha do instalador mantém contexto parcial limitado e redigido.

## Capabilities

### Modified Capabilities

- `worktree-lifecycle`: preferência de raiz persistida e transacional, aplicada aos dois criadores.
- `workers-widget-interaction`: ordem estável do menu bar de Workers ativos.
- `composer-intake`: posição do card de texto colado dentro do composer.
- `dev-server-preview`: Browser fecha Details; link de servidor usa browser interno.
- `agent-cli-updates`: reconciliação durante espera e diagnóstico limitado de falha/timeout.

## Impact

- `crates/ui/src/settings/worktrees.rs`, `crates/engine/src/worktree_settings.rs`, `crates/workers-unpeel/src/{lib.rs,worktree_ownership.rs}` (W1).
- `crates/ui/src/workers/activity_menu.rs` (W2).
- `crates/ui/src/composer.rs` (C1).
- `crates/ui/src/shell.rs`, `crates/ui/src/details_sidebar/{dev_servers.rs,view.rs}` (B1/B2).
- `crates/engine/src/harness_updates.rs` (U1).
- Sem mudança de wire nem de formato persistido existente (campos novos são opcionais). Aceite nativo macOS (menu bar, composição gpui, WebKit, updater real) fica para a validação local depois da entrega.

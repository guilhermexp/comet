# Proposal

## Why

A metadata da sidebar afasta projetos da branch e o título superior expõe Markdown e caminhos que a sidebar já apresenta de forma compacta. A investigação de títulos automáticos também encontrou um comportamento anterior à v1.0.15: uma falha de geração grava as primeiras palavras como título definitivo e bloqueia tentativas normais posteriores.

## What Changes

- Agrupar branch e projetos por proximidade, sem a expansão artificial da branch curta; preservar uma linha, truncamento, PR, controles e os modos normal, Compact e Archived.
- Projetar menções no título superior com o presenter textual compacto existente, mantendo tipografia, controles e o valor durável original. O topo exibirá `studio`, não a URI completa.
- Persistir como título automático somente uma saída válida do gerador. Uma geração malsucedida manterá o Chat sem título definitivo, com o placeholder visual existente, para que os gatilhos normais posteriores possam tentar novamente. Não criar timer, nova ladder, troca de provedor ou campos de proveniência.
- Preservar títulos manuais e todos os títulos já gravados, aplicando um resultado automático somente se o Chat ainda estiver sem título no momento da gravação. Não tentar adivinhar quais títulos antigos vieram de fallback nem renomear conversas em massa.
- Provar os defeitos com casos sintéticos, observar RED/GREEN e validar a apresentação em fixture nativa sem engine, providers ou Workers vivos.

## Capabilities

### New Capabilities

Nenhuma.

### Modified Capabilities

- `chat-navigation-and-sidebar`: explicitar adjacência entre branch e projetos e apresentação compacta de menções no título superior, sem alterar a persistência.
- `upstream-followup-correctness`: esclarecer que o fallback de geração restrita não é um título definitivo e não bloqueia uma tentativa normal posterior.

## Impact

Alvos de implementação: `crates/ui/src/shell.rs`, `crates/ui/src/shell/tabs.rs`, suas fixtures em `shell/spaces.rs`, presenter existente em `composer.rs` e `crates/engine/src/titles.rs` com aplicação condicional no `workspace_host.rs`. Documentação dona de UI/engine, changelog e as duas specs acompanham a correção. Não há nova dependência, mudança de schema/RPC, acesso a credenciais, alteração de contas ou reescrita de Chats existentes.

A lacuna visual do topo e o comportamento do gerador são anteriores à v1.0.15; apenas o espaçamento foi introduzido na mudança recente de projetos na sidebar. A causa operacional inicial de cada geração falha permanece não atribuída a um provedor sem evidência específica.

## Non-goals

Redesenhar a sidebar, alterar atribuição de projetos, adicionar chips interativos ao topo, renomear automaticamente branches a partir de fallback, corrigir consumidores visuais não solicitados, instalar o app ou publicar outra release nesta etapa. O destino proposto é correção revisada e comprovada, integrada na main do fork; tag/release/instalação dependem de autorização própria.

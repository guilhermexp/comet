# Change: Um ciclo de vida de worktree para Chat e Workers, com integração seletiva ao Worktrunk

## Why

O Comet ainda tem dois caminhos de criação e remoção: `Repos` na engine para Chats e
`unpeel_core::worktrees`/`checkout_lifecycle` para Workers. Eles usam raízes, setup e regras de
remoção diferentes. Em particular, o RPC `DeleteWorktree` da engine usa `--force`, tem fallback
recursivo e apaga branch `zeron/*`; o lado Workers exige checkout próprio, sem Worker vivo e árvore
limpa. O setup configurado em Settings ▸ Projects não roda nos worktrees de Chat.

O objetivo original também inclui agentes em paralelo: hoje o `launch_worker` do orquestrador só
seleciona um worktree existente. Sem criação isolada na mesma operação, Workers independentes podem
ser lançados no mesmo checkout.

O [Worktrunk](https://worktrunk.dev/) oferece ideias úteis para esse ciclo: hooks de projeto,
aprovação de comandos e criação de worktree antes de lançar um agente. Esta change implementa um
**subconjunto documentado** dessas ideias dentro do Comet. Ela não promete paridade geral com o
binário `wt` nem exige sua instalação para o ciclo nativo.

## What Changes

- Um serviço local em `crates/workers-unpeel` passa a criar, preparar e remover worktrees para Chat
  e Workers. Todos os worktrees novos usam `~/.zeron/worktrees/<repo-slug>-<hash>/<nome>`; o override
  `ZERON_WORKTREES_DIR` vale para os dois. Nenhuma pasta existente é movida.
- **Localização e posse são fatos distintos.** Estar sob a raiz canônica ou legada não autoriza
  remoção. Worktrees novos recebem prova local durável de criação e identidade Git. Para legados,
  só evidência de posse já registrada é aceita; origem incerta continua utilizável, mas não pode
  ser apagada pelo app até recuperação explícita. Isso vale também para `DeleteWorktree` remoto.
- Criação para Chat e Worker usa o mesmo setup `.comet/worktree.json`/`.cursor/worktrees.json`.
  Falha preserva o checkout e sua associação, informa o comando e **bloqueia o primeiro run/launch**
  até correção ou retry. Um run posterior reutiliza o checkout.
- Remoção exige checkout próprio, worktree Git linkado, nenhuma execução local ativa e ausência de
  mudanças locais, sem `--force` e sem fallback recursivo. A branch local é preservada em todos os
  casos nesta change. O serviço conserva o histórico do projeto Workers.
- O estado de execução local de Chats e Workers é consultado pelo serviço em **todas** as entradas
  de remoção, inclusive controller MCP e RPC, sob a mesma coordenação de criação/launch/remoção.
  Retarget local para um checkout com Worker vivo é recusado.
- O Comet lê os hooks de projeto `pre-start`, `post-start`, `pre-remove` e `post-remove` de
  `.config/wt.toml`: formatos string, tabela e pipeline, com um subconjunto explícito de templates.
  Comandos precisam de aprovação local; comandos alterados precisam de nova aprovação. `pre-*`
  bloqueia a operação; `post-*` roda em segundo plano com log. Comando não aprovado ou template não
  suportado produz erro visível, sem execução parcial. O usuário pode escolher explicitamente uma
  remoção sem hooks, equivalente ao `--no-hooks` do `wt`.
- `launch_worker` ganha uma opção para **criar e registrar um worktree novo antes de lançar** um
  preset. Ela é mutuamente exclusiva com `worktree_path`/`worktree_branch` existentes. Falha de
  setup/hook impede o launch, mas conserva o checkout para recuperação.

## Compatibility boundary

- Na criação iniciada pelo Comet, o arquivo `.config/wt.toml` vem do checkout de origem selecionado;
  na remoção, vem do checkout removido. O Worktrunk lê a configuração do worktree em que seu comando
  foi invocado; os resultados coincidem quando esses checkouts são os mesmos. O Comet não lê os
  hooks de usuário de `~/.config/worktrunk/config.toml`.
- O parser aceita os três **formatos** de hook, mas só as variáveis e filtros listados na spec.
  Jinja condicionais, `vars.*`, funções, aliases e demais filtros do `wt` não são compatíveis nesta
  etapa. Um hook que os use é recusado com o token identificado.
- Hooks são comandos shell como no `wt`; um comando que invoca `wt` precisa que esse binário esteja
  instalado. O ciclo nativo do Comet e os hooks sem essa chamada não precisam dele.
- `.worktreeinclude` e `wt step copy-ignored` **não** acionam cópia automática no Comet nesta
  change. Caches ignorados são dados locais: a futura cópia precisa especificar proveniência,
  limpeza segura e impacto na regra de remoção antes de entrar.
- A exclusão automática de branch integrada e o fluxo `wt merge` ficam para uma change posterior,
  junto com status por checkout. A primeira entrega conserva todas as branches.
- O layout configurável `worktree-path` do `wt` não substitui a raiz canônica do Comet. Worktrees
  criados pelo `wt` continuam externos e podem ser adotados para execução, sem adquirir posse.

## Capabilities

### New Capabilities

- `worktree-lifecycle`: criação, identidade de posse, setup, hooks selecionados, estado local de
  execução e isolamento de Workers no ciclo comum de Chat e Workers.

### Modified Capabilities

- `worktree-deletion-safety`: o RPC usa a política comum, prova de posse e preserva a branch.
- `workers-repository-identity`: remoção considera Chats locais ativos e exige prova de posse,
  inclusive nas raízes legadas.
- `chat-checkout-control`: Retarget local é recusado quando um Worker vivo ocupa o destino.

## Impact

- `crates/workers-unpeel`: serviço comum, registro local de posse, consulta de atividade,
  hooks/aprovações e caminho atômico de criar worktree e lançar Worker no controller MCP.
- `crates/engine`: criação e remoção delegadas ao serviço, associação do Chat preservada após
  falha de setup/hook e registro da atividade de runs locais.
- `crates/ui`: aprovação/diagnóstico de hooks em Settings ▸ Projects e mensagem de Retarget.
- `third_party/unpeel`: sem patch planejado; o Comet deixa de chamar o `create` vendorizado.
- Testes de integração cobrem posse de worktree externo **dentro** da raiz, entrada remota de
  `DeleteWorktree`, atividade local, falhas de preparo e lançamento isolado pelo orquestrador.

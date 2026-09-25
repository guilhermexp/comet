## Context

Ver `proposal.md` para a motivação. `engine` já depende de `zeron-workers-unpeel`; a fronteira
Workers não pode depender da engine. O serviço Git comum fica em `crates/workers-unpeel`, e a
engine o chama fora do executor async, com timeout. O registro Workers é local (`app-state.json`)
e os Chats vivem no doc sincronizado: unificar o ciclo não unifica esses registros.

Hoje `checkout_lifecycle::validate_removal` recebe `owned` de um registro Workers, além de testar
raiz, Git e árvore limpa. A engine não dispõe dessa prova; estar sob uma raiz conhecida não a
substitui. O `CheckoutActionLock` já serializa algumas ações Workers, mas todas as entradas de
criação, launch e remoção precisam participar para a checagem de atividade valer.

## Goals / Non-Goals

**Goals:** um serviço para criação, preparo, atividade e remoção; prova de posse para ambos os
chamadores; criação isolada pelo controller MCP; suporte declarado aos quatro hooks de início e
remoção do `.config/wt.toml`.

**Non-Goals:** mover worktrees existentes, unificar registros de Chat e Workers, implementar o
binário `wt`, seus hooks de usuário, `wt merge`, deleção automática de branch, cópia automática de
ignorados ou `worktree-path` configurável.

## Decisions

### D1. Raiz comum e prova de posse separada

Worktrees novos usam `ZERON_WORKTREES_DIR` não vazio ou `~/.zeron/worktrees`, em
`<slug>-<fnv1a:016x>/<nome>`, com hash do caminho canônico do repositório. A localização serve
apenas como limite adicional de segurança. A autorização de remoção exige ainda prova durável de
que o Comet criou **aquele** checkout do **mesmo** repositório Git: caminho canônico, identidade do
Git common dir e identificador administrativo do worktree. A branch observada é metadado, não
critério de posse, pois o Chat a renomeia pelo título e o usuário pode trocá-la. Antes de
`git worktree add`, o serviço grava um journal `PendingCreate` com identidade do repo, caminho
reservado e ID da operação. Depois do add, grava `CreatedObserved` com a identidade Git observada
e finaliza `Owned`, antes de qualquer setup. Só `CreatedObserved` permite reconciliação automática
após nova verificação Git. Se essa gravação também falhar, o checkout fica no disco sem posse
presumida e exige recuperação explícita; nenhum rollback usa `--force`.

O índice local de posse fica em arquivo próprio sob `~/.zeron` (path injetável nos testes), com
escrita atômica e protegido pelo `CheckoutActionLock`. Ele não entra no CRDT. A prova registrada
inclui a identidade Git observada; uma troca de pasta ou de Git common dir invalida a autorização
até recuperação explícita.

Para Workers legados, o `CheckoutOwnership::AppManaged` e a evidência Git já guardados no registro
podem ser migrados para a prova comum após revalidação. Para Chat legado sem proveniência durável,
raiz e branch `zeron/*` não bastam: o checkout continua utilizável, mas a remoção pelo app é recusada
com orientação de recuperação explícita. Worktrees externos, mesmo dentro de uma raiz conhecida,
continuam `External`. `DeleteWorktree` primeiro resolve o alvo contra `git worktree list` do repo
pedido; só depois usa a prova comum. Esta regra vale para chamadas locais e forwardable.

Alternativa rejeitada: inferir posse do prefixo do caminho. Qualquer ferramenta pode criar um
worktree dentro da raiz, e um symlink pode confundir um teste textual.

### D2. Um fluxo de criação com associação antes do preparo

`checkout_lifecycle` ganha a operação comum. Chat fornece seu nome `zeron/<adj-noun>`; Worker fornece
a branch escolhida. O serviço preserva a seleção de base/fetch best-effort usada pelo caminho
Workers. Ordem: lock → escolher caminho e verificar colisão → gravar `PendingCreate` →
`git worktree add` → gravar `CreatedObserved` → finalizar `Owned`, associação do chamador e
estado `Preparing` → liberar
lock → setup `.comet`/`.cursor` → hooks
aprovados → encerrar `Preparing` sob lock → resultado. A remoção trata `Preparing` como ocupado;
um setup de até cinco minutos não precisa manter o lock global durante toda sua execução.
Um checkout já registrado é reutilizado, com a mesma prova, sem rodar `post-start` duas vezes.
Se o caminho já contém um worktree que outra ferramenta criou, o serviço pode associá-lo como
`External`, sem criar journal `Owned` nem executar hooks `*-start` ou setup de criação sobre ele.

Setup ou `pre-start` com erro **não** desfaz o checkout. O Chat grava o `cwd` antes de reportar erro,
mostra o comando e não inicia o harness; o Worker registra o projeto e não lança o preset. O
próximo envio/launch pedido pelo usuário tenta o preparo novamente no mesmo checkout; não cria
outro. `post-start` só começa após os passos bloqueantes concluírem. Isso mantém o worktree
recuperável e preserva a semântica
bloqueante de `pre-start`.

Alternativa rejeitada: deixar o Chat executar depois de setup falho. O agente receberia um checkout
que o usuário configurou como dependente daquele preparo.

### D3. Estado local de execução é consultado pelo serviço

Uma fonte device-local de atividade, mantida pela engine host, registra o caminho canônico dos
Chats `Working` e o identificador do run. O início do run e a remoção usam o mesmo
`CheckoutActionLock`: o run registra atividade antes de começar a escrever; a remoção mantém o lock
até `git worktree remove` terminar. A engine encerra a marca ao assentar; se morrer, a fonte usa
lease/heartbeat com reconciliação contra a instância do host antes de autorizar exclusão. Expirar
o heartbeat, sozinho, **não** prova que o run morreu: se o host ou o estado real não puderem ser
consultados, a remoção falha fechada. Estado indisponível ou ambíguo nunca vira checkout livre.

`Preparing` e `StartingWorker` também são estados ocupados. O controller marca `StartingWorker`
antes de liberar o lock para spawn e só o encerra quando o Worker consta como vivo ou o launch
falha. Assim não existe janela entre preparo e registro do processo na qual uma remoção possa
apagar o checkout.

O serviço soma esse estado aos Workers vivos do `LocalWorkersClient`; UI, cliente Workers direto e
RPC não podem fornecer uma lista opcional de Chats que permita contornar a verificação. Retarget local usa a
mesma consulta de Workers. Um Chat hospedado em outro device continua com a limitação descrita em
`chat-checkout-control`: sua `Mutate::SetChatCwd` não é encaminhada ao host nesta change.

Alternativa rejeitada: a UI passar `busy_chat_cwds` para `remove_worktree`. Um chamador direto do
cliente Workers e o RPC da engine não passam pela UI.

### D4. Remoção segura, branch preservada

Sob o lock: resolver o worktree do repositório pedido; confirmar prova de posse; conferir atividade;
confirmar checkout linkado e HEAD retido; carregar/aprovar/renderizar `pre-remove`; executá-lo;
encerrar processos `post-start` iniciados pelo Comet; **revalidar** identidade, atividade e
limpeza; então `git worktree remove` sem `--force`. O hook
pode limpar caches ignorados antes da verificação de limpeza, mas qualquer untracked/ignored que
restar bloqueia. Não há `remove_dir_all` como fallback. Um worktree já ausente pode ter o registro
Git podado depois de verificar sua identidade; nenhuma pasta é apagada nesse caminho.

A branch local fica sempre. Isso evita decisões de integração baseadas em heurística nesta primeira
entrega e torna o resultado igual para Chat e Workers. A regra de `wt remove` (incluindo seus seis
critérios e a preferência por upstream à frente) será especificada junto do futuro status/merge.

Alternativa rejeitada: portar cinco dos seis critérios do `wt` agora. Seria uma política diferente
com o mesmo nome, e um erro pode apagar o último nome de uma branch útil.

### D5. Hooks de projeto do Worktrunk: subconjunto explícito

O Comet lê `.config/wt.toml` do checkout de origem selecionado para criação e do checkout removido
para remoção. Suporta `pre-start`, `post-start`, `pre-remove`, `post-remove` como string, tabela de
comandos concorrentes ou array de tabelas em sequência. Aceita só as variáveis `branch`,
`worktree_path`, `worktree_name`, `repo`, `repo_path`, `primary_worktree_path`, `commit`,
`short_commit`, `base`, `default_branch`, `hook_type`, `cwd` e o filtro `sanitize`. A expansão
recebe escaping próprio para o contexto shell; cada formato e token é validado **antes** de rodar
qualquer comando de um hook. Filtros/funções/condicionais não suportados, inclusive `hash_port` e
`vars.*`, produzem erro nomeando o token. Isso é compatibilidade parcial, não interpretação geral
de Jinja. Comandos shell que chamam `wt` continuam dependendo do binário externo.

`pre-*` executa com timeout e aborta a operação em erro. `post-*` roda após sucesso em processo
separado, com log por checkout. `post-remove` executa no checkout principal, pois o alvo já saiu.
Comandos de `post-start` iniciados pelo Comet têm processo rastreado para encerramento na remoção;
processos de outras ferramentas não são terminados implicitamente.

A aprovação é local e vinculada à identidade do repositório e ao texto de cada comando; mudança
requer nova aprovação. Settings ▸ Projects mostra a origem e o comando. `pre-start` ou `pre-remove`
pendente bloqueia run/launch ou remoção; `post-*` pendente é pulado com aviso. Há opção explícita
"remover sem hooks" que pula ambos os hooks de remoção, com confirmação e auditoria, sem pular as
checagens de posse/atividade/limpeza. O setup `.comet` segue o contrato de confiança já existente.
Cada execução usa o snapshot de comandos cujo hash foi aprovado: reler o arquivo depois da
aprovação nunca troca silenciosamente o comando a executar. Aprovar em Settings exibe o texto
exato desse snapshot.

Alternativa rejeitada: anunciar que todos os templates do `wt` funcionam. O formato TOML não
implica suporte a todo o motor Jinja nem ao estado `vars.*` do Worktrunk.

### D6. Isolamento no controller MCP

`launch_worker` aceita opcionalmente `new_worktree: { branch, base_ref? }`, mutuamente exclusivo com
`worktree_path`/`worktree_branch`. O controller valida preset, projeto Git e branch antes de criar;
usa o fluxo comum de criação/registro/preparo e só então lança o preset no caminho retornado. A
operação retorna ID do projeto, caminho, branch e ID do Worker. Falha de preparo ou launch deixa o
checkout registrado e informa como reutilizá-lo; nunca o remove automaticamente. O briefing do
orquestrador orienta usar `new_worktree` para fatias independentes. Lançamento no checkout atual
continua possível quando explicitamente desejado ou quando o projeto não é Git.

Alternativa rejeitada: shell out para `wt switch -x`. Isso introduz dependência de runtime, outro
layout de pastas e regras de posse distintas conforme a máquina.

## Risks / Trade-offs

- **Legados de Chat sem prova não são removíveis pelo app** → fail closed com mensagem de recuperação;
  nenhum diretório é movido ou assumido como próprio por nome/prefixo.
- **Setup pode gerar arquivos ignorados** → `pre-remove` pode limpá-los antes da verificação final;
  sem hook, o usuário preserva ou limpa os arquivos explicitamente antes de remover.
- **Lease de Chat não pode dar falso livre** → início do run e remoção compartilham lock; leitura
  indisponível bloqueia remoção; testes exercitam início concorrente e crash.
- **Hooks de projeto podem conter recursos do `wt` ausentes** → inspeção em Settings, erro por token
  e diagnóstico antes de qualquer execução parcial.
- **Sem cópia automática de caches** → build inicial ainda pode ser lento. Uma change posterior deve
  definir opt-in, manifesto de proveniência e limpeza segura antes de copiar dados ignorados.

## Migration Plan

1. Introduzir prova de posse e a fonte de atividade com testes de concorrência, mantendo os dois
   chamadores existentes.
2. Conectar Workers e engine ao serviço; migrar somente evidência Workers comprovada e deixar
   legados de Chat de origem incerta em estado não removível.
3. Conectar hooks e aprovação, depois `launch_worker.new_worktree`; validar cada etapa pelos testes
   da spec e pelos fluxos visuais reais.
4. Se a implantação precisar de rollback, preservar os registros de posse/atividade novos; versões
   antigas simplesmente não os leem. Não mover nem limpar worktrees durante rollback.

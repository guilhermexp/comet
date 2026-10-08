# Comet Domain Language

Comet coordinates native agent chats, local CLI Workers, and device-local provider account state.

## Provider Usage

**Managed Provider Usage**:
Subscription quota windows reported by a provider's managed coding platform for the current device account. It is not API billing and never enters synced chat state.
_Avoid_: API usage, token billing, synced usage

**Kimi Code**:
Moonshot AI's managed coding subscription served from the Kimi Code platform. It is distinct from the Moonshot Open Platform API.
_Avoid_: Moonshot API, Kimi API billing


**Grok CLI**:
xAI's grok.com coding subscription authenticated by `grok login` into `$GROK_HOME/auth.json`. It is distinct from a console.x.ai API key in `user-settings.json`.
_Avoid_: Grok API billing, xAI API key, Moonshot-style Open Platform key

**Antigravity Account Pool**:
The set of usable Antigravity credentials discovered from CLIProxyAPI's device-local credential directory. Every credential is independently visible with its own Managed Provider Usage; none is the singular active account because CLIProxyAPI owns routing across the pool.
_Avoid_: active Antigravity account, switchable Antigravity slot, Comet-owned login

## Chat

**Chat**:
A conversa durável entre o usuário e um agente, com título, checkout e histórico próprios. É a unidade que a sidebar lista, que se arquiva e que um Chat Transcript Export produz.
_Avoid_: session, workspace, thread, conversa

**Session**:
O estado de execução de um Chat num device — parado, trabalhando, esperando resposta, ou em erro. É efêmero e por device: um Chat existe sem nenhuma Session viva.
_Avoid_: run state, chat status, sessão (quando se quer dizer Chat)

**Agent-created Chat**:
Chat aberto pela tool `sessions` a partir de um Chat de orquestrador. Grava o Chat pai em `origin_chat_id`, herda a configuração efetiva do pai e nunca recebe a tool `sessions` de novo. Continua recebendo `workers`. Não confundir com o filho do Zeron MCP (`zeron mcp` `create_chat`, `parent_chat_id`), que some da sidebar e escolhe harness/modelo livremente; o filho de `sessions` aparece na sidebar. Nenhum dos dois recebe `sessions`.
_Avoid_: child session, subagent, worker

**Chat Transcript**:
O registro sincronizado de um Chat — mensagens e ferramentas usadas — já filtrado para o que pode ser exibido e sincronizado. É a única fonte de qualquer leitura ou export de um Chat.
_Avoid_: history, messages, doc, conversa

**Run Journal**:
O registro local e cru do que um agente emitiu durante um run, incluindo as entradas de ferramenta que o Chat Transcript remove por privacidade. Existe para retomar um run interrompido, não para ser lido.
_Avoid_: log, transcript, history

**Chat Transcript Export**:
Uma cópia do Chat Transcript num formato levável para fora do comet. Do transcript nunca carrega nada que o Chat Transcript já não mostre; a única fonte adicional é o índice de CLI Workers do Chat, que entra como Artifact.
_Avoid_: chat dump, backup, download, export de sessão

**Artifact**:
Algo substantivo que um Chat produziu — um arquivo escrito, um subagente executado, um CLI Worker despachado, um output pesado o bastante para não caber inline. É o que um Chat Transcript Export lista no topo para o registro ficar navegável.
_Avoid_: output, result, file change

## Checkout

**Chat Checkout**:
O par pasta+ref a que um Chat está fixado — `cwd` e `branch` na row do Chat. Decide onde um run escreve e é o que o card Workspace do Details sidebar mostra. Pertence ao Chat, que é durável; nunca à Session, que é o estado de execução daquele Chat num device.
_Avoid_: workspace, session branch, worktree (quando se quer dizer o par)

**Chat Source Context**:
O snapshot imutável de repo root, cwd, branch, checkout e HEAD observado imediatamente antes do run de um Chat. Identifica a origem daquela conversa mesmo se outra Chat mudar o checkout compartilhado depois; não é o estado Git vivo.
_Avoid_: current branch, live checkout, Session context

**Ref**:
Um branch local ou remote-tracking que o repo do Space oferece como destino de um Chat Checkout. Nunca significa commit solto, tag ou detached HEAD.
_Avoid_: branch (quando se quer dizer a lista de opções), revision, commit

**Retarget**:
Mover um Chat Checkout para outra pasta que já existe — o worktree do Ref escolhido — em vez de trocar o Ref dentro da pasta atual. Custa a continuidade do harness: o próximo run abre conversa nova, porque resume é escopo de cwd.
_Avoid_: switch, move, checkout

## Projects

**Project**:
Um Space do registro único: id, Device dono, pasta e nome. É o que o chat MCP `list_projects`, o controller de Workers `list_projects`, Settings → Projects, a sidebar de Workers (projetos locais) e as menções `@` do composer listam — o mesmo id em todas. Todo "adicionar projeto" (paletas, "+" de Settings, `add_project` do controller) cria ou reusa o Space da pasta; nada cunha identidade de projeto fora dele. O repositório de um worktree linkado pertence ao Project da sua raiz.
_Avoid_: Logical Project, Registered Project, workspace (para o registro); em copy de produto, Space

**Worker Checkout**:
O diretório de execução de Workers (`comet-*`), ligado a exatamente um Project deste device: o checkout principal é a pasta do Project, worktrees linkados do mesmo repositório são checkouts dele. Sem evidência (sem pasta nem repositório persistido) fica em "Association pending" e nunca vira projeto. Disponibilidade, arquivamento e propriedade do diretório são fatos separados: reconhecer um worktree externo não autoriza apagá-lo. Arquivar preserva sessões; remover a pasta não elimina a identidade histórica.
_Avoid_: projeto (para o checkout), novo repositório (quando se trata de worktree do mesmo repositório), main (quando se quer dizer checkout principal)

**Retired Device**:
Um device duplicado ou abandonado aposentado em Settings → Devices: seus Projects cujas pastas existem aqui passam para este device com todos os chats (ids preservados) e ele some de toda lista de devices; reconectar não o traz de volta. Só um device que não é o local e não está online pode ser aposentado.
_Avoid_: deletar device, remover device

**Leaf Root**:
O Worker Checkout cadastrado que não é ancestral de nenhum outro checkout cadastrado — o universo fechado contra o qual o casamento de Worked Projects roda. Um checkout que contém outros é um contêiner e nunca participa de casamento por prefixo, senão engole todo caminho abaixo dele.
_Avoid_: parent project, container, root project

**Worked Project**:
O Leaf Root que contém ao menos um caminho absoluto tocado pelos próprios turnos de assistente de um Chat — leitura, escrita, edição, busca ou comando. É o que o bloco `Projects worked` do card Workspace lista. Deriva só do transcript daquele Chat: nunca de Worker despachado, nunca de subagente, e nunca inclui o Chat Checkout do próprio Chat.
_Avoid_: touched folder, visited project, worker project

## Workers

**Worker** / **CLI Worker**:
A unidade autônoma de execução CLI gerenciada localmente pelo runtime Unpeel em processo dedicado (`__session_host__`), com TUI/viewport de terminal, ciclo de vida de atividade, hooks, presets e servidores MCP próprios em worktree ou pasta de projeto. Pode ser despachado por um Chat ou criado de forma independente; não é uma conversa durável de chat, não é um subagent inline acionado dentro de um turno de agente e nunca sincroniza entre devices.
_Avoid_: Chat, subagent, task worker, background agent, thread

## Voice

**Codex Voice**:
A chamada realtime Codex local ou remota controlada pela engine host, usando o helper da instalação standalone do usuário. Desktop e iOS compartilham lifecycle, captions e estado; uma conversa de voz concluída é persistida uma vez no Chat Transcript. O orquestrador de voz dedicado recebe os tools de Chat pelo grant raiz da engine, sem injeção global de MCP. A integração antiga de Live Voice do OMP foi aposentada.
_Avoid_: ditado local, Worker, billing de API

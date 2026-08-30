# Comet Domain Language

Comet coordinates native agent chats, local CLI Workers, and device-local provider account state.

## Provider Usage

**Managed Provider Usage**:
Subscription quota windows reported by a provider's managed coding platform for the current device account. It is not API billing and never enters synced chat state.
_Avoid_: API usage, token billing, synced usage

**Kimi Code**:
Moonshot AI's managed coding subscription served from the Kimi Code platform. It is distinct from the Moonshot Open Platform API.
_Avoid_: Moonshot API, Kimi API billing

## Chat

**Chat**:
A conversa durável entre o usuário e um agente, com título, checkout e histórico próprios. É a unidade que a sidebar lista, que se arquiva e que um Chat Transcript Export produz.
_Avoid_: session, workspace, thread, conversa

**Session**:
O estado de execução de um Chat num device — parado, trabalhando, esperando resposta, ou em erro. É efêmero e por device: um Chat existe sem nenhuma Session viva.
_Avoid_: run state, chat status, sessão (quando se quer dizer Chat)

**Chat Transcript**:
O registro sincronizado de um Chat — mensagens e ferramentas usadas — já filtrado para o que pode ser exibido e sincronizado. É a única fonte de qualquer leitura ou export de um Chat.
_Avoid_: history, messages, doc, conversa

**Run Journal**:
O registro local e cru do que um agente emitiu durante um run, incluindo as entradas de ferramenta que o Chat Transcript remove por privacidade. Existe para retomar um run interrompido, não para ser lido.
_Avoid_: log, transcript, history

**Chat Transcript Export**:
Uma cópia do Chat Transcript num formato levável para fora do comet. Nunca carrega nada que o Chat Transcript já não mostre.
_Avoid_: chat dump, backup, download, export de sessão

**Artifact**:
Algo substantivo que um Chat produziu — um arquivo escrito, um subagente executado, um output pesado o bastante para não caber inline. É o que um Chat Transcript Export lista no topo para o registro ficar navegável.
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

**Registered Project**:
Uma pasta que o usuário cadastrou no working set de projetos (`WorkersProject`, o que `list_projects` retorna) — o universo fechado contra o qual qualquer derivação de projeto casa. Uma pasta que o agente tocou e não está cadastrada não é um Registered Project e não existe para a UI.
_Avoid_: workspace, folder, repo (quando se quer dizer a row cadastrada)

**Leaf Root**:
O Registered Project que não é ancestral de nenhum outro Registered Project. Um projeto cadastrado que contém outros cadastrados é um contêiner e nunca participa de casamento por prefixo, senão engole todo caminho abaixo dele.
_Avoid_: parent project, container, root project

**Worked Project**:
O Leaf Root que contém ao menos um caminho absoluto tocado pelos próprios turnos de assistente de um Chat — leitura, escrita, edição, busca ou comando. É o que o bloco `Projects worked` do card Workspace lista. Deriva só do transcript daquele Chat: nunca de Worker despachado, nunca de subagente, e nunca inclui o Chat Checkout do próprio Chat.
_Avoid_: touched folder, visited project, worker project

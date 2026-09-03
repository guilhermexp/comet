# Comet Domain Language

Comet coordinates native agent chats, local CLI Workers, and device-local provider account state.

## Provider Usage

**Managed Provider Usage**:
Subscription quota windows reported by a provider's managed coding platform for the current device account. It is not API billing and never enters synced chat state.
_Avoid_: API usage, token billing, synced usage

**Kimi Code**:
Moonshot AI's managed coding subscription served from the Kimi Code platform. It is distinct from the Moonshot Open Platform API.
_Avoid_: Moonshot API, Kimi API billing

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

**Chat Transcript**:
O registro sincronizado de um Chat — mensagens e ferramentas usadas — já filtrado para o que pode ser exibido e sincronizado. É a única fonte de qualquer leitura ou export de um Chat.
_Avoid_: history, messages, doc, conversa

**Run Journal**:
O registro local e cru do que um agente emitiu durante um run, incluindo as entradas de ferramenta que o Chat Transcript remove por privacidade. Existe para retomar um run interrompido, não para ser lido.
_Avoid_: log, transcript, history

**Trajectory**:
A leitura local e técnica dos runs de um Chat capturados no device atual, organizada como timeline e ledger de eventos para auditoria. É uma read model própria: não é o Run Journal bruto, não sincroniza entre devices e protege Payload e Result até uma revelação explícita.
_Avoid_: trace, timeline, log, Run Journal, Chat Transcript

**Degraded Interval**:
O registro explícito de um intervalo de sequência (`from_seq` a `to_seq`) gravado pelo Trajectory store (em memória e persistido em SQLite) quando há perda, saturação ou falha de escrita na captura de eventos de um run, declarando formalmente que o histórico de trajetória está incompleto e o motivo da degradação. Garante que histórico incompleto se declare incompleto em vez de omitir eventos silenciosamente; não é erro fatal de runtime nem corrupção de banco de dados.
_Avoid_: gap silencioso, store error, corrupted trace, missing logs

**Raw Reveal**:
A leitura efêmera, device-local e sob demanda do payload de entrada (`Payload`) ou resultado de execução (`Result`) bruto do Run Journal/Trajectory de um evento específico, acionada explicitamente pelo usuário no Inspector da Trajectory. Nunca é persistido em disco pelo read model, não transita no stream padrão de watch, não sincroniza entre devices e é limpo imediatamente ao trocar a seleção ou fechar a surface.
_Avoid_: payload dump, raw log, unredacted trace, permanent reveal

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

**Registered Project**:
Uma pasta que o usuário cadastrou no working set de projetos (`WorkersProject`, o que `list_projects` retorna) — o universo fechado contra o qual qualquer derivação de projeto casa. Uma pasta que o agente tocou e não está cadastrada não é um Registered Project e não existe para a UI.
_Avoid_: workspace, folder, repo (quando se quer dizer a row cadastrada)

**Leaf Root**:
O Registered Project que não é ancestral de nenhum outro Registered Project. Um projeto cadastrado que contém outros cadastrados é um contêiner e nunca participa de casamento por prefixo, senão engole todo caminho abaixo dele.
_Avoid_: parent project, container, root project

**Worked Project**:
O Leaf Root que contém ao menos um caminho absoluto tocado pelos próprios turnos de assistente de um Chat — leitura, escrita, edição, busca ou comando. É o que o bloco `Projects worked` do card Workspace lista. Deriva só do transcript daquele Chat: nunca de Worker despachado, nunca de subagente, e nunca inclui o Chat Checkout do próprio Chat.
_Avoid_: touched folder, visited project, worker project

## Workers

**Worker** / **CLI Worker**:
A unidade autônoma de execução CLI gerenciada localmente pelo runtime Unpeel em processo dedicado (`__session_host__`), com TUI/viewport de terminal, ciclo de vida de atividade, hooks, presets e servidores MCP próprios em worktree ou pasta de projeto. Pode ser despachado por um Chat ou criado de forma independente; não é uma conversa durável de chat, não é um subagent inline acionado dentro de um turno de agente e nunca sincroniza entre devices.
_Avoid_: Chat, subagent, task worker, background agent, thread

## Voice

**Live Voice**:
A call de voz interativa mantida pela engine host em segundo plano com um agente/runtime (OMP), projetando contexto operacional contínuo (status, texto visível em janela temporal e labels de tool) e despachando comandos vocais confirmados como comandos duráveis `Steer` no Chat ativo. Pertence à engine host, sobrevivendo à troca de Chat ativo, perda de foco ou minimização da janela. Não pertence à surface selecionada nem a uma aba específica, e não é transcrição assíncrona de áudio.
_Avoid_: voice input, chat voice, voice note, speech-to-text, microfone do chat

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

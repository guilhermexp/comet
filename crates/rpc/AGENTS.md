# zeron-rpc — a fronteira tipada

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

`UiRpc` e `ControlRpc`: request/response/stream tipados sobre WebSocket (`tokio-tungstenite`), mais um transporte in-memory e os sockets virtuais do device room (frames `{s,k,to,from}`).

## Ownership

Dona da fronteira UI↔engine. É o que mantém honesto o modo in-process: mesmo protocolo, sem atalho de serialização, rodando sobre um duplex em memória.

## Local Contracts

- `GenerateCommitMessage` é unary tipado, forwardable, deadline de relay 100s; a engine aplica budget próprio de 90s também no IPC. Geração usa a autorização de checkout de Changes e devolve só um rascunho editável.

- SearchGitHistory e ResolveGitAvatars são unary tipados e relay-forwardable no registry. Não recriar listas de forwardable/deadlines no handler.
- GetWorktreeSettings e SetWorktreeSettings são unary tipados e forwardable no
  registry único, com deadline de relay de 30s. Só o host destino valida e
  persiste a preferência device-local; não criar tabela paralela de forwarding.

- `SearchHomeFolders`, `SearchWorkspaceFiles`, `WarmWorkspaceSearch` e `SearchWorkspaceContent` são unary tipados e relay-forwardable; a engine de destino limita e autoriza o índice local. `WatchCheckoutChangeRequest` é stream; `ListChangeRequestPage`, `GetChangeRequestRepository`, `GetChangeRequest`, `GetChangeRequestDiff` e `PostChangeRequestComment` são unary tipados e relay-forwardable. Acesso de PR que usa um checkout exige `authorized_checkout`, inclusive para Worker ligado a projeto local. Não criar tabelas de roteamento/deadline paralelas.

- Campos opcionais introduzidos em `ChangeRequestPage` (`fetchedAt`), `ChangeRequestDetail` (`reviewThreads`, `viewerLogin`) e nos itens de PR devem manter defaults de serde para aceitar engines mais antigas; limites opcionais de busca também preservam payloads legados.

- `GetCheckoutStatus`, `WatchCheckoutStatus` (stream), `StageFiles`, `UnstageFiles`, `DiscardFiles`, `CommitCheckout`, `PushCheckout`, `PullCheckout` e `SyncCheckout` são tipados e relay-forwardable. `forwardable` aqui só autoriza o relay ao device dono do checkout — não autoriza cwd arbitrário. A engine de destino recusa path que não é Chat/Space local nem raiz exata de projeto Worker registrado (`authorized_checkout`) antes de qualquer git. Push/pull/sync usam deadline de 900s. Status é o stream leve; mutações são unary. Não recriar listas de forwardable/deadlines no handler.

- `CreateWorktree` e `DeleteWorktree` continuam relay-forwardable e usam deadline de 1800s: o ciclo comum inclui espera pelo lock, Git, setup com orçamento total de 300s e `pre-*` com orçamento de 300s. A engine avisa após 720s, mas espera a operação bloqueante assentar antes de responder; descartar seu `JoinHandle` deixaria uma mutação em curso depois de um falso timeout. A posse do checkout continua sendo validada no device de destino.

- `WatchPreviews` tem parâmetros/reply tipados no registry; não é forwardable. O catálogo no viewer já reúne serviços locais/remotos. Não marcar local_only: esse flag rejeita `targetDeviceId`, que neste método é filtro de conteúdo.

- `ListWorkspaceDirectory`, `SearchWorkspaceFiles`, `ReadWorkspaceFile`, `WatchWorkspaceFiles`, `CreateWorkspaceEntry`, `DeleteWorkspaceEntry` e `MoveWorkspaceEntry` são tipados e relay-forwardable; só `WatchWorkspaceFiles` é stream. Move usa deadline de 60s. Ownership, jaula de path relativo e limites de filesystem são validados pela engine de destino. Delete é permanente (sem Trash). Desde o sync v0.2.102, `MoveWorkspaceEntry`/`DeleteWorkspaceEntry` usam o modelo do upstream #514 (`operationId`, `expectedCheckoutId`, `expectedSourceRevision`, `expectedKind`, `destinationPath`, `recursive`; reply `WorkspaceMutationOutcome`). Create continua do fork, com reply `WorkspaceEntryMutation`.

- `SpawnChat` é `local_only`: params `parentChatId`, `prompt`, `spaceId?`; reply `chatId`, `spaceId?`, `deviceId`. Não é relay-forwardable.
- `WorkerWaitState` é `local_only`: params `{chatId}`, reply `{pendingMessage}`. Unary tipado, rejeitado no relay de ingress (`!forwardable`). Consulta steers aceitos e não consumidos no ledger da engine para destravar esperas de Worker sem alterar lifecycle ou scheduling do agente.

- **Um protocolo só** para in-process, daemon local e device remoto. Atalho que só existe no modo in-process quebra headless silenciosamente.
- **`src/method.rs` é a lista única de métodos**: nome de fio, `params`, `reply`, `forwardable`, `stream` e `deadline` de um método moram todos numa linha do macro `rpc_methods!`. Adicionar RPC = uma linha no macro + o handler na engine. Nome e valor de cada const de `methods::` são fio — nunca renomear. A engine lê esses atributos por `zeron_rpc::info(method)`; não existe segunda lista para estender.
- Frame do device room é o envelope de relay — método novo que precisa ser dirigível de outro device tem que ser relay-forwardable.
- `FetchToolInput` é unary e relay-forwardable ao device dono; a engine valida ownership do chat antes de ler o journal local.
- Os RPCs `*Voice*V2` roteiam controle Codex realtime ao device dono: capabilities/prepare/negotiate e mutações são unary forwardable; `OwnVoiceV2` é stream forwardable (prepare 65s, negotiate 95s). Tokens de lease, áudio e signaling não viram comando durável. Os cinco RPCs antigos de Live Voice do OMP foram removidos do registry; clientes antigos recebem método desconhecido sem mutação de Chat.
- Handler é async e não bloqueia: enumerar path, ler arquivo e afins vão pra `spawn_blocking`.
- No IPC local, `ProtocolError::HandshakeIncomplete` significa que o peer TCP saiu antes do upgrade e fica em debug; handshakes completos inválidos, `Origin` de browser e demais falhas continuam em warning.
- `LinkCache::new` instala o watcher de credenciais antes de retornar; sign-out não pode perder a primeira versão do `watch` nem manter sockets autenticados em cache.

- `GetTitleSettings` and `SetTitleSettings` are device-forwardable registry methods; title preferences are device-local and not CRDT data.
- `LinkCache` trata `host_offline` do relay como evidência, não blip: a sequência que termina nele estaciona dials daquele device por `offline_cooldown` (5 min). Esse cooldown sobrevive ao broadcast "online" (que todo handshake emite — inclusive o dial rejeitado — e zerava o próprio backoff, gerando o loop 1.5s/3s/6s) e ao refresh de token; só `reset_cooldown` (presença fresca) ou sign-out o limpam. Falhas comuns seguem a curva 5s→60s. O motivo do link-down pode chegar logo depois da falha do probe; o dial espera até 250ms por ele.
- Um sinal de retomada do sistema invalida o socket ativo do `HostRelay`, inclusive durante o dial, e inicia outro sem esperar o lease. `LinkCache` fecha links de peers e remove backoffs comuns nesse sinal, mas conserva cooldowns `host_offline` até presença remota fresca (ou sign-out); o sinal `online` continua separado.

- `PublishVisual` is registered only in `rpc_methods!`, local-only and unary. The engine validates a live turn, bounded Luau and assets before storage/publication. New tools remain behind the existing root Chat MCP grant; no per-run MCP injection is restored.

- `WatchProjectSkillUsage` é stream tipado aditivo e forwardable ao device do Project. O host valida a posse do Space; catálogo recebido apenas seleciona identidades, sem autorizar leitura de arquivos. Frames carregam cobertura explícita; host offline/antigo continua erro, nunca zero confirmado. O proxy usa a confirmação de assinatura existente para preservar `UnknownMethod` do host sem invalidar o link compartilhado.

- `CreateChatOnHost` é unary aditivo/forwardable, payload do op createChat existente e reply Chat canônico. O Mutate local tenta esse seam somente para Chat novo em outro owner com relay configurado, com budget total de 1s para evidência opcional. Confirmação online implica snapshot inicial durável no owner antes da row. Retry de row existente retorna pelo caminho local idempotente, sem chamada remota. Transport/Closed, timeout ou UnknownMethod preservam criação local/offline com owner inalterado e cobertura desconhecida, sem snapshot remoto inventado. Ingress rejeita outras mutações/owner incorreto; erros de validação não são suprimidos. O Mutate genérico não é forwardable.

## Work Guidance

- RPC novo = tipo em `zeron-proto` + linha no `rpc_methods!` de `method.rs` + handler na engine. Os três no mesmo commit, senão a UI compila contra um contrato que não existe.

## Verification

- Comandos: `cargo test -p zeron-rpc`

| Camada / path | Tier exigido | Como rodar |
|---|---|---|
| `src/**` (envelopes, transporte) | unit | `cargo test -p zeron-rpc` |
| `src/method.rs` (registro de métodos, incluindo aposentadoria OMP e preservação de Codex Voice) | unit | `cargo test -p zeron-rpc method::tests::every_method_has_info_and_stream_implies_forwardable -- --exact` |
| `src/server.rs` (classificação do handshake IPC) | unit | `cargo test -p zeron-rpc server::tests::only_an_incomplete_websocket_handshake_is_benign -- --exact` |
| `src/device_room.rs` (retomada do HostRelay e cooldown remoto) | unit | `cargo test -p zeron-rpc --lib device_room::tests::local_wake -- --nocapture` |
| `tests/device_room.rs` | integration — roteamento de socket virtual | `cargo test -p zeron-rpc` |
| `tests/device_room.rs` (revogação de credencial) | integration | `cargo test -p zeron-rpc --test device_room sign_out_closes_cached_peer_links -- --exact` |
| `tests/device_room.rs` (cooldown de `host_offline`) | integration | `cargo test -p zeron-rpc --test device_room host_offline -- --nocapture` |

## Child DOX Index

Sem filhos.

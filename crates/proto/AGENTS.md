# zeron-proto — tipos de fio e derivações compartilhadas

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

O vocabulário que todo mundo fala: `AgentEvent` (incluindo `ToolCallPreview` não durável), `ToolCall`, `RunRequest`, `Model`, `FileToolInputSnapshot` sanitizado, entidades, snapshots device-local de usage (`AgentUsageWindow`/`AgentUsageLine`) e envelopes de RPC (serde, framing ndjson). Além dos tipos, o módulo `view` guarda as **derivações puras** que UI e engine precisam concordar — ordenação, gating de staleness, agrupamento e boot gate.

## Ownership

Crate-base do workspace. Não depende de nenhuma outra crate do repo — se você precisou importar algo daqui pra cima, o tipo está no lugar errado.

## Local Contracts

- `AgentEvent::NativeTitle` transporta metadata local de título da CLI para a engine. É consumido antes de journal/broadcast; somente o título resultante do Chat sincroniza pelo contrato existente.

- `ContextUsage` conserva `tokens`/`contextWindow` numéricos no fio; `tokensReported: false` é aditivo e distingue tokens ausentes de zero (ausência do flag mantém semântica legada). `reported_tokens`/`reported_window` e `merge` são a fonte compartilhada para snapshots parciais. Contexto do Chat é distinto de Managed Provider Usage e continua sincronizado nas rows de Session.
- `RunRequest.mcp` continua campo opcional do request serializado, independente dos controles realtime host-local da harness. `voice` define mídia e signaling efêmeros: áudio, leases e partial transcripts nunca entram em documento; só o `VoiceTranscript` final pode ser reduzido pelo contrato de `zeron-doc`. Remote media v1 usa envelopes estritos, chave de tentativa aleatória e SDP limitado.

- `GeneratedImage` transporta referência raster (`id`, `path`, `name`, `mimeType`). Paths emitidos pelo runtime são entrada privada da engine: antes de journal/sync ela importa para uploads do perfil. O evento não carrega bytes/base64.

- `GenerateCommitMessageRequest { cwd }` e `GeneratedCommitMessage { message }` são tipos aditivos de rascunho; a mensagem preserva título/corpo com quebras de linha, sem mutação Git implícita.

- `CreateWorktreeOutcome` mantém `Worktree` achatado para leitores antigos e adiciona `setupError`/`copyWarning` opcionais: um erro de preparo bloqueia o início por Live Voice e um cache ignorado não copiado chega como aviso ao consumidor.

- `hashline_file_paths` extrai paths únicos de headers canônicos `[PATH#TAG]` (quatro hex maiúsculos), compartilhado pela normalização OMP e recuperação visual de histórico; não interpreta conteúdo de linhas de corpo.

- `attachment_mentions::pair_attachment_mentions` associa chips distintos a paths em ordem estável, um-para-um, compartilhado entre desktop e iOS. Nome visível exato desempata colisões do nome sanitizado pelo upload; nomes duplicados preservam a ordem de staging. É uma derivação local pura e não altera o schema de mensagens.

- GitHistoryPage inclui branchTips com default vazio e comparison opcional. SearchGitHistoryParams e ResolveGitAvatarsParams definem as novas requests aditivas; versões antigas podem continuar lendo páginas sem esses campos.

- `preview.rs` define catálogo de serviços, porta estável e snapshot do browser. Metadados de descoberta não entram no documento Loro; pertencem ao catálogo de previews do device.

- `WorkspaceTarget` ancora listagem, busca, leitura e watcher de Files em Chat/Space no device dono. Diretórios retornam cursor opaco; mudanças carregam sequência e `resyncRequired`. Estes contratos são aditivos e não alteram os links locais absolutos de Chat.

- `InputResolved.answers` é aditivo e opcional: `None` preserva compatibilidade com journals antigos/cancelamentos; `Some` contém as respostas submetidas, identificadas por `question_id`.

- Todo tipo que cruza processo (UI↔engine, engine↔engine via DeviceRoom, engine↔edge) mora aqui.
- Mudar shape de tipo serializado é **breaking cross-device**: dois devices em versões diferentes falam o mesmo fio. Campo novo entra opcional/`#[serde(default)]`; remoção exige change no OpenSpec.
- `view` é puro: sem I/O, sem tokio, sem gpui. É o que permite testar as regras sem subir engine nem janela.
- Tipos de Managed Provider Usage são compatíveis por serde e cruzam apenas engine↔UI; não são persistidos em Loro nem sincronizados pelo edge.
- `HarnessId` também chaveia providers device-local de conta/Usage. Uma variante não torna um runtime executável — só o registry de harness da engine publica descritores runnable. Snapshots do Kimi carregam apenas campos normalizados de conta/quota, nunca material de credencial.

- `Session.last_completed_turn` is optional/defaulted completion evidence. Interrupts, errors and liveness expiry do not advance it; subsequent Working and heartbeat rows retain it.

## Work Guidance

- Lógica de apresentação que a UI e a engine derivam do mesmo estado pertence a `view`, não a `zeron-ui` — duplicar ali é como o comportamento diverge entre headed e headless.

## Verification

- Comandos: `cargo test -p zeron-proto`

| Camada / path | Tier exigido | Como rodar |
| `src/view` (derivações puras) | unit | `cargo test -p zeron-proto` |
| `src/**` (tipos serde) | unit — roundtrip de serialização quando o shape tem regra | `cargo test -p zeron-proto` |
## Child DOX Index

Sem filhos.

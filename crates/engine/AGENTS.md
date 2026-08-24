# comet-engine — o backend

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

Tudo que roda mesmo com a janela fechada: engine de sessões (pub/sub, run journal, recovery, watchdog de stall), doc host + executor de comandos, repos/worktrees, sync de checkout-diff, terminais (`portable-pty`), uploads, contas de agente (troca de credencial), auth (WorkOS via edge), host/peers do device room e identidade.

## Ownership

É o backend inteiro. `comet headless` é esta crate e mais nada. Se um comportamento precisa sobreviver ao fechamento da UI, ele mora aqui — não em `comet-ui`.

## Local Contracts

- **Executor é gated por ownership do chat**: só o device host de um chat executa comandos dele. Marcar como processado vem **antes** de executar, nunca depois.
- Comando é entrada durável no session doc, não chamada direta — send/steer/interrupt/respondInput passam pelo ledger. Envio offline enfileira no doc.
- Só `AgentEvent` durável entra no run journal; `ToolCallPreview` limitado é broadcast/fold sem journal, e o `ToolCall` autoritativo é a única cópia completa do input de arquivo.
- Fechamentos confiáveis de segmentos parent/subagent persistem `duration_ms` medido pela engine; recovery não deriva duração de session rows mutáveis.
- Privacidade de input de arquivo: a projeção no session doc retém só o preview limitado de Write/Edit; o input não sanitizado permanece apenas no run journal local deste device. `FetchToolInput` limita o `ServerFrame` unary completo a 1 MiB e só lê após validar ownership local do chat.
- Lookup histórico lê JSONL do tail em chunks reversos, via `spawn_blocking`, com carry absoluto de 8 MiB + 64 KiB; tail oversized/malformado torna o input indisponível em vez de buscar corpo antigo ou alocar sem limite.
- Doc host mantém um LRU de docs; evicção faz flush do snapshot. **Falha de flush tem que ser reportada** — engolir a falha na evicção perde o snapshot da sessão com o handle já fora do mapa (`doc_host.rs`).
- Terminal: `OpenTerminal` roda o **shell de login interativo**, que volta ao prompt em vez de sair. Sem terminar o shell, `TerminalEvent::Exit` nunca é carimbado e quem espera o fim nunca completa. Payload que funciona: `exec /bin/sh -c '<script quotado>'`.
- Nada de bloqueio em contexto async (`rpc.rs`, `repos.rs` já cobraram esse preço). Trabalho síncrono vai pra `spawn_blocking` com timeout.
- Auth passa pelo edge (WorkOS): a engine não guarda segredo de OAuth próprio.
- Usage de providers combina janelas remotas com totais locais limitados de arquivos Claude/Codex. Esses snapshots são device-local e trafegam só no RPC engine→UI; nunca entram no session doc nem no sync.

## Work Guidance

- Recovery e restart são contrato testado (`restart_resume.rs`): mudança em journal ou watchdog reprova ali antes de reprovar em produção.
- Feature nova de backend normalmente é: RPC em `comet-rpc` + handler aqui + comando no ledger de `comet-doc`. Os três no mesmo commit.

## Verification

- Comandos: `cargo test -p comet-engine` · `scripts/e2e-smoke.sh`

| Camada / path | Tier exigido | Como rodar |
|---|---|---|
| `src/**` (sessões, doc host, repos, terminais) | unit | `cargo test -p comet-engine` |
| `tests/e2e.rs`, `tests/restart_resume.rs`, `tests/workspace_sync.rs` | e2e | `cargo test -p comet-engine` |
| `tests/{auth,device_routing,run_controls_chat_id,m5_*,m5c_*}.rs` | integration | `cargo test -p comet-engine` |
| Superfície multi-device real | e2e manual | `scripts/e2e-smoke.sh` |

## Child DOX Index

Sem filhos.

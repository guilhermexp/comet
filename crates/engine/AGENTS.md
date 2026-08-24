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
- **Nada de buffer graúdo vivo através de um `.await`.** Ele passa a morar *dentro* do future, e build debug reserva esse tamanho na stack de todo frame que constrói o future — inclusive o match de ~100 braços de `EngineRpc::handle`, que em `-O0` dá slot próprio a cada braço. Um chunk de leitura de 64 KiB em `diff_sync::capture_git` custava 128 KiB de frame em `capture_diff` e 832 KiB + 796 KiB nos dois frames do handler: o primeiro checkout-diff abortava o app com `tokio-rt-worker has overflowed its stack` (a worker do gpui_tokio tem os 2 MiB default). Leia direto no `Vec` de saída (`take(cap + 1)` + `read_to_end`); `diff_sync::future_size_tests` guarda o teto.
- Auth passa pelo edge (WorkOS): a engine não guarda segredo de OAuth próprio.
- Usage de providers combina janelas remotas com totais locais limitados de arquivos Claude/Codex. Esses snapshots são device-local e trafegam só no RPC engine→UI; nunca entram no session doc nem no sync.
- **AgentAccounts:** troca de slot de credencial por harness. No macOS o Claude tem DOIS stores — o item do Keychain de login (via a máquina `security` em `agent_accounts.rs`) e `$CLAUDE_CONFIG_DIR/.credentials.json` — e qualquer um pode ser sobra velha, então **não há precedência fixa**: a leitura pega o store com o `claudeAiOauth.expiresAt` mais recente (Keychain vence empate e blob sem data), e a escrita cai em TODO store que já tem login, para que a troca não seja desfeita pela próxima virada de expiry. Fora do macOS (e sempre que a config traz paths explícitos de teste) só o arquivo é usado. Codex: `auth.json`.
- **Modelo de titling de chat:** `titles.rs::cheapest_model` escolhe a FAMÍLIA pela primeira linha do tier pequeno (haiku/mini/nano/flash/small/lite), mas depois pega o membro mais novo dessa família sob o mesmo prefixo de provider. Harness que repassa inventário bruto do runtime em ordem alfabética (OMP) lista modelos retirados primeiro — o primeiro match simples pedia `claude-3-haiku-20240307` e dava 404 em todo título.
- **Kimi managed Usage:** Kimi é identidade de conta/Usage ativa e não trocável, nunca um harness executável nem login próprio do app. Produção lê só `${KIMI_SHARE_DIR:-~/.kimi}/credentials/kimi-code.json`, faz refresh sob o `kimi-code.lock` irmão com seis tentativas não bloqueantes (cinco backoffs de 1.5s), re-leitura pós-lock e substituição atômica `0600`. Requests Bearer vão só para `https://api.kimi.com/coding/v1/usages`; redirects são desabilitados. Janelas bem-sucedidas ficam 60s em cache, invalidado por force refresh, mudança/remoção de credencial ou TTL. Credencial e janelas ficam fora do Loro e do sync do edge; erros expõem só warnings seguros ao provider.
- Context usage é last-known por chat: iniciar um novo processo preserva a última
  medição até o runtime emitir outra; update ausente nunca apaga o indicador.

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

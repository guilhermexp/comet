# zeron-voice-session

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

Orquestra chamadas de voz efêmeras entre cliente, host do Chat e mídia da
plataforma. Reduz eventos de protocolo para estado de apresentação e mantém
signaling fora do ledger durável de comandos.

## Ownership

Esta crate é dona dos traits de transporte e mídia, do lifecycle de chamada e
da projeção comum (`VoiceView`, captions, atividade de speaker e estado do
orb). `zeron-client` fornece o relay autenticado; a engine host executa o
backend; iOS e desktop implementam mídia nativa.

## Local Contracts

- Voz usa RPC efêmero com lease; não usa fila offline, retry implícito, nudge
  nem comando durável.
- A mídia é preparada ainda mutada. `close()` encerra captura local sem esperar
  RPC ou callback pendente.
- Só um turno concluído é persistido uma vez no Chat Transcript canônico; áudio
  bruto e estados intermediários não são sincronizados como mensagens.
- Cancelamento, falha de transporte e rejeição encerram a chamada e liberam a
  mídia.

## Work Guidance

Regras de wire ficam em `zeron-proto::voice`; implementação de backend host
fica em `zeron-engine`/`zeron-harness`; esta crate não conhece UIKit, GPUI ou
CPAL. Teste lifecycle com transports e endpoints falsos.

## Verification

| Camada / path | Tier | Como rodar |
|---|---|---|
| `src/lib.rs` e `src/view.rs` | unit | `cargo test -p zeron-voice-session` |

## Child DOX Index

Sem filhos.

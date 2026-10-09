# zeron-audio

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

Codifica e processa PCM efêmero para Codex Voice no desktop: codec de frames,
resampling e níveis, I/O nativo e cancelamento de eco opcional. Áudio bruto não
vai para o transcript, disco ou sync.

## Ownership

Esta crate é dona do codec e do processamento de áudio local. O protocolo dos
frames pertence a `zeron-proto`; o lifecycle de chamada pertence a
`zeron-voice-session`. I/O nativo e AEC são opt-in por feature.

## Local Contracts

- `native` é a feature padrão e expõe captura/reprodução via CPAL.
- `aec` habilita o processador de eco WebRTC; `worker` requer ambas as features.
- Frames precisam respeitar o limite e o formato declarados em
  `zeron_proto::voice`; entradas inválidas falham antes de alocar buffers sem
  limite.
- O callback de áudio não faz I/O, logging nem trabalho de rede.

## Work Guidance

Mudanças de wire format ficam em `zeron-proto`. Não persistir PCM nem incluir
credenciais no processo/helper de mídia. Deixe dependências e versões no
workspace raiz; `optional = true` fica no manifesto desta crate.

## Verification

| Camada / path | Tier | Como rodar |
|---|---|---|
| `src/{lib,dsp}.rs` | unit | `cargo test -p zeron-audio` |
| `src/aec.rs` | unit | `cargo test -p zeron-audio --no-default-features --features aec` |
| `src/native.rs` e `src/worker.rs` com dispositivo real | none — callbacks de captura/reprodução exigem hardware e sessão de áudio nativa | — |

## Child DOX Index

Sem filhos.

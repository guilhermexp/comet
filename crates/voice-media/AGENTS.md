# zeron-voice-media

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

Fronteira desktop entre o runtime/helper nativo de Codex Voice e o lifecycle
compartilhado de chamada. Valida o helper instalado, limita o ambiente herdado
e transporta apenas signaling, controles e níveis.

## Ownership

Esta crate é dona da descoberta e validação de helper, do framing do protocolo
local e de `DesktopMedia`. `zeron-voice-session` é dono do lifecycle de chamada;
`zeron-audio` é dono do PCM e dos DSPs.

## Local Contracts

- O helper usa o runtime instalado e compatível com a versão local do Codex;
  esta crate não baixa nem instala um segundo runtime.
- O processo começa com ambiente limpo e uma allowlist explícita. Não herdar
  chaves de API nem overrides dinâmicos de plugins GStreamer.
- Frames são limitados, cada operação tem timeout e cancelamento derruba o
  processo filho.
- O protocolo não encaminha PCM para a engine ou para o transcript.

## Work Guidance

Compatibilidade do helper deve ser decidida por metadados adjacentes da
instalação local, nunca por versão remota. Mantenha fixtures offline para
protocolos e caminhos de erro; preserve atribuições em `dist/voice/`.

## Verification

| Camada / path | Tier | Como rodar |
|---|---|---|
| `src/lib.rs` (framing, runtime e helper fixtures) | unit | `cargo test -p zeron-voice-media` |
| Chamada com helper/runtime empacotado e mídia real | none — depende do runtime instalado e de dispositivos de áudio | — |

## Child DOX Index

Sem filhos.

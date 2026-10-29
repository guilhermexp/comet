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

- Host local e cliente desktop usam o mesmo resolvedor de mídia: override
  `ZERON_VOICE_MEDIA_DIR` validado (falha não permite fallback), pacote adjacente
  ao CLI do Chat, depois `packages/standalone/current` e
  `packages/app-server-daemon/current` em `CODEX_HOME` ou `~/.codex`.
  Home vazia/relativa não autoriza busca no checkout. Nunca enumerar releases
  inativas, plugins ou vendor; nunca mudar o executável do Chat nem instalar.
- Pacotes nativos exigem `codex-package.json` com layout 1, versão ≥ 0.159,
  variant `codex`, target local, entrypoint `bin/codex[.exe]` e resourcesDir
  `codex-resources`. O helper físico deve ser arquivo regular, sem redireção
  por symlink dentro do pacote, e executável no Unix. `current` pode ser symlink.
  Metadados locais descrevem a instalação; não substituem uma assinatura de
  distribuição. Startup/protocolo e validação nativa do runtime seguem obrigatórios.
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
| `src/lib.rs` (descoberta, override, identidade/layout e ausência) | unit — homes explícitas em fixtures, sem ambiente global nem fallback para instalações reais | `cargo test -p zeron-voice-media --lib media_resolution` · `cargo test -p zeron-voice-media --lib installed_package` |
| Chamada com helper/runtime empacotado e mídia real | none — depende do runtime instalado e de dispositivos de áudio | — |

## Child DOX Index

Sem filhos.

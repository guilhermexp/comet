# Proposal

## Why

A tela desktop de conversa por voz deixa o Details do Chat selecionado visível e sem interação sob o overlay da chamada. O usuário também precisa desligar a órbita de voz sem alterar o movimento do restante do app e controlar uma chamada ativa ao navegar por outros Chats.

## What Changes

- **V1:** excluir o Details, seu cabeçalho e seus alvos de interação da apresentação da tela de voz, preservando a preferência do painel para o retorno ao Chat.
- **V2:** adicionar em Settings → Voice um controle persistido para mostrar ou ocultar completamente as órbitas do agente de voz. Desligado significa nenhum orb desenhado ou animado, não um frame congelado; microfone, áudio, legendas e controles continuam disponíveis.
- **V3:** apresentar um único widget compacto no canto inferior direito quando houver chamada viva fora da tela de voz, independente do Chat selecionado. Reutilizar mudo, retorno à chamada e encerramento existentes; revelar a ação de encerrar em hover e foco de teclado.
- Substituir o orb de retorno exclusivo do composer pelo widget global, evitando dois controles flutuantes concorrentes.

## Capabilities

### New Capabilities

Nenhuma. A alteração pertence à apresentação desktop da capability existente de voz Codex.

### Modified Capabilities

- `codex-live-voice`: exclusividade visual da tela de voz, preferência de visibilidade do orb e controles compactos que acompanham a chamada durante a navegação desktop.

## Impact

- UI Rust/GPUI: `crates/ui/src/shell.rs`, `shell/voice_stage.rs`, `settings.rs` e o card de Settings → Voice em `dictation/model.rs`.
- Cobertura de layout, navegação, preferências e ciclo de vida nos testes GPUI existentes; aparência por fixture nativa isolada dos componentes reais.
- Sem nova sessão, protocolo RPC, dependência, runtime de áudio ou mudança de autorização/provider. iOS, Android, ditado, controles de saída de áudio e digitação no widget ficam fora do escopo.
- Sem push, instalação, reinício do app ou nova release nesta etapa. Este conjunto de artefatos é para aprovação antes da implementação.

# Proposal

## Why

Arquivos locais como PDF abertos pela árvore ou por links do Chat caem no editor de binários, embora o FilePreview já renderize esses formatos. Áudio ainda não possui classificação nem player.

## What Changes

- Encaminhar HTML, PDF, vídeo, tabelas e áudio locais ao FilePreview pelo ponto comum de abertura, reutilizando abas existentes.
- Preservar imagens/SVG com zoom, Markdown com toggle, código editável e unsupported no Files editor, assim como toda fonte remota.
- Renderizar áudio local como media document WebKit no macOS, sem carregar bytes para memória nem adicionar dependências; preservar fallback textual nas demais plataformas.
- Abrir links absolutos locais externos ao checkout no FilePreview sem alterar o cwd do Chat ou permissões de mutação/RPC.

## Capabilities

### New Capabilities

Nenhuma.

### Modified Capabilities

- `global-file-preview`: roteamento local dos formatos ricos, reprodução de áudio e preview de PDF externo por link do Chat.

## Impact

`crates/markdown/src/file_path.rs`, `crates/ui/src/shell.rs` e `crates/ui/src/file_preview/{loader,view,native_document}.rs`, testes e DOX donos. Sem engine/RPC, leitura binária remota ou novas dependências. QA do app completo depende do isolamento de providers exigido por `scripts/AGENTS.md`; testes e fixtures isoladas não equivalem às capturas pelo file tree/Chat.

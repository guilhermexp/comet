# Source Control presentation implementation plan

**Goal:** Reproduzir a hierarquia visual da segunda captura enviada pelo usuário.
**Architecture:** Ajuste local do renderer existente e das projeções de labels. Reusar RPC, confirmação de discard, Material icons e tooltip; sem dependências.
**Spec:** openspec/specs/source-control/spec.md (apresentação compacta); design D9 em openspec/changes/archive/2026-09-14-source-control-panel/design.md.

- [x] Em `crates/ui/src/details_sidebar/view.rs`, empilhar Commit/Sync em largura total, agrupar controles acima do divisor e ocultar Staged vazio. Manter gates de busy e commit.
- [x] No mesmo arquivo, adicionar disclosure por seção; rows de 28px com basename, diretório secundário, tooltip completo, status à direita e ações por ícone no hover. Reusar `toolbar_button_with_tooltip`, `material_icon` e handlers; cliques das ações interrompem propagação.
- [x] Em `source_control.rs`, ajustar rótulos U/! e Sync Changes/Publish Branch; atualizar testes existentes que verificam essas projeções.
- [x] Rodar rustfmt nos arquivos, `cargo test -p zeron-ui --lib source_control` e `cargo build -p zeron`; conferir janela nativa com paths longos e hover.
- [x] Atualizar `crates/ui/AGENTS.md`, registrar evidências e rodar `openspec validate source-control-panel --strict`.

A aba foi renomeada para **Changes**, preservando a chave `source-control`, por pedido do usuário durante a execução.

# AGENTS.md — third_party

Código externo fixado dentro do repositório e referências locais de pesquisa.

## Purpose

- `unpeel/` — snapshot vendorizado de `unpeel-core`, runtimes, protocolos e
  apps do Unpeel. São arquivos Git comuns deste repositório, consumidos por
  `crates/workers-unpeel` via dependência path.
- `cmux/` — checkout local-only do terminal macOS baseado em Ghostty, usado
  apenas como referência de pesquisa.
- `unpeel-upstream.toml` — proveniência verificável do snapshot vendorizado.

## Ownership

- O projeto mantém o snapshot exato de `unpeel/` e suas patches de
  compatibilidade locais, preservando a licença MIT e atribuição upstream.
- `cmux/` não é propriedade nem dependência do projeto e permanece untracked.

## Local Contracts

- **`unpeel/` não é submódulo.** `.gitmodules` e o gitlink foram removidos no
  commit `216b61e8`; clone, worktree e CI recebem os arquivos diretamente. Não
  executar `git submodule` para este path.
- A base conhecida antes da vendorização era
  `f27e61a6e4fa5e7180f0cd28c129a3b110a89bbc`. O snapshot veio do working tree
  e carregava 16 mudanças locais; o patch original separado não foi retido.
  `unpeel-upstream.toml` registra essa limitação e o tree id reproduzível.
- O conteúdo autoritativo é a árvore Git em `third_party/unpeel`; a metadata
  descreve proveniência e nenhum build tool a lê.
- O workspace continua com `exclude = ["third_party/unpeel"]` porque o snapshot
  contém workspaces próprios. Só `unpeel-core` entra no build do Comet pela
  dependência path explícita do `Cargo.toml` raiz.
- Patch necessária ao Comet é editada no próprio fonte vendorizado, com teste
  downstream e atualização simultânea de `vendored_tree` na metadata.
- `cmux/` não é rastreado, está excluído em `.git/info/exclude`, e nenhum build,
  CI ou documento operacional pode depender da sua presença.
- O fork gpui (`wingleeio/zed`) continua uma dependência Git do Cargo, não um
  diretório desta árvore. Crates GPL do Zed permanecem proibidas.

## Work Guidance

- Para atualizar Unpeel, obter uma base identificável, comparar a árvore nova
  com o snapshot atual, reaplicar/revisar patches locais explicitamente,
  preservar `LICENSE` e atualizar `base_revision` + `vendored_tree` no mesmo
  commit.
- Não importar `.git`, worktree state, credenciais, caches ou artefatos que não
  sejam dependências binárias intencionais já documentadas.
- Mudanças em `third_party/unpeel` precisam provar o consumidor real com
  `cargo test -p zeron-workers-unpeel`.

## Verification

### Test Coverage Matrix

| Camada / path | Tier exigido | Como rodar |
|---|---|---|
| `third_party/unpeel` | none — upstream vendorizado; contrato do adaptador é downstream | `cargo test -p zeron-workers-unpeel` |
| `third_party/cmux` | none — referência local untracked | — |

## Child DOX Index

None — flat domain.

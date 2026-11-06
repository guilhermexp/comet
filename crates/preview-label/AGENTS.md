# zeron-preview-label

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

Classificação pura de comandos e labels de processos/previews.

## Ownership

API pura compartilhada, independente da UI.

## Local Contracts

Não consulta processo, rede ou credenciais. Catálogo e autorização de visibilidade pertencem a preview/engine; labels não concedem acesso.

## Work Guidance

Manter tipos e dependências no workspace; mudanças de comportamento seguem OpenSpec.

## Verification

| Camada / path | Tier | Comando |
|---|---|---|
| `src/**` | unit | `cargo test -p zeron-preview-label --lib` |

## Child DOX Index

Sem filhos.

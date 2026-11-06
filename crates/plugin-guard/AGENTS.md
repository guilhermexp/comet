# zeron-plugin-guard

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

Validação compartilhada de comandos, paths e rede de plugins.

## Ownership

API pura compartilhada, independente da UI.

## Local Contracts

Esta crate não ativa plugins nem concede grants MCP. Decisões de runtime e consentimento pertencem à engine; preserve limites, jails e testes negativos.

## Work Guidance

Manter tipos e dependências no workspace; mudanças de comportamento seguem OpenSpec.

## Verification

| Camada / path | Tier | Comando |
|---|---|---|
| `src/**` | unit | `cargo test -p zeron-plugin-guard --lib` |

## Child DOX Index

Sem filhos.

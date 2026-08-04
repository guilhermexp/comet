# comet-harness — adaptadores de coding agent

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

Esconde *qual* agente está rodando atrás de uma interface só. O trait `Harness` mais três implementações: **claude-code** (subprocesso falando stream-json), **codex** (app-server JSON-RPC) e **mock** (determinístico, usado pelo demo e pelos testes). Junto: mailbox de steering, `requestInput` e os catálogos de modelo/reasoning/opções.

## Ownership

Dona de tudo que é específico de vendor. A engine acima só conhece o trait — se um `if provider == "claude"` apareceu fora daqui, a abstração vazou.

## Local Contracts

- Catálogo de modelo/opção é dado do harness, não constante espalhada na UI.
- Steering é mailbox: comando chega enquanto o run está vivo e é entregue no ponto de corte; sem run vivo, vira o próximo turno.
- Resolução de ambiente de shell (`shell_env_resolution.rs`) existe porque o agente herda o ambiente errado quando invocado fora de um shell de login — mudanças aqui quebram o spawn em máquinas reais sem quebrar teste.
- Fixtures de transcript vivem em `tests/fixtures/` — é o que fixa o parse de stream-json contra mudança de formato do vendor.

## Work Guidance

- Vendor mudou o formato de saída? A correção é uma fixture nova + o parse, nunca um `if` no consumidor.
- Adicionar harness novo = implementar o trait + catálogo + fixture de transcript. Nada mais deve precisar mudar.

## Verification

- Comandos: `cargo test -p comet-harness`

| Camada / path | Tier exigido | Como rodar |
|---|---|---|
| `src/**` (parse, mailbox, catálogos) | unit | `cargo test -p comet-harness` |
| `tests/{claude,codex}.rs` | integration — contra fixtures | `cargo test -p comet-harness` |
| `tests/shell_env_resolution.rs` | integration | `cargo test -p comet-harness` |

## Child DOX Index

Sem filhos.

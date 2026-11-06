# crates/launch

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

Launch/background engine do novo produto upstream, conservado como fonte.

## Ownership

Fonte upstream dormente; o runtime ativo é documentado no pai.

## Local Contracts

D8: crate excluída do workspace do fork. Engine in-process, daemon, publicação e RestartGate do fork prevalecem. Não ligar engine gerenciada ou beta sem change separado.

## Work Guidance

Preservar licença/atribuição e a intenção do fork nos syncs. Não usar exclusão do workspace como prova de compilação deste produto alternativo.

## Verification

| Camada / path | Tier | Motivo |
|---|---|---|
| Fonte dormente | none | Não compilada nem ativada nesta integração; revisão e testes próprios são necessários antes de habilitar |

## Child DOX Index

Sem filhos.

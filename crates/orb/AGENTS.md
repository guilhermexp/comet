# zeron-orb

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

Biblioteca de geometria e animação do orb de pensamento/voz, sem renderer. A
mesma frame alimenta a pintura GPUI no desktop e Core Graphics no mobile.

## Ownership

Esta crate é dona dos estados, presets, relógio de animação e primitives
geométricas (`Frame`, linhas e pontos). As surfaces escolhem cores e pintam;
elas não duplicam a geometria.

## Local Contracts

- A saída é geometria em coordenadas lógicas, limitada pelo tamanho e perfil
  resolvido.
- Áudio só modula resposta temporal; não muda os limites nem a topologia da
  frame.
- Respeite reduced motion sem avançar animações contínuas.

## Work Guidance

Mantenha o núcleo independente de UI, renderer e plataforma. Mudanças de perfil
devem preservar limites finitos de geometria e as mesmas unidades consumidas
por `zeron-ui` e `zeron-mobile`.

## Verification

| Camada / path | Tier | Como rodar |
|---|---|---|
| `src/{animator,motion,presets,engine}/**` | unit | `cargo test -p zeron-orb` |
| Pintura GPUI/Core Graphics | none — exige aceitação visual por surface | — |

## Child DOX Index

Sem filhos.

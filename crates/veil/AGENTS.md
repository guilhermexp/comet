# zeron-veil

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

Modela fades de texto streaming sem renderer: progresso por chunk, opacidade de
rows e transição de captions. UI desktop e caption mobile consomem as mesmas
faixas.

## Ownership

Esta crate é dona da matemática e do estado de veil/caption. `zeron-ui` aplica
as opacidades aos runs GPUI; `zeron-mobile` converte offsets de caption para
UTF-16 para o renderer Swift.

## Local Contracts

- O veil altera opacidade, nunca quebra ou mede texto.
- Faixas são intervalos em bytes UTF-8 neste núcleo; a fronteira mobile converte
  para offsets UTF-16 antes de atravessar UniFFI.
- Captions limitam o tamanho visível e mantêm a cauda recente da fala.

## Work Guidance

Mantenha a lógica pura e independente de relógios/plataformas; passe `Instant`
ao estado de caption. Integração visual pertence às surfaces.

## Verification

| Camada / path | Tier | Como rodar |
|---|---|---|
| `src/lib.rs` e `src/caption.rs` | unit | `cargo test -p zeron-veil` |
| Aplicação visual das faixas no transcript | none — aceitação visual por surface | — |

## Child DOX Index

Sem filhos.

# zeron-theme — domínio de temas

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

Modelo source-neutral de temas, registro built-in, biblioteca device-local de temas customizados e importador de temas VS Code. A UI recebe variantes já resolvidas; componentes não interpretam ids de workbench nem scopes TextMate.

## Ownership

É dona de famílias, variantes, cores, acentos, preferência de superfície e proveniência de importação. Persistência e seleção de aparência continuam na `zeron-ui`; renderização nunca mora aqui.

## Local Contracts

- `ThemeVariant` entregue ao runtime é completo e independente do formato-fonte.
- A preferência de superfície é política device-local separada do tema e do accent.
- Importação linked preserva a origem e pode ser recarregada; copy instala snapshot independente.
- Tema inválido ou incompleto falha fechado e não substitui o último registro válido.
- `AccentPreset` é o único enum de accent do app (a ui o reexporta como `AccentColor`). Os aliases serde `violet`/`indigo`/`red`/`purple` → `Zeron` e `teal` → `Cyan` são compatibilidade de dado em disco: sem eles um `ui-settings.json` antigo falha a leitura na abertura do app. Renomear variante exige alias novo, nunca troca seca.
- Built-ins curados podem declarar overrides de papel (`hover`, `active`, `border`, `border_strong`, `input`, `cursor`, `diff_hunk`, `terminal.selection`) e parâmetros de frost (`frost_alpha`, `frost_blur_radius`, `flat_shell`) em `ThemeVariant`. Campos novos em `ThemeVariant` levam `#[serde(default, skip_serializing_if = …)]`: `asset_hash` é SHA-256 da variante serializada, e um campo que serializa por default move o hash dos 31 variantes.
- `terminal_background` pode carregar alpha. O contraste de `terminal.foreground` endurece contra `flatten(terminal_background, background)`, nunca contra o seed translúcido.

## Work Guidance

- Normalize formatos externos em `vscode.rs`; não espalhe chaves de VS Code pelo modelo ou pela UI.
- Built-ins entram em `builtins.rs`; ciclo de vida da biblioteca customizada entra em `library.rs`.
- Preserve licença e atribuição dos temas importados ou incorporados.

## Verification

| Camada / path | Tier exigido | Como rodar |
|---|---|---|
| `src/lib.rs`, `src/builtins.rs` | unit | `cargo test -p zeron-theme` |
| `src/builtins.rs` (`monocode-dark` papéis de interação) | unit | `cargo test -p zeron-theme --lib monocode_fidelity_state_roles_are_neutral` |
| `src/builtins.rs` (variantes sem override) | unit | `cargo test -p zeron-theme --lib monocode_fidelity_non_declaring_variants_are_unchanged` |
| `src/builtins.rs` (`terminal_background` com alpha) | unit | `cargo test -p zeron-theme --lib monocode_fidelity_terminal_background_keeps_alpha` |
| `src/library.rs` | unit + filesystem temporário | `cargo test -p zeron-theme library` |
| `src/vscode.rs` | unit | `cargo test -p zeron-theme vscode` |

## Child DOX Index

Sem filhos.

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

## Work Guidance

- Normalize formatos externos em `vscode.rs`; não espalhe chaves de VS Code pelo modelo ou pela UI.
- Built-ins entram em `builtins.rs`; ciclo de vida da biblioteca customizada entra em `library.rs`.
- Preserve licença e atribuição dos temas importados ou incorporados.

## Verification

| Camada / path | Tier exigido | Como rodar |
|---|---|---|
| `src/lib.rs`, `src/builtins.rs` | unit | `cargo test -p zeron-theme` |
| `src/library.rs` | unit + filesystem temporário | `cargo test -p zeron-theme library` |
| `src/vscode.rs` | unit | `cargo test -p zeron-theme vscode` |

## Child DOX Index

Sem filhos.

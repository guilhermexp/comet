## Why

O `@` do composer só alcança arquivos da checkout atual (`SearchFiles`). Projetos — a lista que Settings → Projects mostra por inteiro — não são mencionáveis: para pedir "resume a situação desse projeto" o usuário digita o path absoluto à mão, sem chip e sem garantia de que a pasta é uma que o app conhece.

## What Changes

- O popup do `@` ganha um nível raiz: uma entrada **Projects** no topo, hairline, e abaixo os resultados de arquivo de sempre.
- Abrir Projects troca o popup pela lista completa de projetos (nome + path), filtrável pelo que se continua digitando depois do `@`. Escape volta ao nível raiz; do raiz, fecha o popup.
- A lista vem do **ledger de projetos** (`project_ledger`) — a mesma fonte de Settings → Projects, lida direto do `app-state.json`, sem depender do daemon de Workers.
- Escolher um projeto insere um chip no prompt, como já acontece com arquivos: o chip mostra o nome, o prompt carrega o path absoluto.
- A menção é puramente textual: NÃO troca o cwd, o device nem o projeto-alvo do chat.

## Capabilities

### New Capabilities

- `composer-project-mentions`: mencionar um projeto conhecido pelo `@` do composer, como referência textual com chip.

## Impact

- `crates/ui/src/composer.rs`: scheme `zeron-project:`, menu de dois níveis, leitura do ledger.
- DOX: `crates/ui/AGENTS.md` (contrato do `@`).
- Sem mudança em engine, RPC, CRDT ou edge.

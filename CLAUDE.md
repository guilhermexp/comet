# CLAUDE.md — comet

Este arquivo é fino de propósito. O contrato do repo está em [`AGENTS.md`](AGENTS.md) (raiz) e nos `AGENTS.md` por subárvore.

## Antes de editar

1. Ler [`AGENTS.md`](AGENTS.md) e caminhar pela cadeia DOX até cada path que vai tocar (o Child DOX Index de cada doc aponta o próximo).
2. Mudança de **comportamento de capability** → abrir change no OpenSpec (`openspec/`, convenções em `openspec/project.md`) antes de codar.
3. Fazer a edição **mínima** no lugar certo. No closeout, DOX pass: atualizar o AGENTS.md dono + pais afetados.

## Comandos

`cargo build -p comet` · `cargo test` · `cargo test -p <crate>` · `cargo fmt --all` · `scripts/dev-demo.sh` · `scripts/e2e-smoke.sh` · `npm -C edge run test|typecheck`

## Lessons Learned

- **`cargo fmt --all` antes de merge do upstream.** O upstream lança várias versões por semana; sem fmt do nosso lado, o merge conflita em ruído de formatação.
- **Suite verde não é evidência de UI correta.** Não existe harness de render gpui — mudança visual se valida rodando `scripts/dev-demo.sh` e olhando.
- **`crates/tui` deletado ≠ painel de terminal.** O que saiu foi o viewport ratatui do `comet tui`. O painel dentro do app é `crates/ui/src/terminal/` e continua vivo.
- **`OpenTerminal` roda shell de login interativo**, que volta ao prompt em vez de sair — sem terminar o shell, `TerminalEvent::Exit` nunca chega e quem espera o fim trava. O payload que funciona é `exec /bin/sh -c '<script quotado>'`.
- **`dist/` da raiz é asset-fonte de packaging**, não build. Só `edge/dist/` é gerado. Apagar a da raiz quebra release sem quebrar build.
- **`gh` resolve pro upstream por default.** Sempre `-R guilhermexp/comet`, senão você está lendo CI e releases do repo alheio.
- **Este repo tem sessão de agente ativa com frequência**: branch e working tree mudam embaixo do pé. Re-checar `git status -sb` antes de editar; snapshot de 10 minutos atrás já esteve obsoleto.
- **Token que some do theme upstream se remapeia**, não se recria — foi o caso de `white_alpha` → `ink`/`hairline`, senão o light mode ganha wash branco.

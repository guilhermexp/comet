# comet-ui — o app gpui

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

O viewport: shell (sidebar de spaces + abas), transcript, composer, painel de terminal, pane de diff, settings, markdown próprio e o kit de animação. Renderiza estado do mirror direto, com notificação por entrada alterada.

## Ownership

Dona de tudo que é pixel. **Não** é dona de comportamento que precisa sobreviver à janela fechada — isso é `comet-engine`. Regra derivada compartilhada com a engine mora em `comet-proto::view`, não aqui.

## Local Contracts

- gpui vem do fork `wingleeio/zed` pinado por rev no `Cargo.toml` raiz. **Não usamos as crates GPL do Zed** (`markdown`, `ui`, `theme`, `editor`) — markdown, componentes e tema são nossos. Puxar uma delas é problema de licença, não de gosto.
- Tema (`theme.rs`) suporta light e dark. Token que sumiu do upstream (ex: `white_alpha`) se remapeia pro equivalente que o upstream aplicou no código gêmeo — não se recria localmente, senão o light mode fica com wash.
- Animação é camada de **paint**: `with_animation` sobre opacidade nunca altera layout. `prefers-reduced-motion` é honrado.
- Altura de linha em code block = linhas × line-height, independente do highlight; o highlight roda time-sliced em background e entra como run de texto (paint-only).
- Transcript é por **bloco**, não por mensagem: id estável `msgId#blockId`, turno vivo não splitado, re-split na persistência. Eco otimista compartilha o id cunhado no cliente pra persistência não piscar.
- `crates/ui/src/terminal/` é o **painel de terminal dentro do app**. Não confundir com o `crates/tui` deletado (viewport ratatui do upstream, removido).
- Presença de terminal se reconcilia por `reconcile_terminal_presence` + evento. Dispatchar `ToggleTerminal` no fechamento da última aba (como o upstream faz) dispara em dobro aqui.

## Work Guidance

- "Não atualizou na tela" começa em `comet-doc` (mirror), não aqui.
- Não há harness de render: mudança visual se valida rodando `scripts/dev-demo.sh` e olhando. Screenshot antes de dizer pronto.

## Verification

- Comandos: `cargo test -p comet-ui` · `scripts/dev-demo.sh`

| Camada / path | Tier exigido | Como rodar |
|---|---|---|
| `src/**` (estado, derivações, parse de markdown) | unit | `cargo test -p comet-ui` |
| `src/markdown/**` | unit — parse e mend têm cobertura própria | `cargo test -p comet-ui` |
| `src/{shell,settings,terminal}/**` (render gpui) | none — sem harness de render; validação é visual | `scripts/dev-demo.sh` |

## Child DOX Index

Subárvores sem doc próprio (ainda não têm regra local além da desta pasta): `shell/` (spaces, tabs), `terminal/` (emulator, panel, view), `settings/`, `markdown/`. Adensar aqui quando alguma ganhar contrato próprio.

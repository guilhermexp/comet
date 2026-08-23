# comet — instruções do repo

Fork de [zeronsh/comet](https://github.com/zeronsh/comet) (MIT). Controlador multi-device de coding agents (Claude Code / Codex): cada máquina roda uma **engine** (daemon Rust) e as sessões sincronizam por **CRDT Loro** através de **Durable Objects** na Cloudflare. Rewrite nativo em Rust + gpui do comet original em TS/Electron.

Detalhe canônico de arquitetura: `ARCHITECTURE.md`. Paridade contra o app original: `docs/PARITY.md`. Inventário funcional: `FUNCTIONAL-BASELINE.html`.

Terminologia canônica de produto vive em [`CONTEXT.md`](CONTEXT.md). Leia antes de mexer na linguagem de provider/conta: **Managed Provider Usage** é quota de assinatura device-local, nunca billing de API nem usage sincronizado.

## Stack

- **Rust workspace** (edition 2024, `resolver = "2"`) — `crates/{proto,doc,sync,harness,engine,rpc,mcp,update,ui}` + `apps/comet`.
- **UI = gpui**, pinado num fork do Zed (`wingleeio/zed`, rev fixado em `Cargo.toml`). Não usamos as crates GPL do Zed (`markdown`, `ui`, `theme`, `editor`) — markdown, componentes e tema são nossos.
- **Sync = loro 1.13 + loro-protocol 0.3** (twin Rust do pacote npm que a edge fala).
- **Edge = TypeScript** (`edge/`) — Worker + SessionRoom DO (por chat) + DeviceRoom DO (por device) + R2 + auth WorkOS. Sem Postgres, sem Hono server, sem WebRTC.
- **apps/ios** — cliente iOS (projeto Xcode), fora do workspace Cargo.
- Binário único `comet`: headed (gpui) ou `comet headless` (só engine).

## Comandos

| Ação | Comando |
|---|---|
| Build do app | `cargo build -p comet` |
| Suite completa | `cargo test` |
| Testes de uma crate | `cargo test -p comet-ui` |
| Formatação (obrigatória antes de merge do upstream) | `cargo fmt --all` |
| Demo local offline (harness mock, seeded) | `scripts/dev-demo.sh` (`--slow` pra ver streaming) |
| Smoke e2e | `scripts/e2e-smoke.sh` |
| Edge | `npm -C edge run dev\|test\|typecheck\|deploy` |
| Packaging | `scripts/package-linux.sh` · `scripts/package-macos.sh` |

## Remotes e publicação

- `origin` = `guilhermexp/comet` (nosso fork) · `upstream` = `zeronsh/comet` (terceiro, MIT).
- **Nunca pushar para o upstream.** Qualquer push vai pro fork.
- `gh` resolve pro upstream por default: **sempre passar `-R guilhermexp/comet`** em `gh run list`, `gh release view`, etc.
- `.github/workflows/{deploy,release}.yml` são herdados do upstream: `deploy` publica o Worker Cloudflare em push na `main` que toque `edge/`; `release` dispara em tag `v*` e publica no R2. **Não pushar tag `v*` no fork sem entender o efeito.**
- Push passa pelo gate `no-mistakes` (`git push no-mistakes <branch>`); status sempre do checkout principal, nunca de worktree.

## Gotchas duráveis

- **Sync com o upstream é frequente** (várias versões por semana). A receita que faz o merge passar é `cargo fmt --all` do nosso lado **antes** do merge. Conflitos se resolvem a favor do fork, e o motivo de cada um vai no corpo do commit de merge.
- `crates/tui` / `apps/tui` foram **deletados** (upstream removeu o viewport ratatui). Isso **não** é o painel de terminal dentro do app — esse vive em `crates/ui/src/terminal/` e está intacto.
- `dist/` guarda **assets-fonte** de packaging (ícone, `.desktop`, `Info.plist`), consumidos por `scripts/package-*.sh` e pelo workflow de release. Só `edge/dist/` é gerado/ignorado — não apagar a `dist/` da raiz.
- Build do gpui é caro; `[profile.dev]` já usa `opt-level = 2` pras deps. Primeira build leva minutos.
- Bump do rev do gpui exige rebase da branch `comet/line-wrap-closing-punctuation` no fork do Zed.
- Este é um repo de terceiro sob MIT. Preservar licença e atribuição.

## Onde mudar o quê

- Mudança de **comportamento de capability** → abrir change no OpenSpec (`openspec/`) antes de editar. Convenções de autoria em `openspec/project.md`.
- Mudança **local de código** → ler a cadeia de `AGENTS.md` até o alvo (Child DOX Index abaixo) e fazer a edição mínima ali.

## DOX Framework

- Este repo usa DOX: AGENTS.md hierárquico, 1 por domínio/pasta durável. Cada AGENTS.md é contrato vinculante da sua subárvore.
- DOX é o eixo ESPAÇO (onde o código mora, como editar aqui). O eixo TEMPO (o que mudar, capability nova/breaking) é OpenSpec — antes de mudar comportamento, ver `openspec/` e seguir `openspec/project.md`. DOX não reescreve as rules do OpenSpec.

### Read Before Editing
1. Ler este AGENTS.md (raiz) + identificar cada path que vai tocar.
2. Caminhar da raiz até cada alvo, lendo todo AGENTS.md no caminho (Child DOX Index aponta o próximo).
3. Doc mais próximo controla detalhe local; pais controlam regra repo-wide. Em conflito, o mais próximo vence no detalhe — nenhum filho enfraquece DOX nem OpenSpec.
4. Não confiar em memória: re-ler a cadeia DOX na sessão atual antes de editar. Fazer a edição MÍNIMA no lugar certo (não duplicar função, não criar helper novo se dá pra estender).

### Update After Editing (DOX pass — obrigatório no closeout)
- Toda mudança significativa: atualizar o AGENTS.md dono mais próximo + pais afetados + Child DOX Index. Remover texto stale na hora.
- Atualizar quando muda: propósito, escopo, ownership, estrutura durável, contratos, workflow, inputs/outputs/permissões/constraints, preferência durável do usuário, ou criação/move/rename de AGENTS.md.
- Mudança de comportamento de capability → também rodar o ciclo OpenSpec (validate → archive).

### Child Doc Shape
Criar AGENTS.md filho quando a pasta vira boundary durável com regra própria — boundary aqui é seam (o comportamento pode ser substituído sem editar o consumidor), não tamanho nem contagem de arquivos. Seções (vazias se não há padrão ainda):
- **Purpose** · **Ownership** · **Local Contracts** · **Work Guidance** · **Verification** · **Child DOX Index**

A seção **Verification** carrega a **Test Coverage Matrix** local (`camada/path → tier`: `unit` | `e2e` | `integration` | `none`) — fonte de verdade do tier que cada `Scenario` do OpenSpec carimba (`Test:`). Semeada das convenções que **já existem** na subárvore, incrementalmente por change. `none` é decisão com motivo, não omissão.

### Closeout
1. Re-checar paths mudados contra a cadeia DOX.
2. Atualizar docs donos + pais/filhos afetados + cada Child DOX Index.
3. Remover texto stale/contraditório.
4. Rodar verificação existente (testes/lint) + ciclo OpenSpec se mudou comportamento.

## Child DOX Index

| Domínio | Doc | O que mora ali |
|---|---|---|
| Workspace Rust | [`crates/AGENTS.md`](crates/AGENTS.md) | As 9 crates da lib: wire types, docs CRDT, sync, harnesses, engine, RPC, MCP, updater, UI |
| Binário e clientes | [`apps/AGENTS.md`](apps/AGENTS.md) | `apps/comet` (CLI headed/headless) e `apps/ios` |
| Edge Cloudflare | [`edge/AGENTS.md`](edge/AGENTS.md) | Worker, SessionRoom/DeviceRoom DOs, R2, auth WorkOS |
| Scripts | [`scripts/AGENTS.md`](scripts/AGENTS.md) | Dev demo, smoke e2e, packaging Linux/macOS |

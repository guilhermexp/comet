# Sync com o upstream (`zeronsh/zeron`)

Registro vivo de **como** sincronizamos com o upstream e **o que decidimos** a cada vez. As decisões detalhadas de cada sync ficam na OpenSpec dele (tabela no fim). Este arquivo guarda o que precisa sobreviver entre syncs: a receita, o que recusamos e por quê, e as armadilhas que já custaram tempo.

Atualizar este arquivo é parte do closeout de todo sync.

## Estado atual

| | |
|---|---|
| Último commit do upstream mergeado | `433aa148` (v0.2.94 + follow-ups de iOS, 2026-09-27) |
| Merge | `1065d252` em `sync/upstream-v0.2.94` |
| OpenSpec | `openspec/changes/sync-upstream-v0-2-94/` |
| Próximo sync começa de | `433aa148`, merge-base natural, sem graft |

## Receita

1. **Buscar o upstream** sem remote fixo e sem tags locais `v*`, que o `release.yml` publicaria:
   ```sh
   git fetch https://github.com/zeronsh/zeron.git '+main:refs/upstream/zeron-main' --no-tags
   git log --oneline --no-merges $(git merge-base origin/main refs/upstream/zeron-main)..refs/upstream/zeron-main
   ```
2. **Medir antes de mexer.**
   - `git merge-tree --write-tree --name-only origin/main refs/upstream/zeron-main` lista os arquivos que vão conflitar.
   - Auditar por área (engine/sync, contas/settings, UI, Explorer/MCP, iOS, infra), classificando cada commit como TAKE, ADAPT ou SKIP e dizendo onde encosta no fork.
3. **Worktree e branch próprias**, a partir do `origin/main`. O checkout principal costuma estar ocupado por outra sessão.
   ```sh
   git worktree add -b sync/upstream-vX.Y.Z ../comet-sync-vX.Y.Z origin/main
   ```
4. **Abrir a OpenSpec** `sync-upstream-vX-Y-Z` com as decisões antes de resolver os conflitos.
5. **`cargo fmt --all` do nosso lado antes do merge.** Sem isso, o merge conflita em ruído de formatação.
6. **Merge de verdade**, sem cherry-pick:
   ```sh
   git merge --no-commit --no-ff refs/upstream/zeron-main
   ```
   - O que recusamos é resolvido a favor do nosso lado **dentro** do merge. Assim o próximo sync não vê de novo o que já foi decidido.
   - Se a ancestralidade quebrar (como na reassinatura de set/2026), use o graft temporário descrito em `sync-upstream-v0-2-83` D1.
7. **Resolver os conflitos por arquivo.** Dá para paralelizar entre agentes, desde que cada um fique com arquivos disjuntos, não rode `git checkout`/`stash`/`commit` nem compile. A compilação e a correção do que ela quebrar ficam com quem coordena.
8. **Compilar e testar:**
   - `cargo check --workspace --all-targets`
   - `cargo test --workspace --no-fail-fast`
   - `npm -C edge run typecheck && npm -C edge run test`
9. **Comparar linha a linha.** Para cada commit do upstream que foi aceito, procurar as linhas que ele acrescentou, que ainda existem no upstream final e que faltam no nosso código. Isso pega o que se perdeu sem querer na resolução.
10. **DOX:** atualizar os `AGENTS.md` donos, a OpenSpec e **este arquivo**.
11. **Publicação:** push só no `origin` (`guilhermexp/comet`), só quando o dono pedir, e com `gh -R guilhermexp/comet`. Nunca pushar para o upstream. Nunca pushar tag `v*`.

## Contratos do fork que sempre vencem

- **Workers e harnesses:** Workers (`crates/workers-unpeel`, `WorkflowTask`, aba Workers), os harnesses OMP e Kimi, Live Voice, trajectory.
- **Steering:** Enter com run ativo faz steer. A fila do upstream coexiste, mas não substitui (v0.2.83 D3).
- **Modelo de grant MCP:**
  - `comet-workers` vai para quem pode lançar Workers.
  - `comet-sessions` e `zeron` (o `zeron mcp`) vão **só** para o orquestrador raiz com IPC ativo.
  - `RunRequest.mcp` fica sempre vazio: não entra a injeção do upstream em todo run.
- **Contas e uso:** o painel Usage da Details, os medidores do fork (Grok gerenciado, Cursor, Kimi, Antigravity) e `usage_lines`.
- **Visual:** Settings com a navegação lateral do fork, Changes dentro do Files, transcript compacto, sem anel de contexto e sem terminal de rodapé.
- **Painel direito:** só conta como aberto se tiver aba viva.
- **Publicação e update:**
  - versão 0.2.18, `release.yml` do fork, zui vendorizado em `third_party/zui`, com os pins de zui do upstream ignorados;
  - o updater não usa o feed `zeron.sh`: vale o `has_release_feed`, e um feed próprio exige `ZERON_RELEASES_URL`.

## Recusado: continua a versão do fork

| O que | Upstream | Sync | Por quê |
|---|---|---|---|
| Fold compacto de tools (`TOOL_FOLD`), tools em árvore, ícones Symbols | #250/#253/#254/#354 | v0.2.83 | O fork tem apresentação própria |
| Anel de contexto (`context_usage.rs`) | — | v0.2.83 | O fork usa o indicador de contexto do rodapé do composer |
| Live preview de markdown no composer, seletor de modelo dentro do pill | — | v0.2.83 | Apresentação do fork |
| Terminal no rodapé | — | v0.2.83 | O terminal vive no painel direito |
| Navegação por passos do New project | #403 | v0.2.83 | Apresentação do fork |
| Faixa de update | — | v0.2.83 | Fork sem feed próprio |
| Sempre enfileirar em vez de steer | #284 | v0.2.83 | Steering é contrato do fork |
| Redesign dos Settings (modal glass → página, switches, Providers, Devices refeito) | #449 (UI) | v0.2.94 | Visual do fork. **A parte de engine entrou**, e o rodapé da sidebar também (pílula avatar+nome, botão Settings que alterna, menu para cima) |
| Anel de uso do plano ao lado do anel de contexto | #547 | v0.2.94 | Depende do anel removido e duplica o Usage |
| Paleta do New project igual à do Cmd+K, breadcrumbs dobrados | #549 | v0.2.94 | Bate no `spaces.rs` do fork |
| Badge de PR sem `#` | f8f9c97f | v0.2.94 | Estilo |
| OpenCode aprovando todas as permissões | #533 | v0.2.94 | Regressão de segurança |
| Checar update sem sync de workspace; link "unmanaged" para as releases do zeron | #535 | v0.2.94 | Faria o comet atualizar para o Zeron |
| Rodapé Subagents/Chats do Explorer (`files/sections.rs`) | #498 | v0.2.94 | Duplica Workers › Subagents |
| Botão "descartar árvore inteira" no diff | #81 (UI) | v0.2.94 | O descarte fica no Changes do Files. **As proteções entraram** |
| Seção General nos Settings | #449/#541 | v0.2.94 | O Shortcuts do fork já cobre; `settings/general` abre Shortcuts |
| Escape em camadas nos Settings | #541 | v0.2.94 | O Settings do fork não fecha com Escape |
| Windows: build ARM64, arrastar abas (sobe zui), cwd do ConPTY, lock de identidade | #545/#536/#528/#527 | v0.2.94 | Fork só roda em macOS |

## Aceito com adaptação

| O que | Upstream | Sync | Como ficou |
|---|---|---|---|
| Orçamento de sockets/dials/HTTP, `sync_jobs`, ACK de nudge | #544 | v0.2.94 | Em cima do outbox durável do fork. O `MAX_CONCURRENT_DIALS` do fork **saiu**: travava joins |
| `FocusChat` e rotação justa por foco | #550 | v0.2.94 | Registrado no `crates/rpc/src/method.rs` (não forwardable) |
| Side chats (fork da conversa) | #498/#568/#567 | v0.2.94 | Aba do painel direito, com o divisor `MessagePart::Fork` |
| Tools de chat do `zeron mcp` (+ `create_chats`/`send_messages`) | #498 | v0.2.94 | Só no orquestrador raiz, via `workers_mcp`. No OMP, bridge multi-tool |
| Contas | #542/#546/#449 | v0.2.94 | Entraram Pi, Devin, OpenCode e Hermes; o Grok do upstream ficou desligado. Entraram o re-login da conta ativa e o cache de uso |
| Proteções do descarte | #81 | v0.2.94 | No `DiscardFiles` do fork: agente ativo, submódulo, `git clean` que preserva ignorados |
| Terminal no canvas de nova chat | #474 | v0.2.94 | Mantida a regra `chatId` xor `cwd` do fork, com exceção só para `space-canvas:` |
| Última seção dos Settings | #541 | v0.2.94 | Gravada por `apply_shell_settings`; vale para ⌘, / Cmd+K / menu |
| App iOS | #570 | v0.2.94 | Reescrita do upstream aceita inteira. As adaptações antigas (OMP, streaming) precisam ser refeitas |
| Parser de markdown em `crates/markdown` | #570 | v0.2.94 | A heurística de path do fork foi para `zeron_markdown::file_path` |
| Queue compartilhada | — | v0.2.83 | Coexiste com o steer. Steers de harness que só lê no fim do turno ficam retidos |

## Armadilhas conhecidas

- **Campos a mais no proto do fork quebram literais do upstream:**
  - `RunRequest` tem `enable_workers_mcp`, `workers_parent_chat_id` e `sessions`;
  - `Chat` tem `origin_chat_id`;
  - `Session` tem `context_usage` e `error`;
  - `MessagePart` tem `completed`, `duration_ms`, `execution` e `file_preview`;
  - `AgentAccount` tem `usage_lines`.

  Crates novos do upstream (`client`, `mobile`) e testes não compilam até completar esses campos. Um script que acha literais `RunRequest {` sem esses campos poupa horas.
- **`serde_json` `preserve_order`** só é ativado no `cargo test --workspace` (unificação de features). Fixtures que comparam JSON como string passam em `-p` e falham no workspace. As fixtures precisam aceitar as duas ordens.
- **Testes WebRTC do `zeron-preview`** (`peer::tests::*`, `tests/leak.rs`) falham às vezes nesta máquina, igual no `origin/main`. Antes de culpar o merge, compare rodando no `origin/main` puro.
- **Settings do fork** gravam pelo `apply_shell_settings`. Campo novo do upstream em `UiSettings` precisa entrar ali, senão nunca é salvo.
- **Testes do upstream que usam `Shell::new`** passam a usar `shell::test_shell` (o fork tem o `WorkersModel`).
- **`crates/ui` com markdown:** o parser mora no crate compartilhado. Qualquer referência a `crate::file_preview` dentro dele quebra.
- **Resolver com agentes:** eles podem apagar código "morto" que outro arquivo ainda usa (por exemplo `motion::fast_tier`). A etapa de compilação precisa reconciliar isso.

## Histórico

| Data | Intervalo do upstream | Onde | Notas |
|---|---|---|---|
| 2026-08-30 | até v0.2.29 | `openspec/changes/archive/2026-08-30-sync-upstream-v0-2-29/` | Marco de proveniência em v0.2.18; merge conservador e porte por capability |
| 2026-09-10 | correções pontuais | `openspec/changes/archive/2026-09-10-adopt-upstream-followup-fixes/` | 7 correções, sem merge de ancestralidade |
| 2026-09-17 | melhorias de setembro | `openspec/changes/archive/2026-09-17-integrate-upstream-september-updates/` | Cmd+K, fontes, appshots, imagens geradas |
| 2026-09-22 | até v0.2.83 (`d721f301`) | `openspec/changes/sync-upstream-v0-2-83/` | Merge `55395013` via graft (upstream reassinado); v0.2.84 em `010e02d2` |
| 2026-09-27 | até `433aa148` (v0.2.94+) | `openspec/changes/sync-upstream-v0-2-94/` | Merge `1065d252`; iOS reescrito; o fork segura o grant MCP |

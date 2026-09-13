## Context

A aba Files tem duas fontes de árvore e a mutação precisa respeitar as duas (`crates/ui/src/details_sidebar/view.rs:921-1130`):

- **RPC** — Chat com `space_id` ou Space cujo path bate com o cwd. `ListWorkspaceDirectory` / `SearchWorkspaceFiles` / `WatchWorkspaceFiles` com `targetDeviceId`, resultado em `DirectoryCache`. É o único caminho que alcança checkout de peer device.
- **Scan local** — Workers e chat sem space. `scan_checkout` (`file_tree.rs:185`) com `WalkBuilder`, sem `.gitignore`, negando diretórios por nome.

O engine é read-only em Files: `WorkspaceFiles` só lista, busca, lê e observa (`crates/engine/src/workspace_files.rs`). A UI tem `rename_entry`/`delete_entry`/`move_entry` (`file_tree.rs:311-367`) jaulados e testados, mas **sem nenhum caller** — código órfão desde que a aba virou read-only.

`popover.rs` e o par `on_mouse_down` / `on_mouse_down_out` (`settings/appearance.rs:748`, `composer.rs:5238`) são o padrão de menu in-app. `ComposerInput::with_single_line` (`composer.rs:2033`) é o input de uma linha da casa, já usado na busca da própria aba e no rename de sessão (`workers/workspace.rs:3547`).

Motivação: proposal.md — Why. Contrato observável: `specs/workspace-file-management/spec.md`.

## Goals / Non-Goals

**Goals**

- Uma superfície de ação única na UI, com dois backends (RPC e fs local), para que o menu de contexto não conheça o tipo de contexto.
- Mutação de checkout peer-owned executada pelo engine dono, com a mesma autorização das leituras.
- Refresh cirúrgico pós-mutação: só os diretórios afetados, sem perder expansão/scroll.

**Non-Goals**

- Drag & drop na árvore, multi-seleção e undo.
- Lixeira do sistema (delete é permanente).
- Edição de conteúdo de arquivo pelo explorer (o preview segue read-only; `MAX_EDITABLE_FILE_BYTES` continua sem caller de escrita).
- Qualquer operação de git (stage/commit/discard) — muda em change própria.
- Overlay de status git nas rows da árvore.

## Decisions

### D1 — Duas implementações atrás de um seam, não um caminho único

A UI expõe uma enum de operação (`FileMutation::{CreateFile, CreateDir, Rename, Delete, Move, Copy}`) e um executor que escolhe o backend pelo mesmo predicado que já escolhe a fonte de leitura (`workspace_file_source`): há `WorkspaceTarget` → RPC; não há → fs local pelos helpers de `file_tree.rs`.

Alternativa rejeitada: mandar tudo por RPC. Contexto de Workers/chat sem space não tem `chatId` nem `spaceId`; `resolve_target` não teria o que resolver e a saída seria sintetizar um target falso ou registrar o folder à revelia do usuário. O split já existe para leitura; inventar um terceiro conceito seria pior.

Alternativa rejeitada: mutar sempre local. Mata o caso peer-owned, que é metade da razão do painel existir.

### D2 — Cinco métodos RPC, não um `MutateWorkspaceEntry` com discriminador

`rpc_methods!` (`crates/rpc/src/method.rs`) é o registro único de contrato; um método por operação mantém params tipados, erro específico e relay `forwardable` idêntico ao das leituras. Um método genérico com campo `op` empurraria o contrato para dentro do payload e obrigaria a validar combinação de campos em runtime.

`CopyWorkspaceEntry` e `MoveWorkspaceEntry` compartilham forma (`sourcePath`, `destinationDirectory`) mas não semântica de colisão (copy renomeia, move recusa) — por isso métodos separados, não um flag.

### D3 — Jaula reaproveitada, estendida para path que ainda não existe

`WorkspaceRelativePath` (`workspace_files.rs:108`) já recusa absoluto, `..`, `\`, `:`, `.git` e não-UTF8. Create precisa validar um path **inexistente** com múltiplos segmentos (`docs/adr/0001.md`), então a jaula ganha um construtor para path novo que valida cada componente com as mesmas regras e devolve o parent a criar. Nada de `canonicalize` no destino antes de existir; a checagem é sintática mais a verificação de que o parent resolvido continua dentro do root.

Symlink: o engine não segue link para escrever. Se qualquer componente do path resolvido for symlink, a operação é recusada (`Unsupported`), coerente com o `O_NOFOLLOW` da leitura.

Operação é atômica onde o SO deixa: rename/move usa `fs::rename` no mesmo filesystem; copy de diretório é recursivo e, se falhar no meio, remove o destino parcial antes de devolver o erro.

### D4 — Delete permanente com confirmação obrigatória, sem lixeira

Sem dependência nova (`trash` crate) e igual ao app de referência. A confirmação é exigida pela spec; o root nunca é alvo. Trade-off assumido em Risks.

### D5 — Nome único derivado, só para copy

Copy/duplicate com colisão gera `nome copy.ext`, depois `nome copy 2.ext`, comparando case-insensitive para não quebrar em APFS/NTFS. Move com colisão é erro — mover é uma operação que o usuário espera que preserve o nome; renomear escondido seria perda de dado silenciosa.

### D6 — Refresh dirigido pelo reply, watcher como rede

Cada reply de mutação devolve o path relativo resultante. A UI invalida e relista **só** os diretórios pai envolvidos (origem e destino), via o caminho que já existe (`load_workspace_files(silent, Some(dirs), …)` no RPC; rescan local no outro). O watcher (`WatchWorkspaceFiles` / notify local) continua ligado e cobre mutação externa, mas não é o caminho primário: esperar debounce deixaria a árvore mentindo por até 100ms + burst.

Expansão, seleção e scroll são estado da UI e não são tocados pelo refresh — é o que o cenário de reconciliação da spec garante.

### D7 — Menu de contexto in-app, itens derivados por função pura

`on_mouse_down(MouseButton::Right, …)` na row e no container do root abre um popover ancorado no ponteiro (padrão de `settings/appearance.rs`), não `popUpContextMenu` nativo: o menu precisa de itens desabilitados e de atalho visível, e o resto da aba já é gpui.

A lista de itens sai de uma função pura `file_menu_items(target_kind, access, clipboard_state) -> Vec<FileMenuItem>`, espelhando `workers/session_menu.rs:27` — é o que torna testável “root não tem Rename/Delete/Cut” e “checkout remoto não tem Finder/Terminal” sem harness de render.

Atalhos na árvore focada: ⌘C / ⌘X / ⌘V, F2, Delete/Backspace, Escape (limpa cut pendente e fecha input inline).

### D8 — Criação e rename inline com `ComposerInput::with_single_line`

Uma row de input aparece na posição destino (pasta nova antes das pastas, arquivo novo depois delas) e some no Enter/Escape/blur. Não há modal. Validação de nome roda antes da chamada: vazio, `.`/`..`, componente com `/` inicial, ou caractere reservado do SO → erro inline, nenhuma chamada disparada.

### D9 — Clipboard interno, não clipboard do SO

Cut/Copy guardam `(relative_path, is_dir, mode)` no estado da sidebar. Nada é escrito no clipboard do SO por Cut/Copy (senão colar em outro app entregaria um path e colar no explorer competiria com o conteúdo real do clipboard). Copy Path e Copy Relative Path — esses sim — escrevem string no clipboard do SO, como o botão de copy da row ativa já faz hoje.

## Risks / Trade-offs

- **Delete permanente sem undo** → confirmação explícita obrigatória, root protegido, e o texto do diálogo nomeia a entrada e diz que é permanente.
- **Mutação remota sem feedback de progresso** (copy recursivo grande pode passar do timeout de 6s dos handlers de Files) → copy/move rodam em `spawn_blocking` com timeout próprio, maior que o de leitura, e erro de timeout não deixa destino parcial (D3).
- **Dois backends divergirem** (RPC aceita o que o local recusa, ou vice-versa) → as regras de nome/colisão vivem numa função compartilhada; o teste de UI cobre a decisão, o teste de engine cobre o efeito no disco.
- **Race com o watcher**: refresh dirigido mais evento do watcher podem relistar o mesmo diretório duas vezes → é idempotente; `DirectoryCache::apply` substitui a página, não acumula.
- **Scan local não honra `.gitignore` de propósito** (`file_tree.rs:9-14`); criar arquivo ignorado continua aparecendo ali e não aparece na árvore via RPC sem `includeIgnored`. A UI já manda `includeIgnored: true`, então a assimetria não muda com esta change.

## Migration Plan

Nenhuma migração de dado. Compatibilidade de peer: device com engine antigo não conhece os métodos novos e responde erro de método desconhecido — a UI trata como o estado de incompatibilidade que a aba já tem para Files (`"Update the project device…"`), sem fallback local para checkout remoto.

## Open Questions

Nenhuma.

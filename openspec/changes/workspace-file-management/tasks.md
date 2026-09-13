## 1. Contrato de fio

- [x] 1.1 Em `crates/proto/src/entities.rs`, ao lado dos requests de Files, adicionar `CreateWorkspaceEntryRequest` (`target` flatten, `parentPath`, `name`, `kind: file|directory`), `RenameWorkspaceEntryRequest` (`path`, `newName`), `DeleteWorkspaceEntryRequest` (`path`), `MoveWorkspaceEntryRequest` (`sourcePath`, `destinationDirectory`), `CopyWorkspaceEntryRequest` (mesmos campos) e a reply comum `WorkspaceEntryMutation { path, isDirectory }`, todos camelCase e com campos novos opcionais/`default` — verificar com `cargo test -p zeron-proto`.
- [x] 1.2 Em `crates/rpc/src/method.rs`, adicionar as cinco entradas em `rpc_methods!` com `forwardable: true` e sem `stream`; verificar que `cargo test -p zeron-rpc method::tests` passa com o registro novo (nomes de fio idênticos às constantes).

## 2. Mutação no engine

- [x] 2.1 Em `crates/engine/src/workspace_files.rs`, estender a jaula com um construtor para path ainda inexistente (valida cada componente, recusa absoluto/`..`/`.git`/symlink no caminho resolvido) e cobrir com teste unitário de recusa por componente — `cargo test -p zeron-engine workspace_files`.
- [x] 2.2 Implementar `create_entry` (arquivo vazio ou diretório, criando pais intermediários, recusando colisão) em `spawn_blocking`, autorizado por `resolve_target`; teste unitário do efeito no disco.
- [x] 2.3 Implementar `rename_entry` (recusa root, recusa colisão, preserva conteúdo) e `delete_entry` (arquivo e diretório recursivo, permanente); testes unitários dos dois.
- [x] 2.4 Implementar `move_entry` (colisão = erro; destino descendente da origem = erro) e `copy_entry` (colisão = nome único `nome copy.ext` / `nome copy 2.ext` case-insensitive; cópia recursiva; limpa destino parcial em falha); testes unitários incluindo o caso de colar pasta dentro de si mesma.
- [x] 2.5 Em `crates/engine/src/rpc.rs`, adicionar os cinco braços em `EngineRpc::handle` com timeout próprio para as operações potencialmente longas (copy/move) e mapeamento de erro tipado; verificar compilando `cargo check -p zeron-engine`.
- [x] 2.6 Criar `crates/engine/tests/workspace_files_mutations.rs` cobrindo por RPC in-memory: criação (inclusive nome aninhado), rename com colisão recusada, delete recursivo, move, copy com nome único, path com `..` recusado e device não-dono recusado — `cargo test -p zeron-engine --test workspace_files_mutations`.
- [x] 2.7 Em `crates/engine/tests/device_routing.rs`, estender `workspace_file_surface` provando que uma criação em checkout peer-owned atravessa o relay e passa a ser listada — `cargo test -p zeron-engine --test device_routing`.

## 3. Superfície de ação na UI

- [x] 3.1 Em `crates/ui/src/details_sidebar/file_tree.rs`, completar os helpers locais: manter `rename_entry`/`delete_entry`/`move_entry` e adicionar `create_entry` e `copy_entry` com as MESMAS regras de nome/colisão do engine, extraindo a derivação de nome único e a validação de nome para funções compartilhadas cobertas por teste — `cargo test -p zeron-ui file_tree`.
- [x] 3.2 Definir em `view.rs` a enum `FileMutation` e o executor que escolhe RPC (`call_as` com `targetDeviceId`) ou fs local pelo mesmo predicado de `workspace_file_source`, devolvendo o path resultante; teste unitário da escolha de backend por contexto.
- [x] 3.3 Após cada mutação bem-sucedida, relistar só os diretórios pai afetados preservando expansão/seleção/scroll; teste unitário da reconciliação do `DirectoryCache` com outras pastas expandidas intactas.

## 4. Menu de contexto e toolbar

- [x] 4.1 Adicionar `file_menu_items(target_kind, access, clipboard)` como função pura no módulo do menu, listando New File, New Folder, Cut, Copy, Paste, Duplicate, Copy Path, Copy Relative Path, Rename, Delete, Open in Terminal, Reveal in Finder, com root sem Rename/Delete/Cut e checkout remoto sem Finder/Terminal; testes unitários dos dois recortes — `cargo test -p zeron-ui details_sidebar`.
- [x] 4.2 Abrir o menu por `on_mouse_down(MouseButton::Right)` na row e no container do root, renderizado como popover ancorado no ponteiro seguindo o padrão de `settings/appearance.rs`, fechando em `on_mouse_down_out`.
- [x] 4.3 Ligar cada item ao executor: Cut/Copy gravam o clipboard interno da sidebar; Paste chama Move/Copy; Duplicate chama Copy no próprio pai; Copy Path / Copy Relative Path escrevem no clipboard do SO; Reveal in Finder e Open in Terminal usam o caminho absoluto local.
- [x] 4.4 Adicionar New File e New Folder na toolbar da aba Files, com tooltip, resolvendo o pai como pasta selecionada → pai do arquivo selecionado → root.
- [x] 4.5 Remover os botões avulsos de copy path e reveal da row ativa, agora cobertos pelo menu, sem deixar ícone órfão em `render_file_row`.

## 5. Entrada inline e atalhos

- [x] 5.1 Renderizar a row de input inline (criação e rename) com `ComposerInput::with_single_line`, posicionada como pasta-nova antes das pastas e arquivo-novo depois delas, confirmando em Enter e descartando em Escape/blur; teste unitário do estado (cancelar não muta).
- [x] 5.2 Validar o nome antes de chamar o backend (vazio, `.`, `..`, componente inválido, colisão case-insensitive já conhecida na pasta) e mostrar o erro inline; teste unitário da validação.
- [x] 5.3 Ligar os atalhos na árvore focada: ⌘C, ⌘X, ⌘V, F2, Delete/Backspace e Escape (limpa cut pendente e fecha input); confirmar que Delete passa pela confirmação.
- [x] 5.4 Implementar o diálogo de confirmação de Delete nomeando a entrada e dizendo que a remoção é permanente; teste unitário de que recusar não muta.

## 6. Fechamento

- [x] 6.1 Rodar `cargo fmt --all` e `cargo test --workspace` uma única vez, com tudo já implementado.
- [x] 6.2 Atualizar a cadeia DOX: `crates/rpc/AGENTS.md` (métodos novos), `crates/engine/AGENTS.md` (Files deixa de ser só leitura; matriz de teste), `crates/ui/AGENTS.md` (aba Files: menu, mutação, dois backends) — e a Test Coverage Matrix correspondente.
- [x] 6.3 Validar a change (`openspec validate workspace-file-management --strict`) e provar o comportamento na UI rodando o app pelo roteiro de verificação do repo, registrando a evidência.

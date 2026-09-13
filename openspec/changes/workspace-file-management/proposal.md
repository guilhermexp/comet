## Why

A aba Files do Details Sidebar é hoje um visualizador: lista, pagina, busca, expande e abre preview. Qualquer operação de arquivo — criar, renomear, apagar, duplicar, mover — exige sair do app para um terminal ou editor externo, mesmo quando o checkout é local. Pior no caso remoto: um checkout de peer device só é alcançável pelo engine dono, então não existe caminho nenhum para mutar arquivo de outro device. O explorer precisa das operações de arquivo que todo explorer de IDE tem, com o mesmo alcance de leitura que o painel já tem (local e peer-owned).

## What Changes

- **Engine ganha mutações de workspace file** (`WorkspaceFiles`), hoje estritamente read-only: criar arquivo, criar diretório, renomear, apagar, mover e copiar — todas jauladas em `WorkspaceRelativePath` e autorizadas por `resolve_target` como as leituras.
- **Cinco métodos RPC novos**, `forwardable: true`, para que a operação execute no device dono do checkout: `CreateWorkspaceEntry`, `RenameWorkspaceEntry`, `DeleteWorkspaceEntry`, `MoveWorkspaceEntry`, `CopyWorkspaceEntry`.
- **Aba Files ganha menu de contexto** (botão direito em qualquer row e no root): New File, New Folder, Cut, Copy, Paste, Duplicate, Copy Path, Copy Relative Path, Rename, Delete, Open in Terminal, Reveal in Finder.
- **Toolbar ganha New File e New Folder**, criando dentro da pasta selecionada (ou do parent do arquivo selecionado, ou do root).
- **Criação e rename acontecem inline na árvore**, numa row de input, com validação de nome e recusa de colisão — não em modal.
- **Cut/Copy/Paste é clipboard interno do explorer** (move/copy no device dono), não clipboard do SO; Copy Path e Copy Relative Path escrevem no clipboard do SO.
- **Delete pede confirmação e é permanente** (sem lixeira, sem undo); o root nunca é alvo de rename/delete/cut.
- **Copy Path / Reveal in Finder deixam de ser botões só da row ativa** e passam ao menu; Reveal e Open in Terminal permanecem restritos a checkout local, ocultos no caso remoto.
- As operações valem tanto no caminho RPC (Chat/Space registrado) quanto no caminho de scan local (Workers/chat sem space), pela mesma superfície de ação da UI.

## Capabilities

### New Capabilities
- `workspace-file-management`: operações de mutação de arquivo do explorer (criar, renomear, apagar, mover, copiar, duplicar), autorização e jaula de path no engine dono, refresh da árvore após a mutação, e as ações de path/terminal/finder do menu de contexto.

### Modified Capabilities
<!-- Nenhuma. Browsing, paginação, busca e preview de `remote-files` seguem inalterados. -->

## Impact

- `crates/proto/src/entities.rs` — requests/replies novos ancorados em `WorkspaceTarget`.
- `crates/rpc/src/method.rs` — cinco entradas em `rpc_methods!`.
- `crates/engine/src/workspace_files.rs` — métodos de mutação + jaula + `spawn_blocking`; `crates/engine/src/rpc.rs` — braços novos em `EngineRpc::handle`.
- `crates/ui/src/details_sidebar/file_tree.rs` — helpers locais de mutação (hoje órfãos) passam a ser o backend do caminho local, mais criação e cópia; `crates/ui/src/details_sidebar/view.rs` — menu de contexto, toolbar, row de input inline, clipboard interno, refresh.
- `crates/engine/tests/workspace_files.rs` e `crates/engine/tests/device_routing.rs` — cobertura de mutação e de relay.
- DOX: `crates/rpc/AGENTS.md`, `crates/engine/AGENTS.md`, `crates/ui/AGENTS.md`.
- Sem dependência nova. Sem mudança de esquema persistido.

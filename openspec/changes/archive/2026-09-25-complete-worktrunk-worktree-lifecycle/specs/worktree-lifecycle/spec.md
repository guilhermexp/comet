## ADDED Requirements

### Requirement: Arquivos ignorados escolhidos pelo projeto acompanham um checkout novo

Quando `.worktreeinclude` existe no checkout principal, o serviço comum SHALL copiar para um checkout novo somente arquivos ignorados pelo Git que casem com os padrões do arquivo. Sem `.worktreeinclude`, a cópia SHALL ser omitida. Arquivos rastreados, metadados de VCS, worktrees aninhados e caminhos que saiam das raízes SHALL NOT ser copiados. Um destino existente SHALL NOT ser sobrescrito. A cópia SHALL tentar reflink e usar cópia comum quando o sistema de arquivos não oferecer reflink. Falha de cópia SHALL manter o checkout e sua associação, bloquear o primeiro run e permitir retry de preparo no mesmo caminho.

#### Scenario: Cache selecionado chega ao checkout novo
Test: unit — `chat_and_workers_copy_only_selected_gitignored_files_before_setup` e `copy_ignored::tests::copies_only_included_ignored_files_from_the_principal_worktree` em repositórios temporários.

- **WHEN** Chat ou Worker cria um checkout
- **THEN** o arquivo incluído aparece com o mesmo conteúdo no novo checkout antes do run
- **AND** o outro arquivo não aparece
- **AND** um arquivo rastreado nunca substitui o conteúdo da versão do checkout novo

#### Scenario: Preparar novamente preserva arquivo existente
Test: unit — `copy_ignored::tests::rerun_preserves_existing_destination_contents` e `failed_copy_retries_on_the_same_chat_checkout_before_running_setup`.

- **WHEN** o preparo é repetido no mesmo checkout
- **THEN** o arquivo preexistente mantém seu conteúdo
- **AND** apenas arquivos elegíveis ausentes são copiados

#### Scenario: Sem opt-in não há cópia
Test: unit — `absent_worktreeinclude_does_not_copy_ignored_files`.

- **WHEN** um checkout novo é criado
- **THEN** os arquivos ignorados do principal não são copiados

### Requirement: Branch integrada sai após remoção do checkout gerenciado

Após a remoção física segura de um checkout gerenciado, o serviço comum SHALL tentar apagar sua branch local somente quando ela não for a padrão, não estiver em outro worktree, não tiver mudado desde a avaliação e estiver integrada à branch padrão. São evidências de integração: mesmo commit, ancestralidade, diff three-dot vazio, árvore idêntica ou merge simulado que não acrescenta alterações. Falha ou resultado inconclusivo SHALL preservar a branch e gerar aviso; nenhuma branch remota SHALL ser alterada.

#### Scenario: Branch integrada é removida
Test: unit + integration — `branch_cleanup::tests::deletes_a_squash_merged_branch_when_main_has_later_changes`, `project_actions::worktree_lifecycle_removes_checkout_but_retains_child_history` e `m5_repos_diffs_terminals::repos_round_trip_add_branches_worktrees`.

- **WHEN** o checkout limpo de uma branch comprovadamente integrada é removido
- **THEN** a branch local também é removida

#### Scenario: Trabalho não integrado permanece
Test: unit + integration — `branch_cleanup::tests::retains_a_branch_with_unmerged_work` e `m5_repos_diffs_terminals::repos_round_trip_add_branches_worktrees`.

- **WHEN** o checkout limpo é removido
- **THEN** a branch permanece no commit original

#### Scenario: Branch alterada durante a remoção permanece
Test: unit — `branch_cleanup::tests::retains_a_branch_whose_ref_moved_after_the_expected_oid_was_captured` e `a_moving_default_ref_aborts_the_atomic_branch_delete`.

- **WHEN** a ref da branch já não aponta para o commit avaliado
- **THEN** o serviço não a apaga
- **AND** o checkout removido não reaparece

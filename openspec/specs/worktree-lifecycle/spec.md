# worktree-lifecycle Specification

## Purpose

Governar um único ciclo local de criação, preparo, uso e remoção de worktrees para Chats e
Workers, com prova de posse e um subconjunto explícito dos hooks de projeto do Worktrunk.

## Requirements

### Requirement: Chat e Workers criam worktrees sob a mesma raiz

Todo worktree novo criado pelo app SHALL ficar sob `~/.zeron/worktrees` ou sob
`ZERON_WORKTREES_DIR` quando definido e não vazio. Repositórios distintos com o mesmo basename
SHALL receber diretórios distintos. O Chat SHALL manter sua branch `zeron/<nome>` e o Worker a
branch escolhida para ele. Checkouts existentes SHALL NOT ser movidos.

#### Scenario: Mesma raiz, repositórios distintos
Test: integration — `project_actions` e `m5_repos_diffs_terminals` com raiz temporária.

- **WHEN** Chat e Worker criam worktrees do mesmo repositório e outro repositório homônimo cria o seu
- **THEN** os dois primeiros ficam sob o mesmo diretório de repositório na raiz canônica
- **AND** o terceiro fica em outro diretório
- **AND** nenhum worktree novo aparece em `~/.unpeel/worktrees`

### Requirement: Posse exige evidência de criação, além de localização

Para autorizar remoção física, o app SHALL exigir evidência durável de que criou o checkout exato,
compatível com o repositório Git e a identidade do worktree observados agora. Estar sob a raiz atual
ou uma raiz legada, ter branch `zeron/*` ou estar associado a um projeto SHALL NOT, isoladamente,
provar posse. Um worktree externo SHALL poder ser usado e associado sem se tornar apagável pelo app.
Worktrees legados sem evidência suficiente SHALL permanecer no disco e receber diagnóstico de
recuperação, sem migração presumida de posse.

Uma falha depois de `git worktree add` SHALL conservar o checkout. Reconciliação automática SHALL
ser permitida somente quando o registro pendente inclui a identidade Git observada após o add e
ela ainda confere; sem essa prova, recuperação SHALL exigir confirmação explícita. Renomear a
branch do mesmo checkout SHALL NOT apagar sua prova de posse.

#### Scenario: Externo dentro da raiz continua externo
Test: integration — `project_actions` e `DeleteWorktree` sobre worktree criado manualmente na raiz temporária.

- **WHEN** `git worktree add` cria um checkout dentro da raiz do Comet e ele é associado a um projeto
- **THEN** Chat e Worker podem usá-lo
- **AND** ambos os caminhos de remoção física o recusam e preservam pasta e branch
- **AND** a associação não executa hooks de início nem setup de criação

#### Scenario: Evidência legada comprovada continua válida
Test: integration — registro Workers legado com `AppManaged` e identidade Git compatível.

- **WHEN** um checkout criado pelo app antes da migração tem evidência de posse e identidade intactas
- **THEN** ele continua elegível para remoção, sujeito às demais guardas

#### Scenario: Chat legado sem proveniência não é assumido como próprio
Test: integration — registro de Chat anterior sem prova local de criação.

- **WHEN** a engine recebe `DeleteWorktree` para esse checkout
- **THEN** ela explica que a posse não foi comprovada e preserva o checkout

#### Scenario: Registro interrompido e branch renomeada
Test: integration — falha após registrar identidade Git e rename posterior da branch.

- **WHEN** o serviço reconcilia seu journal pendente com o mesmo worktree Git
- **THEN** ele recupera a prova desse checkout sem assumir posse de outro
- **AND** uma mudança posterior de nome de branch não invalida a prova

### Requirement: Preparo concluído precede execução

Depois da criação, o mesmo setup `.comet/worktree.json` ou `.cursor/worktrees.json` SHALL rodar para
Chat e Worker antes do primeiro run/launch. Falha SHALL preservar checkout e associação, nomear o
comando e impedir o harness/preset de iniciar. Retry de preparo SHALL reutilizar o mesmo checkout.
`post-start` SHALL iniciar somente após o preparo bloqueante ter sucesso. Se o journal de ownership
não puder ser lido ou não corresponder à identidade Git registrada, o preparo SHALL falhar fechado e o
Run SHALL NOT iniciar; um checkout sem registro no journal é externo e segue utilizável.

#### Scenario: Journal ilegível não pula o preparo
Test: unit — `chat_preparation_fails_closed_on_unreadable_journal_but_accepts_external_checkout`.

- **WHEN** o journal está corrompido e um Chat prepara um checkout do app
- **THEN** o preparo falha e nenhum Run começa sem cópia, setup e hooks
- **AND** um checkout sem registro no journal continua utilizável

#### Scenario: Setup de Chat bem-sucedido
Test: integration — engine em repositório temporário com setup que grava marcador.

- **WHEN** um Chat materializa um worktree novo
- **THEN** o marcador existe antes da primeira resposta do harness

#### Scenario: Setup falha e retry reutiliza o checkout
Test: integration — setup falho no primeiro envio, corrigido antes do segundo.

- **WHEN** o setup do Chat falha
- **THEN** o erro é visível, o harness não inicia e o `cwd` do Chat aponta ao checkout preservado
- **AND** o retry no mesmo checkout permite iniciar após corrigir o preparo

### Requirement: Execução local ativa bloqueia remoção por qualquer entrada

Enquanto um Worker está vivo, um Chat hospedado neste device está `Working`, ou há preparo/launch
em curso no caminho canônico de um checkout, qualquer entrada de remoção física SHALL recusar esse
checkout. A decisão SHALL vir de
estado device-local consultado pelo serviço, sem depender de uma lista opcional fornecida pela UI.
Se a atividade local não puder ser verificada com segurança, a remoção SHALL ser recusada; expiração
de heartbeat sozinha SHALL NOT provar que a execução acabou.

Um terminal do app aberto dentro do checkout SHALL bloquear apenas a remoção física; SHALL NOT
bloquear mover um Chat para o checkout, o preparo nem Runs. O encerramento ordenado do processo SHALL
liberar as reservas de terminal antes de terminar. Reservas `Terminal` e `Removing` cujo processo
registrado não existe mais SHALL ser recuperadas; as demais reservas órfãs continuam fechadas.

#### Scenario: Terminal aberto bloqueia só a remoção
Test: integration — `open_terminal_blocks_removal_until_it_closes` e
`terminal_inside_linked_checkout_reserves_it_only_against_removal`.

- **WHEN** um terminal está aberto numa subpasta do checkout
- **THEN** a remoção é recusada e a pasta permanece
- **AND** o checkout continua disponível para Chats; fechar o terminal libera a remoção

#### Scenario: Processo encerrado não deixa reserva de terminal
Test: unit — `removing_and_terminal_entries_of_a_dead_process_are_reclaimed_but_other_kinds_stay_busy`
e `release_and_wait_removes_the_entry_before_returning`.

- **WHEN** o app encerra normalmente ou morre com um terminal aberto
- **THEN** a reserva é liberada no shutdown ou recuperada quando seu processo não existe mais

#### Scenario: Chat ativo bloqueia remoção Workers
Test: integration — `project_actions` com Chat `Working` no checkout; UI chama o mesmo cliente.

- **WHEN** o cliente Workers pede remoção enquanto esse Chat trabalha
- **THEN** a operação é recusada e a pasta permanece

#### Scenario: Worker vivo bloqueia DeleteWorktree
Test: integration — `DeleteWorktree` com Worker vivo no mesmo checkout.

- **WHEN** a engine recebe a remoção
- **THEN** ela a recusa e a pasta permanece

#### Scenario: Run começa durante pedido de remoção
Test: integration — início de run e remoção concorrentes no mesmo checkout.

- **WHEN** as duas operações disputam o checkout
- **THEN** somente uma avança; nenhum run escreve numa pasta já removida

#### Scenario: Preparo ou launch ainda em curso bloqueia remoção
Test: integration — setup e spawn de Worker atrasados com remoção concorrente.

- **WHEN** o serviço já criou o checkout e ainda prepara ou lança seu Worker
- **THEN** a remoção o trata como ocupado até o resultado ficar registrado

### Requirement: Hooks de projeto selecionados são executados com semântica declarada

O app SHALL ler `.config/wt.toml` do checkout de origem escolhido para criar e do checkout a
remover para remover. SHALL aceitar `pre-start`, `post-start`, `pre-remove` e `post-remove` como
string, tabela concorrente ou pipeline ordenado. O subconjunto de templates suportado SHALL ser:
`branch`, `worktree_path`, `worktree_name`, `repo`, `repo_path`, `primary_worktree_path`, `commit`,
`short_commit`, `base`, `default_branch`, `hook_type`, `cwd` e `sanitize`. Um token ou sintaxe fora
desse conjunto SHALL produzir erro que o identifique antes de executar qualquer comando daquele
hook. Para evitar interpolação sem escape, comandos com template e continuação de linha por
backslash,
here-doc, expansão shell aninhada ou aspas ANSI SHALL ser recusados antes de executar qualquer etapa.
Um `pre-*` aprovado SHALL bloquear a operação e sua falha SHALL abortar run/launch ou
remoção, preservando o checkout. `post-*` aprovado SHALL rodar em segundo plano, com log. O ciclo
nativo SHALL funcionar sem binário `wt`; um hook que invoca `wt` depende dele.

#### Scenario: Formatos string, tabela e pipeline
Test: unit — parser/executor com marcadores em diretório temporário.

- **WHEN** um hook aprovado contém etapas em pipeline e comandos concorrentes dentro de uma etapa
- **THEN** as etapas executam em ordem e o passo seguinte só inicia após o anterior ter sucesso

#### Scenario: Pre-start falho bloqueia o launch
Test: integration — `launch_worker` em projeto cujo `pre-start` aprovado sai com erro.

- **WHEN** o Worker cria o checkout
- **THEN** o checkout fica registrado e o preset não inicia
- **AND** o erro nomeia o hook que falhou

#### Scenario: Template não suportado não executa parcialmente
Test: unit — hook com comando válido seguido de `{{ branch | hash_port }}`.

- **WHEN** o hook é preparado
- **THEN** nenhum comando daquele hook roda
- **AND** o erro identifica `hash_port` como não suportado

### Requirement: Hooks de projeto requerem aprovação local

O app SHALL mostrar em Settings ▸ Projects os comandos vindos de `.config/wt.toml`, sua origem e
estado de aprovação. Alterar o texto de um comando SHALL exigir nova aprovação. Um `pre-start` ou
`pre-remove` pendente SHALL bloquear respectivamente run/launch ou remoção; `post-*` pendente SHALL
ser pulado com aviso. A remoção SHALL oferecer uma escolha explícita e auditável de pular hooks,
sem pular prova de posse, atividade ou limpeza. O setup configurado pelo usuário no Comet mantém
seu contrato atual.

#### Scenario: Pre-remove pendente preserva checkout
Test: integration — checkout próprio com `pre-remove` não aprovado.

- **WHEN** a remoção é pedida sem escolha de pular hooks
- **THEN** ela informa a aprovação pendente e preserva checkout e branch

#### Scenario: Comando alterado pede aprovação novamente
Test: unit — aprovar, alterar o comando e consultar o estado.

- **WHEN** um comando aprovado muda
- **THEN** ele não é executado até nova aprovação

#### Scenario: Pular hooks não pula guardas de segurança
Test: integration — remoção com opção sem hooks sobre checkout externo ou com mudanças locais.

- **WHEN** o usuário escolhe remover sem hooks
- **THEN** a remoção ainda recusa checkout sem posse ou sujo

### Requirement: O orquestrador pode criar um checkout por Worker

`launch_worker` SHALL aceitar uma opção `new_worktree` com branch e base opcional, mutuamente
exclusiva com o par de caminho/branch de worktree existente. O app SHALL criar, registrar e preparar
o checkout antes de lançar o preset e SHALL executar o Worker exatamente nele. Falha posterior
SHALL preservar o checkout e informar o projeto e caminho para recuperação. O briefing do
orquestrador SHALL indicar a opção para fatias independentes; execução no checkout atual continua
possível quando escolhida explicitamente.

#### Scenario: Dois Workers independentes não compartilham checkout
Test: integration — `controller_mcp` lança dois presets com `new_worktree` distintos.

- **WHEN** o orquestrador lança dois Workers com branches novas diferentes
- **THEN** cada Worker recebe um checkout e `cwd` próprios

#### Scenario: Falha de launch preserva checkout criado
Test: integration — preset válido e falha injetada na inicialização do host após a criação.

- **WHEN** a criação funciona e o launch falha
- **THEN** o erro informa o projeto criado e seu caminho
- **AND** o checkout não é apagado

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

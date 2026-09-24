# Tasks

## 1. Prova de posse e localização

- [ ] 1.1 Implementar a raiz comum com `ZERON_WORKTREES_DIR` e diretório `<slug>-<fnv1a:08x>` por caminho canônico de repositório. Verify: unit — repos homônimos divergem; Chat e Worker usam a mesma raiz temporária.
- [ ] 1.2 Gravar `PendingCreate` antes de `git worktree add`, `CreatedObserved` com identidade Git depois e `Owned` antes do setup. Reconciliar automaticamente só `CreatedObserved` compatível; sem essa prova, exigir recuperação explícita. Verify: integration — falhas nas duas gravações preservam checkout sem `--force`; branch renomeada não perde posse.
- [ ] 1.3 Migrar somente registros Workers legados `AppManaged` cuja evidência Git ainda bate; classificar Chat legado sem proveniência e worktree externo como não removíveis pelo app. Verify: integration — casos positivos e negativos em `project_actions`.
- [ ] 1.4 Cobrir worktree externo criado com `git worktree add` **dentro** da raiz do Comet, inclusive via symlink e `DeleteWorktree` forwardable. Verify: integration — Worker pode executar nele; UI, controller e engine recusam remoção física; pasta e branch sobrevivem.

## 2. Atividade local e coordenação

- [ ] 2.1 Introduzir a fonte device-local de Chats `Working`, preparo e launch em curso, com caminho canônico, operação/run ID e heartbeat/lease. Expiração sozinha não libera checkout: reconciliar com o host ou falhar fechado. Verify: unit — início, assentamento, preparo, crash/expiração e leitura indisponível.
- [ ] 2.2 Fazer início de run, registro de `Preparing`/`StartingWorker` e remoção participarem do `CheckoutActionLock`; o serviço consulta Chats e Workers vivos por conta própria, sem parâmetro opcional da UI. Verify: integration — run, setup e spawn concorrentes com remoção, inclusive via controller MCP.
- [ ] 2.3 Aplicar a mesma consulta de Worker vivo no Retarget local, preservando a limitação de Chat hospedado em outro device. Verify: integration — Worker vivo recusa `SetChatCwd` com motivo; parado permite.

## 3. Serviço comum de criação e remoção

- [ ] 3.1 Substituir `unpeel_core::worktrees::create` em `create_worktree_at` pelo serviço comum, preservando seleção de base/fetch e adoção sem transferir posse nem rodar setup/hooks de início. Verify: `cargo test -p zeron-workers-unpeel --test project_actions` com raiz temporária; worktree adotado fica externo e seus arquivos não mudam.
- [ ] 3.2 Mover criação do Chat em `Repos` para o serviço em `spawn_blocking` com timeout, mantendo nome e shape do RPC. Verify: integração `m5_repos_diffs_terminals` — path novo e branch `zeron/*` corretos.
- [ ] 3.3 Registrar associação de Chat/Worker e `Preparing` antes do preparo; em falha de `.comet`/`.cursor`, conservar checkout, bloquear harness/preset e fazer o próximo pedido do usuário repetir o preparo no mesmo caminho. Verify: integração — primeiro preparo falha, segundo funciona sem criar outra pasta.
- [ ] 3.4 Compartilhar a remoção sem `--force` ou fallback recursivo. Sob lock, provar posse e identidade, consultar atividade, executar `pre-remove`, revalidar identidade/atividade/limpeza e remover; branch sempre permanece. Verify: `project_actions` e `m5_repos_diffs_terminals` cobrem limpo, sujo, externo, ocupado e checkout ausente.
- [ ] 3.5 Atualizar o caminho de remoção Workers para conservar registro/histórico em sucesso ou falha e retirar rollback `unpeel_core::worktrees::remove(..., true)` da criação. Verify: integration — falha de registro/launch não apaga checkout; remoção preserva sessões e branch.

## 4. Hooks selecionados do Worktrunk

- [ ] 4.1 Ler `.config/wt.toml` do checkout de origem na criação e do alvo na remoção. Parsear string, tabela e pipeline apenas para `pre/post-start` e `pre/post-remove`; outros campos do arquivo são ignorados. Verify: unit — os três formatos e fontes distintas.
- [ ] 4.2 Validar todos os comandos de um hook antes de executar qualquer um; renderizar somente as variáveis listadas na spec e `sanitize`, com escaping shell. Token, condicional ou filtro não suportado produz erro nomeado. Verify: unit — branch com espaços/barra, `hash_port`, `vars.*` e pipeline sem execução parcial.
- [ ] 4.3 Executar `pre-*` com timeout e falha bloqueante; `post-*` em segundo plano com log e cwd correto (`post-remove` no principal). Rastrear/encerrar somente processos `post-start` que o Comet iniciou. Verify: integração — ordem, falha, log e processo próprio encerrado.
- [ ] 4.4 Persistir aprovação local vinculada à identidade do repo e ao texto de cada comando; expor inspeção/aprovação em Settings ▸ Projects. `pre-*` pendente bloqueia, `post-*` pendente gera aviso. Verify: unit/integration — mudança de comando revoga aprovação; checkout preservado.
- [ ] 4.5 Oferecer remoção explícita sem hooks com confirmação/auditoria, sem pular posse, atividade ou limpeza. Executar `pre-remove` antes da última checagem de sujeira para permitir limpeza de caches. Verify: integração — hook limpa cache ignorado e remoção passa; externo ou untracked restante continuam bloqueados.

## 5. Isolamento do orquestrador

- [ ] 5.1 Adicionar `launch_worker.new_worktree: { branch, base_ref? }` ao parser/schema do controller MCP, exclusivo de `worktree_path`/`worktree_branch`; validar projeto Git e preset antes da criação. Verify: `cargo test -p zeron-workers-unpeel --test controller_mcp` para payloads válidos e inválidos.
- [ ] 5.2 Reusar criação, registro e preparo comuns antes do launch; retornar ID do projeto, path, branch e session ID. Falha de preparo/launch devolve ID/path recuperáveis e nunca apaga checkout. Verify: integração — dois Workers independentes recebem cwd diferentes; falha conserva checkout.
- [ ] 5.3 Atualizar `help`/briefing do controller para orientar worktree novo por fatia independente e manter launch no checkout atual como escolha explícita. Verify: teste do schema/help e leitura do briefing gerado.

## 6. Documentação e comprovação

- [ ] 6.1 Atualizar DOX de `crates/workers-unpeel`, `crates/engine`, `crates/ui`, pais afetados e Test Coverage Matrix; registrar a distinção localização/posse, estado local, hooks parciais e branch preservada. Verify: leitura da cadeia de cada path editado.
- [ ] 6.2 Rodar `cargo test -p zeron-workers-unpeel`, `cargo test -p zeron-engine`, `cargo test -p zeron-ui`, `cargo fmt --all --check` e `openspec validate unify-worktree-lifecycle --strict`. Verify: todos verdes ou falhas preexistentes documentadas com evidência.
- [ ] 6.3 Validar no app normal Settings ▸ Projects (aprovação/diagnóstico), criação Chat, lançamento isolado de dois Workers e recusa de remoção de externo/ocupado; registrar evidência visual. Verify: fluxo observado no checkout de execução.
- [ ] 6.4 Arquivar a change somente após comprovação e revisão dos contratos de posse, remoção e controller MCP. Verify: specs canônicas sincronizadas e nenhuma task pendente.

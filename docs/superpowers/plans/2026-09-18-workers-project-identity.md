# Identidade de projetos, worktrees e PRs no Comet — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Delegação limitada às unidades e dependências descritas abaixo.

**Goal:** Um projeto por repositório em Workers e Projects, com seus checkouts/branches/PRs vinculados e histórico preservado depois da remoção de pastas.

**Architecture:** Identidade local persistente e aditiva, separada do cwd de execução; reconciliação conservadora; uma associação compartilhada pelas duas projeções de UI. PR continua específico de checkout/branch. Nenhuma migração destrutiva.

**Tech Stack:** Rust 2024, serde/serde_json, Git CLI, estado local unpeel com lock e rename atômico, GPUI.

**Spec:** [proposta](../../../openspec/changes/archive/2026-09-18-stabilize-workers-project-identity/proposal.md), [design e diagnóstico](../../../openspec/changes/archive/2026-09-18-stabilize-workers-project-identity/design.md), [contratos](../../../openspec/changes/archive/2026-09-18-stabilize-workers-project-identity/specs/), [checklist canônico](../../../openspec/changes/archive/2026-09-18-stabilize-workers-project-identity/tasks.md).

**Status:** Implementado, validado e integrado no checkout `comet` em 18/09/2026. O checklist executado está no change OpenSpec; evidências em `docs/verification/2026-09-18-workers-project-identity.md`. Aceite e publicação são etapas posteriores.

## Global Constraints

- Ler AGENTS da raiz, `crates`, `crates/workers-unpeel`, `crates/ui` e os de `third_party` antes dos respectivos edits. Preservar alterações concorrentes já presentes, especialmente `controller_mcp.rs`, `ui/AGENTS.md` e outros arquivos dirty. Usar checkout isolado para execução, com target de build próprio.
- Seguir Contexto → Especificação → TDD → Delegação → Implementação → Comprovação → Revisão → Validação → Aceite → Publicação → Doc. Preparar contratos e testes antes de distribuir implementação.
- Sem migração no ambiente real durante desenvolvimento; usar fixtures sanitizadas e diretórios temporários. Nenhum prompt/transcrição no relatório de diagnóstico.
- Preservar project/session IDs, cwd de lançamento, nomes, ícones, datas, configurações e chaves JSON desconhecidas. Novos campos opcionais. Sem dependência invertida vendor → adapter.
- Não vincular por basename, prefixo de pasta ou URL remota isoladamente. Não forçar branch `main`. Não apagar diretório, branch ou sessão na reconciliação.
- Não reutilizar PR entre checkouts. Erro de probe não equivale a pasta excluída ou ausência de PR.
- Arquivos OpenSpec são a fonte do contrato; este plano detalha execução. Atualizar juntos quando o escopo mudar.

---

## Resultado visível e limites

```text
Workers                              Projects
Comet                                Comet
  Workers do checkout principal        Checkout principal
  fix/provider-aware-chat-titles       Worktrees disponíveis
    Workers desse checkout               fix/provider-aware-chat-titles
  Histórico                            Histórico de checkouts
    checkout removido                    checkout removido
Associação pendente                  Associação pendente
  registro antigo sem prova            registro antigo sem prova
```

O contêiner Comet permanece o mesmo ao mudar branch, abrir PR, remover um worktree ou cadastrar o principal depois dos filhos. O principal pode estar em qualquer branch. PR é badge do checkout correspondente. Pastas não Git continuam projetos independentes. Clones separados, ainda que com mesmo remote, continuam separados por padrão.

A inspiração no Orchestrator é a identidade durável; não copiar sua chave de remote como solução universal. O diagnóstico e as três alternativas consideradas estão no design.

## Contratos propostos antes da delegação

Novo módulo adapter `crates/workers-unpeel/src/project_identity.rs`. As assinaturas abaixo são alvo de implementação, não APIs existentes:

```rust
fn probe_checkout(path: &Path) -> CheckoutObservation;
fn plan_reconciliation(
    state: &serde_json::Value,
    observations: &[CheckoutObservation],
) -> ReconciliationPlan;
fn apply_reconciliation(
    state: &mut serde_json::Value,
    plan: &ReconciliationPlan,
) -> Result<MigrationReport, IdentityConflict>;
fn project_catalog(
    identity: &IdentityRegistry,
    ledger: &[LedgerProject],
    projects: &[WorkersProject],
) -> ProjectCatalog;
```

`CheckoutObservation`: caminho consultado/canônico, common directory e principal opcionais, branch atual ou detached OID, remotes nomeados, disponibilidade e erro de probe. `IdentityRegistry`: versão, repositórios por ID, checkouts por ID, índice de aliases de caminho e supressões do ledger. `ReconciliationPlan`: observações, precondições dos registros afetados e patches; IDs novos só para itens realmente novos, reutilizados no mesmo plano. `MigrationReport`: associados, preservados, pendentes, conflitos e versão; sem conteúdo de sessões.

`ProjectCatalog`: contêineres sem capacidade de execução, filhos com ID/cwd original, histórico e pendentes. `apply_reconciliation` só altera os campos previstos e rejeita precondição alterada; chamador refaz descoberta/plano em conflito. Disponibilidade e branch atuais são projeções observadas, não verdades permanentes de migração.

JSON wire: namespace `comet_project_identity` com versão inicial 1. Snapshot publica campos opcionais `repositoryID`, `checkoutKind`, `checkoutOwnership`, `checkoutAvailability` e `lastKnownBranch`; conserva `projectID`, `parentProjectID`, `gitBranch` e `worktreeBranch` para compatibilidade. O parentesco lógico vem de `repositoryID`; `parentProjectID` continua representando somente relações reais entre registros, sem ID sintético de execução. Versão desconhecida é preservada e não reescrita; mostrar diagnóstico em vez de reinterpretar silenciosamente.

## Task 1 — Fixar regressões e contrato

**Arquivos:** criar `crates/workers-unpeel/src/project_identity.rs` e fixtures em `crates/workers-unpeel/tests/fixtures/project-identity/`; testes co-localizados em `project_ledger.rs` e `lib.rs`; testes de snapshot em `third_party/unpeel/crates/unpeel-core/src/controller_host.rs`.

**Consome:** estado legado e fixtures Git temporárias. **Produz:** reprodução automatizada e contrato wire, sem migrar dados reais.

- [ ] Conferir os quatro changes relacionados e registrar no design quais implementações/testes de fato existem. Não confiar em checkboxes antigos.
- [ ] Criar fixture com principal, worktree criado externamente, sessões distintas e grupo com path duplicado. Exercitar cadastro, snapshot, remoção externa e novo snapshot. Assert central: mesmo repository ID, mesmo session ID, checkout indisponível e não nova raiz.
- [ ] Criar fixture do principal não registrado; adicionar principal depois não pode duplicar contêiner. Adicionar fixture legada sem pasta nem metadados Git: deve ficar pendente.
- [ ] Rodar `cargo test -p zeron-workers-unpeel project_identity` e o teste de snapshot; registrar falha comportamental esperada, não aceitar somente erro de compilação como reprodução.
- [ ] Fixar os tipos e o JSON opcional acima com serialização de ida/volta, inclusive campos desconhecidos e versão futura. Unidade de commit: contrato e regressões reproduzidas; não publicar branch com suíte deliberadamente quebrada.

Exemplo de sequência de asserções a traduzir para o helper real da fixture:

```rust
assert_eq!(after.repository_id, before.repository_id);
assert_eq!(after.session_ids, before.session_ids);
assert_eq!(after.availability, CheckoutAvailability::Missing);
assert!(!after.is_independent_project);
```

## Task 2 — Descoberta e persistência no cadastro

**Arquivos:** `project_identity.rs`, `lib.rs`, `controller_mcp.rs`; projeção vendor em `controller_host.rs`.

**Consome:** contrato da Task 1. **Produz:** cadastro estável por checkout com identidade do repositório, sem mudar cwd.

- [ ] Escrever casos Git para principal, worktree externo/gerenciado, detached, symlink, caminho com espaços, repositório sem remote, remote sem origin e clones diferentes com mesma URL. Rodar filtro `project_identity` e observar casos ainda vermelhos.
- [ ] Implementar probe limitado por timeout usando `git rev-parse --git-common-dir`, raiz e lista de worktrees; normalizar saídas relativas em relação ao cwd apropriado. Não consultar rede.
- [ ] Em `add_project` e `create_worktree_at`, persistir associação e cadastro na mesma edição atômica. A criação via Comet é a evidência de ownership; estar sob uma pasta chamada worktrees não basta.
- [ ] Adaptar snapshot e `WorkersProject` para os campos opcionais; fallback legado continua disponível até reconciliação. Excluir grupos do índice Git.
- [ ] Expor identidade e disponibilidade no `list_projects` do controller. Preservar exigência de registrar o cwd exato antes de `launch_worker`; testar que o Worker externo não executa na raiz.
- [ ] Rodar `cargo test -p zeron-workers-unpeel project_identity` e `cargo test --manifest-path third_party/unpeel/crates/Cargo.toml -p unpeel-core controller_host` pelo workspace vendorizado (o pacote não é membro do workspace Comet). Unidade de commit: descoberta + cadastro + compatibilidade.

## Task 3 — Migrar e reconciliar o histórico

**Arquivos:** `project_identity.rs`, `project_ledger.rs`, `lib.rs`; fixtures de Task 1.

**Consome:** cadastro/snapshot da Task 2. **Produz:** relatório e patches idempotentes do legado.

- [ ] Escrever teste de estado misto: worktree disponível, conhecido removido, desconhecido removido, ledger-only, grupo, paths reutilizados e metadados personalizados. Comparar sessões/IDs/datas/chaves extras antes/depois.
- [ ] Implementar `plan_reconciliation` puro e dry-run, seguido de aplicação revalidada dentro de `app_state::edit_at`. Probe fora do lock. Conflitos refazem o plano; não aplicar estado carregado antigo por inteiro.
- [ ] Criar backup local restrito e journal antes da primeira escrita, preservando estado contemporâneo. Testar falha de escrita: arquivo anterior íntegro, sem migração parcial.
- [ ] Implementar reconciliação incremental na inicialização/background e cadastro; segundo passe sem mudanças não escreve nem cria IDs. Testar entrada legada criada depois da primeira migração.
- [ ] Persistir decisões manuais de associação e operação inversa restrita. Testar conflito de evidência e rollback com sessão criada depois da migração.
- [ ] Adicionar supressão de metadados esquecidos sem apagar referências mínimas das sessões; re-add explícito restaura. Testar polling e restart.
- [ ] Rodar `cargo test -p zeron-workers-unpeel project_identity` e `cargo test -p zeron-workers-unpeel project_ledger`. Unidade de commit: reconciliação e histórico. Nenhuma execução contra `~/.unpeel/app-state.json` real nesta etapa.

## Task 4 — Unificar catálogo, Workers e PR

**Arquivos:** `project_identity.rs`, `project_ledger.rs`, `lib.rs`, `crates/ui/src/workers/workspace.rs`, `presentation.rs`, `model.rs`, `crates/ui/src/change_requests.rs`.

**Consome:** identidade reconciliada. **Produz:** `ProjectCatalog` e projeção Workers estável.

- [ ] Escrever testes puros de árvore/filtro: um contêiner, Workers principais diretos, branches filhas, contêiner sem registro principal, grupos organizacionais e histórico recolhido. Preservar seleção explícita e ordenação por atividade real.
- [ ] Implementar catálogo compartilhado no adapter, incluindo pastas não Git e pendentes; UI não recalcula parentesco pelo path.
- [ ] Integrar Workers: indisponível em execução/selecionado continua visível; histórico inativo não polui raízes. Contêiner não é launch target.
- [ ] Escrever testes de PR para branch atual diferente da de criação, siblings, detached, missing e mudança de cwd. Invalidar contexto antigo quando mudar a branch; não herdar badge de outro checkout.
- [ ] Rodar `cargo test -p zeron-ui workers` e `cargo test -p zeron-ui change_request`. Unidade de commit: catálogo + Workers/PR.

## Task 5 — Reorganizar Settings → Projects

**Arquivos:** `crates/ui/src/settings/projects.rs`, `crates/workers-unpeel/src/project_ledger.rs`, `project_git.rs`, `project_identity.rs`.

**Consome:** mesmo `ProjectCatalog` da Task 4. **Produz:** lista de projetos e detalhe de checkouts/histórico.

- [ ] Escrever testes de rows/search: três worktrees do mesmo repo geram uma linha, query por filho encontra pai e filho, nomes iguais em repos distintos não colidem, pendentes são alcançáveis, max de atividade não muda na migração.
- [ ] Implementar lista agregada e seleção explícita de checkout no detalhe, preservando dados antigos de nome/ícone/datas. Não esconder histórico por remover a pasta.
- [ ] Preservar Config, setup, Reveal, Auto Doc e ações no cwd selecionado. Desabilitar somente ações que exigem checkout disponível. Mostrar falha de probe separada de não Git.
- [ ] Acrescentar relatório de reconciliação, associação manual com prévia e desfazer. Contêiner sem principal não oferece launch implícito.
- [ ] Rodar `cargo test -p zeron-workers-unpeel project_ledger` e `cargo test -p zeron-ui settings::projects`. Unidade de commit: Projects agrupado.

## Task 6 — Corrigir ações de ciclo de vida

**Arquivos:** `lib.rs`, `project_identity.rs`, `project_ledger.rs`, `crates/ui/src/workers/project_menu.rs`, `model.rs`, `crates/ui/src/settings/projects.rs`.

**Consome:** ownership/disponibilidade explícitos e catálogo. **Produz:** backend e menus com a mesma capacidade.

- [ ] Escrever testes que antes falham: externo recebe archive sem exclusão física; principal nunca removível como worktree; arquivo em diretório arbitrário sobrevive; erro Git conserva registro; archive preserva sessões; esquecer não ressuscita no polling.
- [ ] Introduzir `archive_checkout` separado dos handlers destrutivos existentes e migrar seus chamadores de UI. Não usar `remove_project` como implementação de archive.
- [ ] Autorizar remoção física somente por ownership persistido mais vínculo Git validado; bloquear Worker ativo, dirty/untracked e dados locais não preservados. Nunca fazer fallback recursivo de filesystem nem apagar branch implicitamente.
- [ ] Sucesso/falha preservam o contexto histórico; após operação longa revalidar estado antes de escrever. Backend deve rejeitar mesmo que chamado sem menu.
- [ ] Atualizar textos, confirmação e capacidades de menu a partir de um único predicado. Testar restore do checkout disponível.
- [ ] Rodar `cargo test -p zeron-workers-unpeel` e testes Workers/Settings da UI. Unidade de commit: ciclo de vida e ações coerentes.

## Task 7 — Comprovação integrada, revisão e aceite

**Arquivos:** relatório novo `docs/verification/2026-09-18-workers-project-identity.md`, fixtures de demo, AGENTS próximos, `CONTEXT.md` e specs afetadas após implementação.

- [ ] Rodar `cargo fmt --all --check`, `cargo test -p zeron-workers-unpeel`, `cargo test --manifest-path third_party/unpeel/crates/Cargo.toml -p unpeel-core controller_host` e `cargo test -p zeron-ui`. Conferir que filtros executaram testes, não zero casos. Registrar falhas preexistentes separadamente; não declarar verde se bloqueado.
- [ ] Fazer revisão independente de identidade/migração/concorrência e actions. Resolver achados antes do aceite. Não distribuir o mesmo arquivo para dois implementadores simultaneamente.
- [ ] Executar `scripts/dev-demo.sh` com estado de demonstração isolado. Confirmar primeiro que o launcher não lê/escreve o perfil real; adaptar a fixture se necessário. GPUI não tem harness de render: teste unitário não comprova layout.
- [ ] Capturar Workers e Projects com um projeto contendo principal, worktree com PR, worktree externo sem PR, checkout removido e histórico. Validar busca, filtro, scroll, seleção, archive, restart e contêiner sem principal.
- [ ] Registrar evidência visual e resultados no relatório, com critério por cenário da tabela abaixo. Só aplicar migração real após os gates e dentro da futura execução autorizada, com dry-run e backup.
- [ ] Atualizar `CONTEXT.md`, AGENTS do adapter/UI e proveniência vendor se alterado; reconciliar os changes relacionados. Validar OpenSpec e arquivar somente após aceite da implementação.
- [ ] Preparar diff/PR no fork com evidências; publicação/release é etapa posterior, sujeita à autorização e guardas do repositório. Este pedido de plano não publica nada.

## Delegação e ordem

Tasks 1–3 são sequenciais sob um responsável pelo contrato/backend. Após congelar `ProjectCatalog`, separar Workers/PR (Task 4 UI) e Settings (Task 5 UI) em agentes/worktrees isolados; o responsável backend concentra alterações compartilhadas em `lib.rs`, `project_identity.rs` e `project_ledger.rs`. Task 6 integra depois dos consumidores. Revisão independente e validação visual na Task 7. Cada entrega inclui diff, testes executados e limites; integração serial pelo responsável principal.

## Matriz mínima de aceite

| Cenário | Evidência exigida |
|---|---|
| Agente cria branch/worktree externamente e registra cwd | Uma raiz de projeto nas duas telas; Worker no cwd certo |
| Branch atual difere da criada; principal fora de main | Label/PR atualizados, identidade inalterada |
| Remove pasta fora do app e reinicia | Mesmo projeto/sessões; checkout marcado indisponível no histórico |
| Pasta antiga sem Git recuperável | Associação pendente explícita, vínculo manual persistente |
| Principal não cadastrado ou indisponível | Um contêiner estável; nenhuma execução no cwd errado |
| Detached / local sem remote / upstream sem origin | Agrupamento local funcional; detached sem PR de branch |
| Clones/forks com mesma URL ou nome | Não fundidos automaticamente |
| Grupo com path igual ao principal | Grupo não rouba a identidade nem vira ledger duplicado |
| Reconcile repetido, escritor concorrente, versão futura | Sem IDs duplicados, perda de sessões/chaves ou escrita de versão desconhecida |
| Archive / forget / remoção gerenciada ou externa | Capacidades e mensagens coerentes; nenhuma perda implícita |
| Settings com muitos filhos e histórico | Uma linha por projeto; todos os checkouts pesquisáveis e alcançáveis |
| UI nativa | Screenshots e interação comprovadas nas duas superfícies |

**Definição de pronto:** todos os cenários comprovados, revisão resolvida, migração idempotente/recuperável e Workers/Projects consistentes. Fechar apenas a aparência da sidebar não completa este plano.

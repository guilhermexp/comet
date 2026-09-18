## Why

Workers externos são registrados por caminho, enquanto sua associação ao repositório é inferida durante a leitura do disco. Quando o worktree desaparece, essa associação pode desaparecer também; Settings → Projects, por contrato atual, apresenta cada caminho como projeto independente. O resultado são branches promovidas a pastas de projetos e registros antigos sem contexto.

## What Changes

- Persistir a identidade do projeto/repositório e a associação de cada checkout, independentemente da existência atual da pasta.
- Unificar a projeção usada por Workers e Projects: um projeto contém seu checkout principal, worktrees e histórico.
- Reconciliar os registros existentes de maneira idempotente, conservando IDs, sessões, nomes, ícones e datas; associações sem evidência ficam explicitamente pendentes.
- Separar disponibilidade, tipo de checkout e propriedade do diretório. Adotar um worktree externo não concede permissão para apagá-lo.
- Manter execução no cwd exato e PR vinculado ao checkout e à branch atuais; metadados históricos não simulam estado Git atual.
- Mudar a ação comum de retirada da lista para arquivamento sem perda de sessões; exclusões continuam explícitas e com escopo visível.
- Alterar a apresentação de Projects de lista plana de caminhos para lista de projetos com checkouts e histórico acessíveis no detalhe.

## Capabilities

### New Capabilities
- `workers-repository-identity`: identidade persistente, descoberta, reconciliação, disponibilidade e ações sobre checkouts.

### Modified Capabilities
- `projects-settings`: agrupamento por projeto, preservação do ledger por checkout e semântica de arquivar/esquecer.
- `workers-sidebar-context`: hierarquia estável, checkouts indisponíveis e contexto de branch/PR.

## Impact

Rust: `crates/workers-unpeel`, projeção em `third_party/unpeel/crates/unpeel-core/src/controller_host.rs`, Workers/Settings/change_requests em `crates/ui`. Metadados locais opcionais no estado existente, sem mudança no CRDT, edge ou runtimes de agentes. Sem nova dependência prevista.

Este change integra as lacunas de `adopt-external-worktrees`, `unify-worktree-identity`, `distinguish-worktrees-from-groups` e `guard-worktree-deletion`; seus checkboxes não substituem evidência do código. Não arquivar essas propostas durante o planejamento. A reconciliação dos seus escopos faz parte da implementação.

## Context

Ver proposta para motivação. Diagnóstico em 18/09/2026:

| Ponto | Comportamento confirmado | Consequência |
|---|---|---|
| `LocalWorkersClient::add_project`, `crates/workers-unpeel/src/lib.rs` | Deduplicação por caminho; cadastro externo sem parentesco persistido | Cada checkout começa como projeto independente |
| `DiskCatalog::capture`, `third_party/unpeel/crates/unpeel-core/src/controller_host.rs` | Infere pai pela pasta `.git` e pelo cadastro do checkout principal | Remover a pasta ou não cadastrar o principal desfaz/impede a associação |
| `project_ledger.rs` + `settings/projects.rs` | Ledger por caminho, apresentação plana deliberada | Até worktrees corretamente agrupados em Workers viram projetos em Settings |
| `workers/project_menu.rs`, `workers/model.rs`, `remove_worktree` | UI usa classificação projetada; remoção exige campo persistido | Worktree externo pode receber ação que o backend rejeita |
| `change_requests.rs` | PR consultado por cwd e branch | Perder contexto do checkout também prejudica PR; PR não pode ser chave do projeto |

A inspeção local encontrou 44 registros de projetos e 44 entradas no ledger, com 18 caminhos inexistentes naquele momento. São evidências datadas, não números a codificar. Um teste temporário com o detector atual reproduziu a perda de `main_repo` e branch depois de `git worktree remove`. Não foi uma suíte completa do Comet.

O exemplo `header-busca` apontava para JK Distribuição e tinha associação recuperável. Outros worktrees vivos apontavam para JK Transmissão, cujo checkout principal não estava cadastrado. Semelhança de nomes não autoriza juntar esses projetos. Para vários caminhos já removidos, nem o registro Git do worktree restava.

## Goals / Non-Goals

**Goals:** identidade estável; uma árvore coerente em Workers e Projects; preservar sessões e cwd; recuperar associações comprováveis; histórico acessível e ações consistentes.

**Non-Goals:** reescrever o motor Git, push/pull, sincronização CRDT, runtimes de agentes, ou juntar clones distintos pelo remote. Não apagar pastas, branches ou sessões durante migração. Não impor que o checkout principal esteja na branch `main`.

## Decisions

### 1. Registro adicional de identidade, mantendo IDs existentes

Escolha: chave local versionada `comet_project_identity` no mesmo estado, com repositórios, associações por ID de projeto existente e associações das entradas legadas por caminho. `repository_id` é UUID estável; `checkout_id` é o ID já usado pelos Workers quando existir. Ledger-only recebe identidade de checkout própria, sem inventar um alvo executável.

O registro contém nome/ícone do projeto, identificador do checkout principal quando conhecido, aliases comprovados de caminho, origem da associação, estado de arquivamento e metadados Git observados por último. Campos novos são opcionais na leitura de snapshots antigos. Grupos organizacionais continuam grupos; não entram no índice de identidade por caminho.

Identidade do repositório, tipo (`primary`, `linked`, `non_git`, `unresolved`), propriedade (`app_managed`, `external`), disponibilidade (`available`, `missing`, `probe_failed`) e branch atual são dimensões distintas. `missing` só resulta de inexistência confirmada; falha de permissão ou Git não equivale a exclusão. Branch observada por último tem timestamp e não vira automaticamente branch atual.

Alternativas: agrupar apenas na UI continuaria perdendo vínculos; substituir todos os IDs e o schema vendorizado ampliaria risco de migração. A extensão aditiva preserva os contratos dos Workers.

### 2. Descoberta Git e registro têm responsabilidades separadas

Descobrir checkout principal/common directory com comandos Git em cwd válido, normalizar caminhos e resolver symlinks; não depender apenas do formato literal `.git/worktrees`. Usar identidade local comprovada do common directory para associar worktrees. Remote é metadado com nome, host e URL; não é chave global nem exige `origin`. Repositório sem remote funciona. Clones e forks não são fundidos automaticamente.

A implementação registra também o fingerprint local do diretório Git comum
(device/inode em Unix). Recriar outro repositório no mesmo caminho causa
conflito em vez de herdar a identidade antiga. O campo é opcional para leitura
de estados anteriores; plataformas sem essa evidência não devem inventá-la.

Descoberta e reconciliação ficam em novo `crates/workers-unpeel/src/project_identity.rs`, com funções puras para planejar alterações. Escrita ocorre nos comandos de cadastro/criação e numa reconciliação de inicialização/background; nunca no render. Git roda fora do lock, com limite de tempo. Antes do commit do plano, reler o estado sob `app_state::edit_at` e revalidar os registros afetados.

A adaptação mínima no `controller_host.rs` lê campos opcionais da identidade persistida e os publica no snapshot. O vendor não depende de `workers-unpeel`. Contrato JSON e testes de compatibilidade são compartilhados por fixtures, sem um segundo algoritmo de migração no vendor.

Se o principal não está registrado, a projeção produz um contêiner de repositório sem cwd executável. Quando o principal for adicionado, vincula-se ao mesmo contêiner. Selecionar um contêiner não lança Worker; é necessário escolher um checkout disponível. Isso evita cadastrar artificialmente um projeto de execução.

### 3. Workers e Projects usam a mesma associação

Workers mantém o formato existente: pasta do projeto, Workers do principal diretamente abaixo, worktrees com ícone de branch e seus Workers. Mudança de branch no principal não muda a identidade do projeto. Checkout indisponível com Worker em execução continua visível, sinalizado; demais checkouts indisponíveis ficam em histórico recolhido sob o projeto. Sessão selecionada permanece alcançável. Sem evidência de associação, usar seção explícita “Associação pendente”, não inventar projeto pai.

Projects lista uma linha por repositório/projeto lógico; o detalhe contém principal, worktrees disponíveis e histórico. Pesquisa por nome, branch ou caminho de qualquer checkout encontra o projeto e identifica o filho correspondente. Ordenação usa a maior atividade real dos filhos, sem alterar timestamps ao reconciliar. Nome/ícone do principal tornam-se padrões do contêiner; personalizações dos filhos são conservadas. Sem principal, usar nome da raiz Git comprovada; basename antigo só é fallback para entradas ainda não resolvidas.

Config, Reveal, Auto Doc e launch continuam ligados ao checkout explicitamente selecionado. Desabilitar operações de filesystem para indisponíveis; não redirecionar silenciosamente à raiz. Preservar scroll, seleção, filtros de projeto e ordenação existente dos Workers.

### 4. PR e remote não definem parentesco

Manter lookup no dispositivo, cwd e branch atual do checkout. A identidade auxilia seleção/associação, sem reutilizar PR do principal no filho. Detached HEAD tem identidade e rótulo de commit, mas não consulta PR como branch. Para checkout ausente, estado é indisponível; eventual PR histórico aparece como último conhecido, nunca confirmação atual. Nenhuma consulta de rede é necessária para construir a árvore.

### 5. Arquivar, esquecer e apagar checkout são ações diferentes

A ação comum “Arquivar checkout” o retira do working set, mantém sessões e ledger e permite restauração quando disponível. Worktree externo não recebe ação de exclusão física. Remoção física de checkout gerenciado exige validação de ownership e vínculo Git reais, rejeita principal e diretórios arbitrários e bloqueia enquanto houver Worker ativo ou alterações locais não preservadas. Sucesso conserva histórico; falha conserva cadastro e informa erro. Não apagar branch implicitamente.

“Esquecer metadados” só se aplica ao histórico inativo; não apaga sessões nem diretório. Registro com sessões retidas continua acessível no histórico de Workers, com metadados mínimos. Impedir reaparecimento por simples polling com supressão persistida no ledger; adicionar explicitamente o checkout novamente restaura sua presença. Exclusão de sessões permanece ação independente já existente, com escopo próprio.

## Risks / Trade-offs

- [Worktrees antigos sem prova de origem] → Associação pendente com ação manual de vincular ao projeto e prévia dos itens afetados; armazenar essa decisão. Não usar apenas prefixo de path, basename ou remote.
- [Estado compartilhado com outros escritores] → Lock e rename atômico existentes, patch apenas de campos conhecidos, revalidação sob lock, testes de escrita concorrente e preservação de chaves desconhecidas.
- [Checkout movido ou caminho reutilizado] → Validar common directory e identidade observada; conflito não reatribui sessões antigas. Relocação sem prova pede associação explícita.
- [Versões antigas continuam criando registros] → Reconciliação incremental e idempotente por entrada; não confiar em um único marcador global “migração feita”.
- [Maior escopo de UI] → Entregar backend antes das duas superfícies e validar visualmente juntas; não liberar somente o agrupamento de Workers.
- [Rollback para binário antigo] → Pode reapresentar lista plana; preservar metadados aditivos e não restaurar snapshot inteiro sobre sessões novas.

## Migration Plan

1. Inventariar estado sob leitura e produzir relatório de associações comprovadas, conflitos e pendentes. Relatório não contém prompts/transcrições.
2. Criar backup local do estado imediatamente anterior à primeira alteração, com acesso restrito. Registrar versão e patches aplicados em journal.

3. Priorizar associação já persistida válida; depois evidência Git viva/common directory e registros de worktrees ainda existentes. Conflitos ficam pendentes. Grupos não elegíveis como raiz Git.
4. Aplicar patches sob o lock existente, preservando IDs, sessões, datas, ícones e chaves desconhecidas. Não chamar `remove_project`, `remove_sessions_in_projects`, `git worktree remove` ou prune durante migração.
5. Reconciliar ledger e working set pela mesma identidade. Registros ausentes continuam histórico, não são descartados. Repetir a operação não produz novos IDs nem novas alterações.
6. Expor relatório e associações pendentes no detalhe de Projects; permitir vinculação manual e desfazer essa vinculação por patch restrito.
7. Habilitar ambas as projeções após testes de compatibilidade. Rollback desabilita a projeção nova e preserva dados; rollback de patches só desfaz campos ainda iguais aos valores escritos, nunca sobrescreve sessões posteriores.


O diagnóstico é read-only (`diagnose_project_identity`). O rollback usa os
snapshots before/after do último journal e compare-and-swap do namespace de
identidade: preserva projetos, sessões e chaves adicionadas depois, mas recusa
se houve outra decisão de identidade após aquele registro. Journals antigos
sem os snapshots não permitem rollback automático.

## Relationship to existing changes

| Change | Tratamento na execução |
|---|---|
| `adopt-external-worktrees` | Reusar detecção/testes válidos; substituir associação apenas transitória pelo contrato durável |
| `unify-worktree-identity` | Conferir código versus checkboxes; incorporar testes pendentes de launch/removal externo |
| `distinguish-worktrees-from-groups` | Preservar distinção e corrigir colisão de caminho de grupo no índice |
| `guard-worktree-deletion` | Preservar guardas do engine e criar evidência equivalente no caminho Workers; não assumir que são o mesmo handler |

Arquivar ou ajustar os changes relacionados somente após comprovação integrada, sem alterar retrospectivamente testes que não foram executados.

## Context

`checkout_lifecycle` já é a única entrada de criação e remoção para Chat e Workers. O preparo continua pendente no journal até setup e `pre-start` terminarem; ambos os clientes podem repeti-lo. Remoção já ocorre sob `CheckoutActionLock`, verifica posse/atividade duas vezes e não força Git.

## Decisions

### D1. Copiar durante o preparo pendente

A cópia roda antes de `.comet`/`.cursor` e de `pre-start` nas duas entradas de preparo. Ela é idempotente: só cria arquivos ausentes. Se falhar, não marca `Prepared`; o próximo pedido repete no checkout existente. Não copiar em adoção de checkout externo. Resolver o principal pelo `git worktree list --porcelain`, em vez de assumir que o projeto pai está no principal.

### D2. Selecionar por Git e `.worktreeinclude`

Enumerar arquivos com `git ls-files --others --ignored --exclude-standard -z` no principal, que respeita `.gitignore` aninhado, global e `info/exclude`. Aplicar padrões gitignore de `.worktreeinclude` com o matcher `ignore`. Rejeitar caminhos absolutos, `..`, links simbólicos, metadados VCS/estado e worktrees aninhados; não seguir links no destino. Reflink por arquivo (clonefile/APFS e FICLONE/Linux), com fallback para cópia comum. Usar `create_new`/sem overwrite também no fallback.

### D3. Avaliar e limpar branch depois da remoção física

Capturar branch e OID antes de remover. Identificar a branch padrão local de forma explícita (`origin/HEAD`, depois `main`/`master` quando existir), sem inferir por nome da feature. Sob o mesmo lock, depois de `git worktree remove`, avaliar integração com critérios conservadores e revalidar que nenhuma outra worktree usa a branch. A deleção usa uma transação `git update-ref --stdin` que verifica o OID da branch e o OID da ref padrão usada como prova; se qualquer uma mudou, conserva a branch. Se Git não puder provar integração ou a operação falhar, manter a branch e registrar aviso. Nunca modificar ref remota. Usar `git merge-tree --write-tree` quando possível para reconhecer squash merge sem alterar o checkout. A verificação por patch-id da ferramenta fica fora deste contrato porque a spec aprovada inicialmente citava os cinco critérios anteriores; caso necessário, uma mudança posterior amplia o algoritmo.

## Risks / Trade-offs

- `.worktreeinclude` pode escolher arquivos grandes; reflink reduz disco nos sistemas suportados, mas fallback pode custar tempo e espaço. A cópia é opt-in.
- Cache copiado é dado ignorado: a remoção permanece bloqueada até limpeza explícita ou `pre-remove` aprovado. Isto evita perda silenciosa.
- Branch cleanup é deliberadamente conservador: uma branch integrada pode permanecer quando não há prova segura, e o aviso explica o motivo.

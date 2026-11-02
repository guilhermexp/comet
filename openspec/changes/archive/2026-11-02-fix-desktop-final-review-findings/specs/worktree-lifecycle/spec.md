## MODIFIED Requirements

### Requirement: Chat e Workers criam worktrees sob a mesma raiz

Todo worktree novo criado pelo app SHALL ficar sob a mesma raiz efetiva para Chat e Workers, com
esta precedência: `ZERON_WORKTREES_DIR` quando definido e não vazio; senão a localização salva em
Settings General quando ela está habilitada com uma pasta absoluta; senão `~/.zeron/worktrees`.
Repositórios distintos com o mesmo basename SHALL receber diretórios distintos. O Chat SHALL manter
sua branch `zeron/<nome>` e o Worker a branch escolhida para ele. Checkouts existentes SHALL NOT ser
movidos por mudança de raiz, nem ter posse presumida por estarem sob a raiz atual.

#### Scenario: Mesma raiz, repositórios distintos
Test: integration — `project_actions` e `m5_repos_diffs_terminals` com raiz temporária.

- **WHEN** Chat e Worker criam worktrees do mesmo repositório e outro repositório homônimo cria o seu
- **THEN** os dois primeiros ficam sob o mesmo diretório de repositório na raiz canônica
- **AND** o terceiro fica em outro diretório
- **AND** nenhum worktree novo aparece em `~/.unpeel/worktrees`

#### Scenario: Precedência ambiente, preferência salva e padrão
Test: integration — `worktree_destination` com HOME, dados e Git isolados.

- **WHEN** a preferência salva está habilitada com uma pasta absoluta
- **THEN** o checkout novo fica sob essa pasta
- **AND WHEN** `ZERON_WORKTREES_DIR` também está definido
- **THEN** ele prevalece
- **AND WHEN** a preferência está desabilitada ou ausente
- **THEN** o checkout novo fica sob `~/.zeron/worktrees` e os checkouts anteriores permanecem onde estão

### Requirement: Saved worktree root drives Chat and Worker creation
The effective root for new Chat worktrees and new Worker checkouts SHALL be derived from the saved preference, preserving the documented environment override precedence and the default root when the preference is disabled or absent. Workers SHALL read the saved preference from the same data directory as the app, including when `ZERON_DATA_DIR` is present but empty.

#### Scenario: Worker and Chat creation use the saved root
Test: integration — temporary Git repositories and synthetic settings.
- **WHEN** a synthetic settings file enables a custom root and a Chat worktree and a Worker checkout are created from a temporary repository
- **THEN** both checkouts are created under the custom root
- **AND** an explicit environment override still takes precedence
- **AND** a disabled preference uses the default root

#### Scenario: One creation resolves the root once
Test: integration — temporary repositories with the preference switched mid-creation.
- **WHEN** the saved root changes after a Worker creation resolved its root but before its ownership journal validates the destination
- **THEN** that checkout is created and owned under the root it resolved
- **AND** the next creation uses the new root

#### Scenario: Workers read the app's data dir on Windows
Test: unit — pure resolver with Windows and Unix inputs.
- **WHEN** `ZERON_DATA_DIR` is not set and `LOCALAPPDATA` is not set or is empty on Windows
- **THEN** Workers read the saved preference from `%USERPROFILE%\AppData\Local\Zeron`, the same directory the app uses, not `~/.zeron`

#### Scenario: Empty data dir override follows the app
Test: unit — pure resolver, and a child process with isolated HOME, working directory and environment.
- **WHEN** `ZERON_DATA_DIR` is present but empty
- **THEN** Workers use the working directory as the data dir, as the app does, before any Windows fallback
- **AND** a saved enabled custom root found there drives the Worker root

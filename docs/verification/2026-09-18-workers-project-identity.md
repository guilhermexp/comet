# Workers / Projects identity — verification

Status: implementação concluída, validada e integrada no checkout solicitado em 18/09/2026. Aceite do usuário e publicação permanecem etapas posteriores.

## Ambiente

- Worktree isolado: `comet-identity-fix`, branch `fix/workers-project-identity`, baseado em `ed3f3a1a` com as alterações locais preexistentes preservadas.
- Target Cargo exclusivo deste checkout.
- Perfil de demonstração: `/tmp/comet-project-identity-native-20260918`, criado por `scripts/seed-project-identity-demo.py`.
- Nenhuma migração executada contra o perfil real nesta etapa.

## Evidência inicial

- Build baseline do adapter: `cargo test -p zeron-workers-unpeel --lib --no-run`, sucesso em 1m04s.
- Validação de remoção com Git real em diretórios temporários: teste inicialmente vermelho para checkout gerenciado válido; após implementação, sete cenários passaram (gerenciado limpo, externo/principal/arbitrário/ativo, dirty/untracked, symlink, principal dentro de diretório gerenciado, arquivos ignorados, detached sem ref durável).
- Revisão independente reproduziu perda de arquivos ignorados pelo Git e identificou commits detached sem referência. Ambos ganharam regressão observada vermelha antes da correção.
- Timeout de subprocesso: regressão real com descendente mantendo pipes falhou em 2s para deadline de 100ms; após grupo de processo e coleta limitada por deadline, passou.
- Teste integrado de archive inicialmente vermelho: estado salvo não aparecia no bootstrap. Corrigir o contrato de serialização/projeção é gate obrigatório antes do aceite.

## Comprovação integrada (antes dos ajustes finais de revisão)

- `UNPEEL_HOME=/tmp/comet-identity-suite-profile cargo test -p zeron-workers-unpeel`: **282 testes passaram**, nenhum falhou. Inclui 15 testes de ações e 5 de contrato de identidade. O teste de archive anteriormente vermelho passou.
- `UNPEEL_HOME=/tmp/comet-identity-suite-profile cargo test -p zeron-ui`: **1.408 testes passaram**, nenhum falhou. `native_preview_focus` foi explicitamente skipped pelo harness; não conta como validação nativa.
- `UNPEEL_HOME=/tmp/comet-identity-vendor-profile cargo test --manifest-path third_party/unpeel/crates/Cargo.toml -p unpeel-core controller_host`: **9 testes passaram**. `cargo test -p unpeel-core` na raiz não suporta dev-dependencies desse pacote externo; o comando do plano foi corrigido para o workspace do vendor.
- Regressão adicional `interrupted_removal_blocks_restart_until_explicit_clean_restore`: passou; reconciliação não autoriza restart após interrupção, restore recusa alterações não preservadas e histórico permanece intacto.

## Observação nativa (perfil isolado)

- App próprio `dev.comet.identity-qa`; nenhum Worker real foi encerrado.
- Workers agrupou o principal e dois checkouts externos sob Comet Demo; repositório sem principal cadastrado ganhou contêiner não executável.
- Projects exibiu Comet Demo uma vez com seletor de três checkouts. Selecionar `external` mudou Path para esse checkout.
- Após `--remove-observed-checkout` do gerador de fixture, Projects reteve o checkout no mesmo grupo, com estado indisponível e ações de configuração/arquivos desabilitadas.
- Workers reteve o filho ausente e sua sessão sob Comet Demo.
- Capturas intermediárias em `/tmp/comet-identity-evidence/`; build final precisa nova captura após os ajustes de pendentes/archive.
- Não foi consultado um PR remoto real durante essa demo: isolamento de cwd/branch é coberto por testes de lógica.

## Comprovação após revisão

- Suites combinadas de UI e adapter: **1.706 testes passaram** (1.413 UI + 293 adapter), zero falhas. Logs locais: `/tmp/comet-identity-final-suites.log`.
- `controller_api`: **31 passaram**, incluindo o guard direto de restart/resume.
- O teste de substituição de repositório no mesmo caminho foi observado vermelho e passou após persistir fingerprint local do common directory.
- Revisões independentes identificaram e corrigiram proteção de ignored files, detached HEAD sem ref, timeout com pipes herdados, namespace inválido, wire IDs, menu de contêiner sintético e archive de sessões encerradas. A revisão final identificou ainda o interleaving de cadastro concorrente; fechamento registrado abaixo.

## Evidência nativa final

- [Workers agrupado, principal ausente e pendentes](assets/workers-project-identity/workers-final.png).
- [Projects agrupado e pendentes separados](assets/workers-project-identity/projects-final.png).
- [Checkout removido permanece no projeto e bloqueia ações](assets/workers-project-identity/projects-missing-checkout.png).
- [Menu de checkout externo oferece archive sem remoção física](assets/workers-project-identity/external-checkout-menu.png).
- [Archive remove o checkout da sidebar ativa](assets/workers-project-identity/workers-archived-checkout.png).
- [Restore preserva configuração, caminho e habilita ações novamente](assets/workers-project-identity/projects-restored-checkout.png).
- [Busca por branch retorna o projeto agregado](assets/workers-project-identity/projects-branch-search.png).

A checagem visual revelou que o menu contextual estava ligado apenas aos ícones
de quick launch. O handler foi adicionado à própria linha, permitindo archive
até para checkouts ausentes; contêineres sintéticos continuam sem ações inválidas.

## Fechamento

- Suite final após os últimos ajustes: **1,710 testes passaram** (1.414 UI + 296 adapter), zero falhas. Comando: `UNPEEL_HOME=/tmp/comet-identity-suite-profile cargo test -p zeron-workers-unpeel -p zeron-ui`.
- Contratos: 8 casos passaram, incluindo ledger-only disponível/ausente, identidade não executável, associação manual e re-add sem duplicação. Snapshot de inputs rejeita cadastro/ledger concorrente, permitindo atualizações somente de sessões.
- Vendor: `cargo test --manifest-path third_party/unpeel/crates/Cargo.toml -p unpeel-core controller_` — **46 passaram**. A suite `app_state` teve **7 casos aprovados** pelo agente de backend.
- `checkout_eligibility`: snapshot e criação bloqueados para conflito, remoção pendente/interrompida, schema futuro, namespace malformado, entrada nula e tipos inválidos de archived/availability. Restart/resume usa o mesmo guard de namespace no host.
- Revisão independente final: nenhum blocker remanescente nos achados revisados; conclusão por leitura, seguida das suites acima.
- `cargo fmt --all --check`, `git diff --check` e `openspec validate stabilize-workers-project-identity --strict`: passaram no checkout solicitado.
- Integração em `/Users/guilhermevarela/Documents/Projetos/SelfHosting/comet` sem colisões de arquivos; arquivos fora do escopo comparados com hashes iniciais permaneceram intactos.
- `cargo build -p zeron` passou nesse checkout; binário em `target/debug/zeron`. Testes executados no worktree isolado de mesmo conteúdo, com target próprio.
- App de QA isolado encerrado. Nenhum commit, release ou instalação da nova build foi realizado. O perfil real não foi migrado durante a validação; a reconciliação será executada ao usar a nova build normal.

As capturas nativas comprovam os fluxos locais. Não houve consulta a PR remoto
real nessa demonstração; isolamento de PR por cwd/branch, detached e missing foi
verificado nas suites de lógica. `native_preview_focus` permaneceu skipped por
seu harness específico e não foi contado como prova visual.

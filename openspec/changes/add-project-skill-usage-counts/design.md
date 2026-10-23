# Design

## Context

Motivação e escopo: `proposal.md`. Contrato observável: `specs/details-sidebar/spec.md`.

### Estado observado

- `crates/ui/src/details_sidebar/session_card.rs:108-168` busca `ListSkills` uma vez por harness/contexto; `ContextSources` guarda `ProjectSkill { name, relative_path }`. O render (`527-641`) tem linhas de 24px, no máximo sete visíveis, nome truncado e clique que abre o arquivo.
- `Project` é o `Space` do registro único (`CONTEXT.md`). `project_chats` em `crates/workers-unpeel/src/project_activity.rs:196-209` filtra `Chat.space_id`, sem excluir arquivados. Worktrees já conservam esse Space (`crates/engine/src/workspace_host.rs:993-1055`). Não usar o filtro mais amplo de repositório da sidebar, que pode abranger outros Projects/devices.
- `turn_steps.rs:166-190,250` reconhece a tool nativa Skill e leituras `skill://`; o teste de prefixo também inclui referências, enquanto `transcript.rs:12649-12670` diferencia o URI de entrada das referências. Não há classificador público pronto para o novo total.
- O Chat Transcript preserva `ReadFile.path` e somente `skill`/`path`/`name` da tool Skill (`crates/doc/src/parts.rs:1019-1090`). O corpo de eval não é preservado para essa finalidade. `MessagePart::Tool.id`, entry id e `continuation_of` permitem reconhecer snapshots/continuações.
- Forks copiam entries anteriores e gravam `MessagePart::Fork` com id `fork:<chat-id>` (`engine/src/rpc.rs:2328-2373`). Somar ingenuamente todos os Chats duplicaria os usos herdados.
- `DocHost::read_local_doc` (`doc_host.rs:1171-1199`) lê handle existente ou snapshot/outbox local sem abrir room. Já `WatchDocMessages` abre o doc e ativa sync; não se deve montar um watcher permanente por Chat frio para contar skills.
- `SessionDoc::read_entries` pula entries irrecuperáveis (`doc/src/schema.rs:525-547`); uma leitura que perdeu entries não pode ser rotulada como cobertura integral.
- Em chat2, o doc local thin conserva mensagens e chamadas de tool; o tail do edge não é fonte para o backfill (`docs/chat2-sync.md`). Para docs de Chats que o device não hospeda, não há marcador persistido suficiente de completude; esse caso permanece parcial.

## Goals / Non-Goals

**Goals:** obter uma projeção reconstruível do histórico existente, computada no host do Project; não manter um contador incrementado pelo render; manter trabalho de I/O e Loro fora da thread da UI e de callbacks de commit.

**Non-Goals:** criar banco/serviço de analytics, ledger paralelo sincronizado, ler Run Journal ou histórico privado de providers, acompanhar CLI Workers ou docs internos de subagentes, inferir uso a partir de shell/eval, modificar os totais atuais de TurnSteps, descobrir skills que não aparecem no catálogo, migrar histórico de skills renomeadas ou manter contribuições de Chats purgados.

## Decisions

### D1 — Project é o Space, não o repositório inteiro

O conjunto elegível é `Chat.space_id == project_id`, incluindo Chats arquivados, Agent-created Chats e contribuições próprias de side Chats. Usar `Space.device_id` para rotear a consulta ao host atual. `cwd`, `checkout_id`, nome do Project e Projects worked não criam associações adicionais. Um worktree vinculado já entra pelo Space. Não agregar Spaces distintos só porque compartilham `repository_id`.

**Alternativa rejeitada:** casar prefixos de pastas ou usar o filtro de repositório da sidebar; ambos somariam conversas de outro Project.

### D2 — Contar tentativas de invocação identificáveis, não intenção ou conformidade

Introduzir em `zeron-proto` uma derivação pura com referência de skill por nome ou entrypoint. Reconhecer:

1. `ToolCall::Unknown`/`Mcp` cujo nome sem namespace é `skill`, usando os identificadores sanitizados `path`, `skill`, `name` (path de entrada explícito tem precedência; identificadores conflitantes não são adivinhados).
2. `ToolCall::ReadFile` de `skill://<nome>` sem subpath, permitindo somente a forma de entrada; `skill://<nome>/SKILL.md` também é entrada explícita, mas `/references/...` não é.
3. `ReadFile` de um `SKILL.md` que corresponda exatamente a um entrypoint do catálogo do projeto após normalizar a raiz do checkout daquele Chat.

Uma chamada identificada conta quando aparece no transcript, mesmo pendente ou posteriormente falha, como o resumo de activity já trata tentativas. Resultado/sucesso não é outra invocação. Mensagens de usuário, inclusive links `zeron-invoke:`, não contam: são solicitação, não evidência de chamada; somá-las ainda duplicaria a posterior tool sem uma identidade causal comum.

Concentrar a nova regra em um helper puro; não aproveitar o prefix match de TurnSteps nem alterar suas contagens existentes nesta change. Reads absolutos de entrypoints passam a ser detectáveis pelo novo total, mas a categoria antiga do resumo permanece intencionalmente inalterada. Não prometer paridade numérica com o resumo de cada turno.

**Limites explícitos:** Codex sem tool/read estruturado, execução opaca em eval/Exec, identificadores antigos descartados e docs internos de subagentes não são recuperáveis. `complete` significa leitura completa do corpus retido, não captura de todo uso possível.

O parser de ReadFile segue a gramática do runtime: `:50`/`:50-`, `:50-200`, `:50+150`, `:-60`, listas por vírgula, `:raw` isolado e raw antes/depois de seletores (`:2-4:raw`, `:raw:2-4`, `:17:raw`, `:17-:raw`). Remove somente sufixos válidos de recursos de entrada; um nome literal com dois-pontos no catálogo tem precedência sobre a interpretação de range.

### D3 — Identidade pelo entrypoint relativo, nomes apenas quando inequívocos

A UI já recebe o catálogo completo de `ListSkills`; conservar uma projeção mínima dele antes de filtrar as linhas. Proposta de tipo em `crates/proto/src/skill_usage.rs`:

- `SkillUsageCatalogEntry { name: String, relative_path: Option<String> }`: `Some` identifica uma skill do projeto relativa ao checkout; `None` conserva o nome de uma skill externa para detectar colisão, sem revelar seu path.
- `ProjectSkillUsageCount { relative_path: String, count: u64 }`.

A lista visual continua usando seu `ProjectSkill`. A mesma regra de pertencimento já aplicada por `project_skills` deve produzir a projeção do catálogo, sem uma segunda convenção de filtro. Normalizar separadores e componentes de paths, sem I/O de canonicalização ou resolução de links remotos na máquina da UI.

Path explícito é comparado ao entrypoint relativo usando o cwd do Chat que registrou a chamada (e a raiz do Space para paths que apontem explicitamente ao checkout principal). Assim main e worktree compartilham `.agents/skills/x/SKILL.md`. URI/nome simples só casa se o catálogo completo tiver uma identidade candidata inequívoca, considerando também nomes externos. Duas skills de mesmo nome nunca se fundem apenas pelo label. Identidade explícita fora das raízes elegíveis não vira skill do projeto.

**Alternativa rejeitada:** um mapa só por nome; ele confunde plugins, skills globais e skills distintas.

### D4 — Deduplicação e histórico reconstruível

Projetar contribuições por Chat a partir de entries sanitizadas, substituindo a contribuição anterior quando o Chat mudar. Chave de chamada: `(chat_id, continuation_of.unwrap_or(entry.id), tool.id)`. Não deduplicar por nome/turno: duas chamadas reais contam duas vezes. Uma mudança de preview/resultado com a mesma chave substitui a identidade anterior, sem incremento acumulativo; identidade ainda incompleta só passa a contar quando identificável.

Para um side Chat, localizar seu marcador próprio `fork:<chat-id>` e ignorar o prefixo copiado anterior a ele. Em forks de forks, não usar o primeiro marcador herdado. Child Chat sem prefixo copiado conta todas as próprias entries. Se a estrutura declarar um prefixo herdado mas não permitir identificar uma fronteira confiável, marcar a contribuição como parcial, não adivinhar. Purga/remoção de um Chat elimina sua contribuição; arquivamento não elimina.

**Alternativa rejeitada:** contador monotônico gravado em outro documento; exigiria migração e deduplicação de sincronização sem necessidade.

### D5 — Snapshot/stream no host, backfill sem rooms por Chat

Adicionar `WatchProjectSkillUsage`, stream `forwardable`, no registro `rpc_methods!`. Tipos novos em `zeron-proto`, handler em `engine/src/rpc.rs`, serviço/projeção em um novo `engine/src/skill_usage.rs`.

Contrato proposto:

- Params `WatchProjectSkillUsageParams { project_id, catalog: Vec<SkillUsageCatalogEntry>, target_device_id? }`.
- Frame `ProjectSkillUsageSnapshot { project_id, host_device_id, counts: Vec<ProjectSkillUsageCount>, coverage: SkillUsageCoverage, chats_total, chats_counted }`.
- `SkillUsageCoverage`: `Loading`, `Complete`, `Partial`, `Unavailable`; wire em camelCase. Tipos e método aditivos, sem alterar containers CRDT nem payloads existentes.

O host verifica o Project no registro e a posse pelo device local. Um catálogo recebido só seleciona identidades: nunca autoriza ler arquivos, Run Journal ou conversas fora do Project. Identidades duplicadas no catálogo são normalizadas uma vez.

No primeiro assinante, ler `workspace.read_chats`, filtrar por Space e reconstruir cada contribuição do Chat Transcript local com concorrência limitada (inicialmente uma leitura por vez), via `spawn_blocking`. Reutilizar handles já abertos e a leitura fria `read_local_doc`, sem chamar `open`, sem pin de room e sem `WatchDocMessages` por Chat. Um doc ausente, ilegível, com causalidade pendente ou com entries puladas resulta em `Partial`, nunca em lista vazia confirmada. Um Chat cujo host não seja este device também permanece parcial, pois um snapshot de viewer não prova completude. Usar o snapshot e outbox da implementação chat2 existente, sem ignorar updates locais ainda não compactados.

Após o backfill, assinar `workspace.watch_chats()` para inclusão, remoção, reatribuição e atualização de atividade, invalidando apenas a contribuição afetada. Complementar com `sessions.watch_sessions()` nas transições de Working para estado final/ocioso: `note_message` ignora texto vazio, portanto só `last_message_at` perderia turnos compostos apenas de tools. O contrato é atualizar no encerramento/publicação do segmento ou turno, sem exigir reprocessamento por token. Um uso em outro Chat do mesmo Project deve atualizar o contador ao final do turno sem abrir esse Chat. Retomar uma assinatura sempre reconcilia o corpus atual; o cache é uma otimização por assinatura, não fonte de verdade ou cache eterno por timestamp.

O stream entrega frames completos e canceláveis. Concorrência entre primeiro backfill, atualizações e encerramento não pode sobrescrever um snapshot mais recente com um cálculo antigo. Obter os receivers antes de iniciar a varredura e processar seus estados pendentes ao final evita perder chamadas durante o backfill, inclusive turnos só de tools sem mudança no registro de Chats. Não criar loop de polling sobre todos os transcripts ou bus global de eventos só para esta feature.

**Ajuste interno F2/F3/F5 autorizado na revisão de 2026-10-23:** a criação de uma row nova hospedada neste device persiste um SessionDoc vazio antes do upsert, com INSERT OR IGNORE no store (nunca substituir snapshot recuperado). Isso prova o zero legítimo após restart, sem reinterpretar história ausente. O caminho antigo `read_local_doc` conserva o fast path cached para replay; a projeção usa `read_local_doc_with_coverage`, cuja falha de outbox continua conservadora. Para sincronização sem mudança de metadados, o DocsStore expõe notificações efêmeras de snapshot/outbox a consumidores ativos; DocHost sinaliza imports e CaughtUp normais. O stream assina antes da varredura, invalida somente contribuições afetadas, recupera lag e coalesce gravações durante Working até o encerramento. Nenhum ledger, polling de transcripts, alteração de CRDT ou nova superfície RPC é introduzido. Uma conexão SQLite externa não emite sinais neste store; o caminho normal da engine compartilha o mesmo Arc<DocsStore>.

**Complemento F2 autorizado no follow-up:** o Mutate(createChat) normal do viewer usa o seam aditivo `CreateChatOnHost` no relay configurado para o owner do Space. O owner valida a operação/posse e chama a criação legítima existente (snapshot INSERT OR IGNORE antes da row), devolvendo o Chat canônico para import no viewer. Nenhum marker de vazio é inferido de uma row sincronizada. UnknownMethod de owner antigo conserva o caminho legado sem alegar cobertura; falhas de transporte em rotas configuradas retornam erro retentável antes de anunciar sucesso. Sem relay configurado, permanece a mutação de registro existente (perfil local/legado). Não há novo ledger/CRDT ou mudança de ownership; o Mutate genérico permanece local.

**Alternativa rejeitada:** abrir todas as conversas no frontend; fixa docs no LRU, aumenta sync e perde histórico frio/remoto. Se o host não tiver parte do histórico, usar `Partial` até a sincronização normal o disponibilizar, sem iniciar um download em massa específico deste contador.

### D6 — UI discreta e cobertura honesta

Estado da contagem separado de `ContextSources`, com chave `(project_id, host_device_id, assinatura normalizada do catálogo)` e geração para descartar respostas atrasadas. Trocar entre Chats do mesmo Project com catálogo equivalente não reinicia a contagem. Trocar harness/catálogo revalida o casamento, mas nunca restringe o conjunto de Chats ao harness atual. Reutilizar a assinatura scoped do RPC e o padrão de cancelamento de `details_sidebar/worked_project_diffs.rs`. Host antigo, offline ou método desconhecido é `Unavailable`, não `x0`.

EOF curto pode encerrar o receiver encaminhado sem alterar presença (grace de 70s), Connected ou EngineHandle. A recuperação fica no task scoped desta assinatura: subscribe_checked preserva UnknownMethod e encerra recovery para host permanentemente não suportado; falhas transitórias/EOF esperam backoff 1/2/4/8/16/30s, teto 30s. Ocultar/mudar Project/catálogo/host descarta task, timer e RPC. Não se usa redraw para disparar tentativas nem framework de rede novo.

Na lista, preservar 24px/linha, sete linhas e clique existente. Nome com `flex_1/min_w_0/truncate`, contador com `flex_shrink_0`, tamanho de 12px, `theme.text_muted`; sem fundo de badge, ícone, nova animação ou cor de status para zero. O header permanece `29 skills`, não total de usos.

Copy de UI em inglês, conforme o painel existente:

| Estado | Sufixo | Tooltip |
|---|---|---|
| Complete | `xN` | `N recorded invocations in this project. Includes failed attempts; only identifiable skill activity is counted.` |
| Loading | `…` | `Loading project skill history…` |
| Partial | `—` | `Project skill history is incomplete. A total cannot be confirmed.` |
| Unavailable | `—` | `Project skill history is unavailable on the host device.` |

Preservar nome/abertura no hover da row; o tooltip do contador explica a métrica. Não mostrar o último total de um Project anterior ou um número antigo como total atual confirmado.

## Risks / Trade-offs

- **R1 — Sinal incompleto por provider** → contar somente evidência estruturada preservada; não extrair comandos/código nem prometer total absoluto. `x0` significa zero detectado no corpus coberto.
- **R2 — Histórico frio/re-homing incompleto** → consultar o host atual, carregar sem abrir rooms, preservar `Partial` quando faltar histórico e reconciliar na nova assinatura/atualização normal.
- **R3 — Duplicação em forks/continuações** → fronteira própria do fork e chaves de tool, com fixtures específicas; não deduplicar só por tool id global.
- **R4 — Corpus grande** → backfill serial em background, contribuições por Chat e atualização dirigida, sem recontar o Project inteiro a cada render/token.
- **R5 — QA Linux não prova aparência/AppKit macOS** → testes de projeção e smoke do serviço na box; captura nativa apenas em ambiente com isolamento efetivo de providers. Não lançar cópia local do app com a receita de QA proibida pelo projeto. Declarar `notProven` se a aparência não for observada; não chamar teste gpui de prova visual nativa.

## Migration Plan

1. Aprovar estes artefatos. Nenhuma mudança de código ou execução na box faz parte da criação das specs.
2. Na aplicação, descobrir estado/projeto/presets reais da `agents-us`, selecionar o executor com o usuário e criar branch/worktree isolada na box, com espelho local só para conferência. Transportar esta change como fonte do brief; não iniciar da `main` mutável nem misturar trabalho alheio.
3. Implementar com provas RED/GREEN e smoke na box; Cargo direto lá reutiliza o target, sem o wrapper destrutivo do Mac. Atualizar docs donas de `proto`, `engine`, `rpc`, `ui` e o changelog do fork após a prova. Não mudar o schema do transcript nem a política de sanitização.
4. Após conferir a entrega e o commit exato, arquivar a change pelo CLI, sincronizar AGENTS e integrar na `main` do fork. Bump da versão somente na etapa de publicação, escolhendo a próxima versão a partir do estado real da main/releases; não fixar um número nesta spec.
5. Criar a tag `v<workspace-version>` no commit integrado e acompanhar `.github/workflows/release.yml` em `guilhermexp/comet`. O build macOS roda no runner Apple do GitHub Actions, não na box Linux. `workflow_dispatch` sozinho só gera artifacts e não publica uma release.
6. Conferir release latest, DMG, `macos-arm64-app.tar.gz`, `manifest.json`, versão e SHA-256; o app instalado consulta esse feed. Detecção, download e reinício seguem os gates existentes de atualização; não prometer atualização instantânea durante trabalho ativo nem forçar encerrar este Chat.
7. Se houver defeito após integração, fazer correção/revert por novo commit e, se já publicada, uma release de versão maior; não reescrever main nem mover tag publicada. Não há migração de dados para desfazer. Encerrar só as sessões da tarefa e remover worktrees/branches integradas e targets/caches da tarefa pelo fluxo de cleanup.

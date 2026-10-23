# Tasks

> Plano de implementação da change `add-project-skill-usage-counts`. Esta criação de specs não executa os itens abaixo. Aplicar somente após revisão dos artefatos e novo pedido do usuário.

**Goal:** mostrar `xN` por skill no Context sources, somando invocações detectadas nos Chats retidos do mesmo Project.

**Architecture:** derivação pura de identidade em `zeron-proto`; projeção reconstruível e stream no host do Space; UI consome contagens e cobertura separadamente do catálogo. Backfill local sem abrir rooms frias, atualização dirigida ao fim do turno e nenhuma mudança no CRDT ou em TurnSteps.

**Tech Stack:** Rust workspace, tokio, Loro/SQLite existentes, RPC tipado e GPUI. Sem dependência nova.

**Spec:** `specs/details-sidebar/spec.md`; decisões/interfaces em `design.md` D1–D6.

## Global Constraints

- Código e checks na `agents-us`, em branch/worktree isolada. Mac somente para specs, conferência e operações de publicação previstas; não compilar o produto localmente.
- Project é `Chat.space_id`, inclusive arquivados/worktrees, nunca nome, prefixo, `repository_id` ou Projects worked. Não incluir CLI Workers nem docs internos de subagentes.
- Métrica é tentativa identificável registrada, inclusive falha/pendente; não intenção do usuário, menção textual ou confirmação de conformidade. Não ler Run Journal nem histórico privado de providers.
- `x0` só com corpus integral; `…` loading e `—` parcial/indisponível. Preservar 24px por row, sete rows visíveis, ordem, scroll e abertura do arquivo.
- Antes de editar exports, consultar referências com LSP quando disponível. Ler a cadeia DOX dos alvos. Não alterar código/configuração do harness.
- Cargo direto na box, sem `scripts/cargo-verify.py` (o wrapper destrutivo é para o Mac). Não repetir baterias pesadas nem iniciar `no-mistakes` sem pedido explícito.
- Não publicar até aceitar as provas do commit final. Push somente no fork, sem force. `gh` sempre com `-R guilhermexp/comet`.

## Review Focus

- **R1:** providers/identidades com sinal incompleto → não contar eval, Exec, referências, links do composer ou nome ambíguo; coberto em 2.1.
- **R2:** snapshots ausentes, malformados e Chats cujo host não é local → parcial, nunca `x0`; coberto em 3.1 e 3.5.
- **R3:** fork de fork, continuações e replay → prefixos herdados não contam, chamadas novas continuam distintas; coberto em 3.1.
- **R4:** turno só de tools, atualização durante backfill e cancelamento → nenhum uso perdido nem snapshot antigo publicado; coberto em 3.4 e 4.1.
- **R5:** mudança de Project/host/catálogo e nomes longos → não exibir contagem do contexto antigo nem ocultar o contador; coberto em 4.1 e 5.2.

## 1. Preparar execução isolada na box

- [x] 1.1 Após aprovação deste plano, descobrir status, sessões, locations, runs e presets reais de `agents-us`; identificar o checkout exato do Comet e pedir somente a escolha de preset/modelo ainda não feita pelo usuário. Não assumir o preferido como autorizado.
- [x] 1.2 Criar a branch/worktree da tarefa a partir da base atual conferida, com espelho local na mesma revisão; copiar esta change/brief para a box sem absorver mudanças alheias. Executar preflight com o executor escolhido, Cargo e demais binários necessários; iniciar somente com preflight OK. O brief inclui o fluxo RED/GREEN, releitura de ajustes, limite de escopo, provas exigidas e proibição de merge/tag/release pelo executor antes do aceite.

## 2. Identificar invocações e catálogo sem ambiguidade

**Files:** criar `crates/proto/src/skill_usage.rs`; registrar em `crates/proto/src/lib.rs`; integração da projeção existente em `crates/ui/src/details_sidebar/session_card.rs`. Não alterar `turn_steps.rs` ou sanitizadores.

**Interfaces propostas:** `SkillInvocationRef<'a> { Name(&'a str), EntryPath(&'a str) }`; `skill_invocation_reference(call: &ToolCall) -> Option<SkillInvocationRef<'_>>`; `project_skill_catalog(skills: &[Skill], project_root: &Path) -> Vec<SkillUsageCatalogEntry>`. DTOs `SkillUsageCatalogEntry`, `ProjectSkillUsageCount`, `SkillUsageCoverage`, `WatchProjectSkillUsageParams`, `ProjectSkillUsageSnapshot` conforme design D3/D5. Casamento de referência com catálogo deve devolver somente a identidade relativa inequívoca, sem I/O.

- [x] 2.1 Adicionar testes RED co-located em `skill_usage.rs`: tool `Skill` Unknown/Mcp com `skill`/`name`/`path`, namespace, identificador vazio/conflitante; `skill://brainstorming`, `skill://brainstorming/SKILL.md` e entrypoint explícito elegíveis; `skill://brainstorming/references/guide.md`, Exec e eval inelegíveis. Nome igual global/projeto não pode casar por nome; path explícito resolve somente o entrypoint correto. Incluir nomes Unicode e paths com espaços. Rodar `cargo test --locked -p zeron-proto skill_usage` na box e guardar a falha esperada antes de implementar.
- [x] 2.2 Implementar os helpers puros e DTOs em `skill_usage.rs`, reutilizando a regra de pertencimento atual de `project_skills`, preservando nomes externos apenas para colisão e normalizando o catálogo uma vez. Rodar o mesmo filtro para GREEN, atualizar `crates/proto/AGENTS.md` com o contrato exercitado e registrar o commit; não contar menções ou mensagens de usuário e não modificar o resumo antigo de activities.

## 3. Agregar histórico e expor stream no host do Project

**Files:** criar `crates/engine/src/skill_usage.rs` e `crates/engine/tests/project_skill_usage.rs`; registrar módulo em `crates/engine/src/lib.rs`; integrar `crates/engine/src/rpc.rs`, `crates/rpc/src/method.rs`. Usar `DocHost::read_local_doc`, `WorkspaceHost::watch_chats`, `SessionsEngine::watch_sessions` existentes; só ampliar `doc_host.rs` se necessário para evidenciar completude sem alterar seu contrato de leitura.

**Interfaces propostas:** `chat_skill_usage(chat: &Chat, space: &Space, entries: &[SessionMessageEntry], catalog: &[SkillUsageCatalogEntry]) -> ChatSkillUsage`, tipo interno com contribuições deduplicadas e cobertura; novo stream `WatchProjectSkillUsage` recebe os DTOs da seção 2 e publica `ProjectSkillUsageSnapshot` completo a cada mudança visível.

- [x] 3.1 Adicionar testes RED puros: Project A tem dois usos num Chat e três em outro arquivado, total cinco; Project B, Chat sem Space, Worker e Projects worked não entram. Main/worktree do mesmo Space casam pelo entrypoint relativo. Duplicar snapshot/resultado/replay mantém um uso; segunda tool id da mesma skill no turno conta dois; continuação mantém uma chamada. Fork e fork de fork contam só após o marcador próprio. Arquivar preserva e purgar remove a contribuição. Rodar `cargo test --locked -p zeron-engine --lib skill_usage` e guardar RED.
- [x] 3.2 Implementar `chat_skill_usage` e a composição por Project em `engine/src/skill_usage.rs`, com contribuição substituída por Chat em vez de incremento monotônico. Rodar o mesmo filtro para GREEN.
- [x] 3.3 Adicionar RED de integração em `project_skill_usage.rs`: reconstrução de Chat frio após restart usando snapshot/outbox (inclusive epoch chat2 e histórico anterior), sem `DocHost::open`/aumento de handles/joins; doc ausente, entrada irrecuperável, causalidade pendente ou Chat de outro host geram Partial. Rodar `cargo test --locked -p zeron-engine --test project_skill_usage` e guardar RED. Implementar backfill serial em `spawn_blocking`, distinguindo zero confirmado de corpus incompleto; verificar GREEN.
- [x] 3.4 Adicionar RED de atualização/cancelamento: um Chat não selecionado registra uso e o frame vai de cinco para seis; outro Project não altera o frame; turno só de tools sem texto também atualiza ao encerrar; uso ocorrido durante o backfill não desaparece; receiver cancelado encerra o trabalho. Implementar com `watch_chats()` mais `watch_sessions()` para o caso sem texto, assinados antes da varredura, sem polling global ou leitura por token. Verificar GREEN no filtro de integração.
- [x] 3.5 Registrar `WatchProjectSkillUsage` como stream forwardable no mesmo commit do handler. Provar via RPC real em `project_skill_usage.rs` e/ou casos focados em `crates/engine/tests/device_routing.rs`: roteamento ao device do Space, recusa no host incorreto e isolamento dos Chats por Project, mesmo com catálogo arbitrário. Host offline/método desconhecido permanece indisponível, não zero. Rodar `cargo test --locked -p zeron-engine --test project_skill_usage` e os casos novos de `device_routing` pelo nome exato; guardar saída e SHA e atualizar os contratos/matrizes em `crates/engine/AGENTS.md` e `crates/rpc/AGENTS.md`.

## 4. Mostrar contador compacto no Context sources

**Files:** `crates/ui/src/details_sidebar/session_card.rs`, `view.rs`; criar `crates/ui/src/details_sidebar/skill_usage.rs` para reducer/lifecycle da assinatura, seguindo `worked_project_diffs.rs`; registrar como filho no módulo existente.

**Interfaces:** consumir catálogo/DTOs da seção 2 e stream da seção 3; estado separado da lista. Chave de assinatura: Project, host e catálogo normalizado, não Chat selecionado. Um reducer puro transforma snapshot/erro em label e tooltip conforme design D6.

- [x] 4.1 Adicionar RED co-located para label/estado e lifecycle: Complete(12) → `x12`, Complete(0) → `x0`, Loading → `…`, Partial/Unavailable → `—`; falha/unknown method limpa confirmação anterior. Resposta atrasada de A após selecionar B é ignorada; alternar Chats do mesmo Project/catálogo reutiliza assinatura; mudar host/catálogo invalida; ocultar Details cancela. Usar fixture GPUI/RPC existente quando necessário para provar transições, sem teste tautológico de encaminhamento. Rodar `cargo test --locked -p zeron-ui --lib project_skill_usage` e guardar RED.
- [x] 4.2 Implementar o estado e assinatura scoped, preservando a lista mesmo se a contagem falhar. Renderizar o sufixo sem fundo de badge à direita, com `flex_shrink_0`, 12px e `theme.text_muted`, reservando o espaço pelo truncamento do nome; manter 24px/row, sete rows, header de quantidade de skills e clique existente. Seguir `~/.agents/skills/impeccable/SKILL.md` quando disponível; na box sem a instalação, o brief deve levar as regras pertinentes e D6, sem instalar ou editar skills. Rodar o filtro para GREEN e os testes existentes de `session_card`, atualizar o contrato e matriz em `crates/ui/AGENTS.md`.

## 5. Provar entrega, integrar e publicar após aceite

- [x] 5.1 Na box, fazer um smoke executável temporário em `.tmp/` usando engine/RPC reais e dados isolados: dois Chats do Project com dois + três usos → cinco; arquivar/reabrir e reiniciar mantém cinco; fork não duplica; nova chamada em outro Chat → seis; Project distinto continua isolado; histórico ausente não produz `x0`. Registrar comando, saída e SHA e remover o smoke após a prova. Consolidar uma rodada final dos filtros afetados e `cargo fmt --all -- --check`; não repetir o mesmo gate em cada checkout.
- [ ] 5.2 Verificar a superfície renderizada com nome longo, `x0`, `x12`, contador de vários dígitos, estados de cobertura, clique/scroll, larguras estreita/normal e temas claro/escuro. Só lançar fixture/app em ambiente com isolamento real das configurações de todos os providers. Não repetir o launcher de QA proibido no Mac; se não houver isolamento, registrar aparência nativa como `notProven` e não apresentar unit tests/Linux como prova AppKit. Captura e evidência nativa pendentes precisam ser explicitadas antes do aceite de entrega.
- [ ] 5.3 Após as provas, registrar o comportamento e os limites em `fork_changelog.md`; validar a change com `openspec validate add-project-skill-usage-counts --strict`. Conferir o diff/commits trazidos da box no espelho, resolver somente achados deste escopo e apresentar evidências/limites ao usuário antes de integrar.

## Workflow follow-up

- Com as tarefas acima concluídas e a entrega aceita, arquivar a change pelo CLI (atualizando a spec canônica), sincronizar os AGENTS afetados e integrar a branch na `main` do fork sem force. Não absorver alterações de outras changes abertas.
- Selecionar a próxima versão a partir da main/releases reais, atualizar versão/lockfile conforme as convenções do repo e criar `v<workspace-version>` no commit integrado. Push no fork e acompanhar o run exato de `.github/workflows/release.yml` com `gh ... -R guilhermexp/comet`; confirmar sucesso de build macOS e publish. Não substituir por `workflow_dispatch`, que não publica release.
- Conferir latest, versão do manifest, DMG e app tarball arm64 e seus SHA-256, ligando-os ao commit/tag publicado. Acompanhar a detecção pelo app já instalado sem forçar reinício enquanto houver trabalho ativo; declarar separadamente release publicada e atualização realmente observada.
- Encerrar só a sessão da tarefa e remover branch/worktree integrada e builds/caches da tarefa na box e no espelho pelo fluxo de cleanup, preservando alterações e sessões alheias.

## Evidência da implementação na box (2026-10-23)

- Preparação 1.1/1.2 aceita pelo orquestrador em A1; fallback de navegação sem trail autorizado em A2. Nenhuma instalação ou mudança de modelo/preset.
- REDs inspecionáveis: commits `4545cb7a1` (proto), `c5d955de1` (engine/RPC), `dc96c2458` (UI); logs completos em `.tmp/evidence/project-skill-usage/`.
- Gate focado: 24 testes distintos passaram em `final-gate-2.log`. O formatter dessa rodada apontou somente uma quebra de chamada em teste; correção manual confirmada por `fmt-check-final.log` (exit 0). Novas fixtures explícitas de prosa/solicitação e subagente inline com 99 usos foram confirmadas por `engine-corpus-boundaries-final.log` (4 unit + 6 integration, exit 0).
- Smoke final com engine/RPC reais e área temporária: 2+3=5; reconexão/restart/fork/fork de fork mantêm 5; uso em outro Chat passa a 6; outro Project fica em 9; histórico ausente é Partial. `smoke-final.log` guarda comando, fonte/hash e saída. Fonte temporária executável e binário removidos após a prova; nenhum provider/Worker lançado.
- Extensão mínima autorizada explicitamente pelo usuário: uma chamada de cancelamento em `worked_project_diffs.rs` no hook de ocultação já existente.
- 5.2 permanece aberta: GPUI/RPC comprova geometria, scroll/colapso, gesto de OpenFile e cancelamento em fixtures; aparência, hover/tooltip e abertura nativos AppKit são **notProven**. Nenhuma cópia QA do app foi lançada contra configurações reais de providers.
- 5.3 tem DOX/changelog atualizados e validação OpenSpec estrita válida; conferência do espelho e aceite final continuam com o orquestrador. Change não arquivada; sem push, merge, tag ou release. SHA final e relatório novo ficam em `.tmp/evidence/project-skill-usage/revision.txt` e `.tmp/reports/`.

## Revisão F1–F5 na box (2026-10-23)

- A entrega anterior `42fb2d90` não foi aceita; seus relatórios/logs permanecem preservados. Correções internas autorizadas em `.tmp/briefs/project-skill-usage-review.md`, com A1/A2 relidos por grupo.
- F1: GPUI/RPC reproduziu EOF sem rearmar; recuperação por presença/conectividade e offline inicial passam sem duplicar watch estável ou retry por redraw. Geração e cancelamento permanecem cobertos.
- F2: CreateChat RPC reproduziu Partial no Project com cinco usos. Snapshot vazio de nascimento é durável e insert-if-absent; restart/retry/recuperação anterior à row não substituem histórico. Ausência real/corrupção seguem Partial.
- F3: import normal após scan reproduziu total obsoleto. Imports, CaughtUp e persistência/outbox local invalidam a contribuição afetada, com notificações efêmeras, coalescidas durante Working. Regressões incluem Partial recuperado e corrida durante backfill sem alterar metadados/Sessions.
- F4: ranges de entrada antes não casavam; seletores válidos (incluindo raw/lista), nomes literais com dois-pontos, colisões e drives Windows estão cobertos sem alterar sanitizadores/TurnSteps.
- F5: outbox malformado impedia replay cached; o caminho antigo volta a servir o mesmo Arc/entries e a leitura de cobertura continua falhando conservadoramente.
- RED/GREEN e gate/smoke da revisão em `.tmp/evidence/project-skill-usage-review/`; SHA e mapa completo no relatório novo timestampado em `.tmp/reports/`. O gate final cobre todos os filtros afetados; 5.2 nativo e 5.3 aceite do orquestrador continuam abertos, sem archive ou publicação.

## Follow-up das variantes restantes F1/F2/F4 (2026-10-23)

- F3/F5 aceitos pelo orquestrador e preservados; suas REDs anteriores não foram repetidas.
- F1: regressão GPUI/RPC fecha receiver mantendo presença online/Connected e o mesmo EngineHandle, e recupera x43 por assinatura nova após atraso. Recovery usa timer scoped/backoff, cessa em UnknownMethod e é cancelado em hide/mudança de catálogo; ausência de retry por redraw/host estável continua coberta.
- F2: duas engines, RegistryRoom mock WebSocket e relay do device reproduzem CreateChat de A em Project de B → Partial(5). Correção confirma o nascimento durável em B pelo seam aditivo CreateChatOnHost, sem abrir doc/room ou mudar owner; assinaturas antiga/nova mantêm Complete(5) e restart mantém a prova. Retry de row antiga sem snapshot/corrupção continua Partial; owner antigo mantém protocolo compatível e não ganha zero inventado.
- F4: o novo teste cobre todas as formas válidas da gramática fornecida, nos cinco formatos de recurso (incluindo Windows/URI), mantendo referências/malformed excluídos e nomes literais com dois-pontos. A antiga negativa incorreta de `:1-` foi removida.
- Logs novos: `.tmp/evidence/project-skill-usage-review-followup/`. Relatórios/provas anteriores preservados. Gate relevante, smoke e SHA final serão mapeados no relatório timestampado novo. 5.2 nativo e 5.3 aceite permanecem abertos; sem archive/publicação.

## Correção restrita da regressão CreateChat offline (2026-10-23)

- F1/F3/F4/F5 fechados pelo orquestrador e preservados sem reauditoria. Somente o roteamento de CreateChat, seus testes e contratos relacionados foram ajustados.
- RED: viewer real com LinkCache/Transport offline não criava row; retry de row existente exigia owner; probe sem resposta bloqueava criação. GREEN: criação local/offline preserva owner sem snapshot inventado; retry mantém row/bytes e não consulta link; oportunidade online tem teto de 1s e falha de validação não vira sucesso.
- O teste online de duas engines/registry/relay/restart/Complete(5) e o caminho legado permanecem obrigatórios no gate focado. Não repetir baterias de proto/UI nem REDs de achados fechados.
- Cobertura offline/legada pode permanecer Unavailable/Partial até histórico normal disponível; não há fila/framework/ledger novo nem exigência de telemetria para criar Chat.
- Evidências novas em `.tmp/evidence/project-skill-usage-create-offline/`, com SHA e relatório timestampado próprio; provas anteriores preservadas. 5.2 nativo e 5.3 aceite do orquestrador continuam abertos, sem publicação/archive.

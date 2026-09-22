# Trajectory Repair Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `executing-plans` to implement this plan task-by-task. Delegate bounded implementation and independent review under the repository's development cycle, with one writer per shared file. Steps use checkbox (`- [ ]`) syntax for tracking. The canonical completion ledger is the OpenSpec `tasks.md`; this document defines the execution recipe, not a second progress ledger.

**Goal:** Tornar a Trajectory utilizável para descobrir o que aconteceu, inspecionar uma operação completa e distinguir fatos observados de dados ausentes, inclusive no histórico existente.

**Architecture:** Preservar eventos e identidades; derivar operações correlacionadas em `zeron-proto`, consultadas por índice local na engine. Capturar fronteiras, timing, uso e schemas antes da normalização perder informação, mantendo fonte completa opt-in separada. A surface continua dona de seleção, input, scroll, watch e Reveal efêmero.

**Tech Stack:** Rust edition 2024, gpui vendorizado, tokio, SQLite/WAL, RPC local e harness OMP existentes; nenhuma dependência nova planejada.

**Spec:** `openspec/changes/repair-trajectory-observability-fidelity/specs/chat-trajectory-preview/spec.md`; decisões em `design.md` da mesma change; acompanhamento em `tasks.md`.

## Global Constraints

- Este pedido autoriza produzir o plano. Implementação, commits, publicação e reinício do app host não foram executados por este documento.
- Read model device-local; não entra em Loro, sync, Chat Transcript Export ou Managed Provider Usage.
- Captura semântica sanitizada sempre ativa; captura completa exige consentimento para o próximo run de um Chat/perfil, consumido uma vez e limpo no restart.
- `ROW_HEIGHT` permanece `px(26.0)`; três lanes `Input`, `Model`, `Tools`; breakpoint permanece `px(600.0)`.
- Busca atenua registros não correspondentes sem remover a cronologia. Overrides manuais de fold são preservados; seleção abre somente seu caminho ancestral necessário.
- Ausência não é zero. Run concluído não transforma tool sem resultado em sucesso. Evento selecionado e operação derivada são identidades distintas.
- Fonte diagnóstica: validade de 7 dias, orçamento de 128 MiB por perfil, limite de 1 MiB por campo, fidelidade/tamanho original explícitos, permissões owner-only.
- Captura fail-open com writer serializado e fila bounded; leituras bloqueantes fora do hot path async; referências opacas, RPCs locais e sem forwarding.
- Migrações aditivas; preservar Run Journal, cutover legado, ordenação `(source_seq, sub_seq)` e revisão de commit `rev`. Nunca apagar a base para reconstruir histórico.
- Campos serializados novos são opcionais. Daemon antigo responde unsupported de forma explícita; não interpretar como resultado vazio conhecido.
- Ler a cadeia DOX atual antes de implementar. Worktrees usam targets próprios. Não editar vendor nem o checkout de referência do OMP.
- Captura completa obedece ADR 0005; Raw Reveal obedece ADR 0004 e continua explícito mesmo quando a captura foi consentida.

## Baseline e alcance

Auditoria em 2026-09-22, por código, screenshots do usuário e consulta SQLite somente leitura. Um snapshot do Chat tinha 20.373 registros/12 runs, nenhum turn/step observado. No run mostrado, 81 registros de chamada com resultado associado continuavam Running. O exemplo selecionado era chamada `48930` / resultado `48931`. Esses números são uma fotografia de um stream ativo, não asserts contra o perfil de produção.

Defeitos adicionais confirmados: busca desenhada sem input; Fold Calls dobra Step inteiro; seleção não abre ancestrais; dimming da timeline depende de rows visíveis; corpo do Inspector não rola; adapter OMP emite zeros de consumo; schema é descrição estática; fonte Raw já foi normalizada; import legado descarta o preview sanitizado. O desempenho em 20 mil eventos é um risco a medir, não uma lentidão já comprovada.

Este plano é a receita vigente e incorpora o plano histórico `docs/plans/2026-09-06-0304-fix-trajectory-observability-fidelity-plan.md`. Seus R1–R14 mantêm significado; R15–R22 abaixo cobrem os novos achados. O plano antigo é contexto histórico, não uma fila paralela.

Não incluir trajetórias de CLI Workers, billing/quota, redesign geral, export de raw, interceptação HTTP do provider ou coleta do ambiente inteiro. Não presumir que campos descartados do histórico possam ser recuperados.

## Entregas, dependências e ownership

| Entrega | Resultado verificável | Depende de | Dono principal |
|---|---|---|---|
| T01 | Baseline isolado e fixtures do transporte | — | integração |
| T02 | Import e histórico persistido sanitizados | T01 | engine |
| T03 | Busca editável e identificadores pesquisáveis | T01 | UI |
| T04 | Inspector rolável e resumo legível | T01 | UI |
| T05 | Projeção compartilhada de operações | T01 | proto |
| T06 | Consulta indexada + Inspector correlacionado | T05 | engine/RPC/UI |
| T07 | Fronteiras observadas e envelope interno | T05 | harness/engine |
| T08 | Folds, seleção e dimming coerentes | T03,T06,T07 | UI |
| T09 | Instantes, intervalos e proveniência | T06,T07 | engine/UI |
| T10 | Consumo e ocupação honestos | T07 | harness/engine/UI |
| T11 | Consentimento e contratos de fonte | T07 | engine/RPC |
| T12 | Fonte privada, retenção e Reveal | T11 | engine/RPC/UI |
| T13 | Schema observado e fonte original | T06,T07,T12 | harness/engine/UI |
| T14 | Enriquecimento histórico idempotente | T02,T09,T10,T13 | engine |
| T15 | Follow Live e escala de 20 mil eventos | T08,T09,T14 | UI/integração |
| T16 | Comprovação integrada, revisão e docs | T02–T15 | integração/revisores |

Ordem recomendada: T01 → T02 → T03/T04 → T05/T06 → T07/T08 → T09/T10 → T11/T12/T13 → T14/T15 → T16. Paralelismo é elegibilidade, não licença para editar o mesmo arquivo: T03/T04 e T09/T10 se integram sequencialmente quando compartilham `view.rs`, `inspector.rs` ou `sessions.rs`. Cada entrega passa por TDD, implementação, comprovação, revisão independente e validação nativa quando visual. A publicação é etapa posterior ao aceite, no fork e pelo gate vigente.

## Mapa de arquivos e interfaces

| Arquivos | Responsabilidade e mudança |
|---|---|
| `crates/proto/src/trajectory.rs` | Operação/escopo/completude, timing/usage/schema/source opcionais; projeção pura compartilhada e compatibilidade serde |
| `crates/harness/src/lib.rs`, `src/omp/{mod,normalize,protocol,process}.rs` | Envelope interno; fronteiras/usage/catalog; captura pré-normalização sob política; migrar todos os consumidores internos |
| `crates/engine/src/{sessions,trajectory_store}.rs` | Captura, índices/queries, writer/rev, reparo de previews e enriquecimento bounded |
| Novo `crates/engine/src/trajectory_diagnostics.rs` | Única boundary nova: fonte completa, consentimento aplicado na escrita, limites/expiração/leitura |
| `crates/engine/src/{rpc,profile,workspace_host,lib}.rs` | Autorização local, composição de fonte e ciclo de vida, retenção serializada |
| `crates/rpc/src/{lib,method}.rs` | `GetTrajectoryOperation` e controles de diagnóstico locais; compatibilidade de referências |
| `crates/ui/src/trajectory/{model,view,toolbar}.rs` | Projeção, input real, seleção, respostas assíncronas e controles/follow |
| `crates/ui/src/trajectory/{ledger,timeline,inspector}.rs` | Rows fixas, geometria/hit-testing/dimming, resumos, scroll e dados correlacionados |
| `crates/harness/tests/omp_rpc.rs`, `tests/fixtures/` | Frames compatíveis com o OMP instalado, sem dados do usuário |
| Novo `crates/engine/tests/trajectory_fidelity.rs` | Captura→persistência→watch→reabertura; interfaces públicas de engine/RPC, sem expor internals apenas para testes |
| `crates/ui/src/capture.rs` | Fixtures nativas existentes, apenas para cenários de interação/escala |
| AGENTS dos domínios acima e docs atuais | Atualizar na implementação quando o contrato correspondente estiver comprovado |

Interfaces a estabilizar em T05/T06, antes da delegação dos consumidores:

```text
GetTrajectoryOperation request:
  chat_id + selected_record_id + bounded continuation when present
response:
  selected_event_id + server-resolved scoped operation identity
  sanitized payload/result + their separate persisted source references
  authoritative outcome + completeness + snapshot_rev + continuation

scope = profile/device authorization + Chat + run + complete parent path + call_id
watch rev > snapshot_rev invalidates an older lookup; selection generation gates responses
```

Não aceitar paths de fonte enviados pelo cliente nem chaves de escopo que contornem lookup do registro persistido. Assinaturas Rust/serde finais são congeladas em T05/T06 com testes de round-trip e chamadores mapeados; a shape acima é o contrato entre entregas. Envelope é interno ao harness/engine e nunca adiciona raw ao `AgentEvent` público.

## Receita de execução por entrega

Os comandos abaixo são metas de execução, não resultados já obtidos. Cada novo filtro deve listar e executar ao menos um teste; zero testes não passa. Para um defeito exclusivamente de render GPUI, registrar primeiro a falha por interação nativa e depois a mesma interação corrigida, em vez de inventar cobertura headless inexistente.

### T01 — Caracterizar sem depender do perfil do usuário

**Files:** harness fixtures/`omp_rpc.rs`, testes co-located, `trajectory_fidelity.rs` novo quando necessário. **Interface:** fornece frames sanitizados e oráculos para T02–T15.

1. Registrar checkout, HEAD, alterações alheias, executável/perfil da verificação e versão/capabilities do OMP instalado. Mapear todos os consumidores de símbolos a alterar com graft/LSP; confirmar fontes atuais se o índice estiver stale.
2. Criar frames mínimos que produzam estas relações pelo adaptador, sem preencher diretamente o Inspector:

```text
seq 1 input consumed; step A
seq 2 reasoning; seq 3 call edit/a; seq 4 call read/b
seq 5 result read/b success; seq 6 result edit/a failure
step B; seq 7 message; seq 8 final usage(message-id=M)
duplicate usage(M); next consumed input; result-only/c
legacy segment without timestamps; end-only instant in a measured run
```

3. Variantes: mesmo call ID em outro pai/run; resultado fora de página; resultado tardio; Done sem resultado; fonte sem schema; contexto sem consumo. Para a forma das imagens usar sequências sintéticas 48930/48931, não copiar seu conteúdo.
4. Verificar `cargo test -p zeron-harness --test omp_rpc` e registrar a interação nativa que falha para busca/scroll/folds. Capability ausente bloqueia somente a entrega correspondente; não fingir suporte por fixture perfeita.

### T02 — Corrigir sanitização futura e já persistida

**Files:** `trajectory_store.rs`, `rpc.rs`, testes de store/watch e `trajectory_fidelity.rs`. **Consumes:** importador e sanitizador existentes. **Produces:** preview seguro desde a primeira leitura; patches duráveis de sanitização com nova rev.

1. Adicionar este teste co-located ao store, cobrindo separadamente texto e reasoning:

```rust
#[tokio::test]
async fn trajectory_legacy_preview_redacts_before_reveal() {
    for kind in ["textDelta", "reasoningDelta"] {
        let temp = tempfile::TempDir::new().unwrap();
        let store = TrajectoryStore::open(temp.path()).unwrap();
        let source = temp.path().join("legacy.jsonl");
        let text = "password=trajectory_private_sentinel";
        let lines = [
            serde_json::json!({"seq":1,"event":{"type":kind,"text":text}}),
            serde_json::json!({"seq":2,"event":{"type":"done","status":"completed"}}),
        ].map(|line| line.to_string()).join("\n");
        std::fs::write(&source, &lines).unwrap();
        store.import_legacy_journal("chat", &source).unwrap();
        let rows = store.list_all_records("chat").unwrap();
        let serialized = serde_json::to_string(&rows).unwrap();
        assert!(!serialized.contains("trajectory_private_sentinel"));
        assert_eq!(std::fs::read_to_string(&source).unwrap(), lines);
    }
}
```

2. Rodar `cargo test -p zeron-engine trajectory_legacy_preview_redacts_before_reveal`; esperar falha pela presença do sentinel. Estender a fixture para store antigo já contaminado e assinatura/watch concorrente.
3. No import, usar o preview produzido pelo sanitizador para ambos os coalescers:

```rust
let (summary, sanitized_text) = sanitize_prompt_preview(&text, 1024);
// O campo persistido recebe sanitized_text; nunca truncate_preview(&text, ...).
```

4. Sanitizar a saída dos readers enquanto um reparo versionado percorre, em lotes no writer ordenado, previews já persistidos. Reusar `sanitize_prompt_preview`/redação existente; não reabrir conteúdo bruto do journal para esse reparo. Repetição e retomada após falha precisam ser idempotentes; preservar IDs, referências, metadata e bytes de recovery.
5. Verificar import/store direto, snapshot/watch antes e depois do reparo, reabertura e rev de posição antiga. Nenhum sentinel em preview padrão/SQLite após o reparo; Reveal autorizado continua lendo a fonte original. Aceite exige proteção de leitura imediata, não só futura migração.

### T03 — Conectar uma busca real

**Files:** `view.rs`, `toolbar.rs`, `model.rs`; padrão existente em `crates/ui/src/pickers.rs` e input de `composer.rs`. **Consumes:** `ToolbarAction::Search`, `ClearSearch`, `set_search`. **Produces:** um único estado editável de consulta, de lifetime da surface.

1. Reproduzir na janela: clicar no campo e digitar um nome de tool; registrar ausência de edição. Criar teste de busca por call ID no módulo de testes de `model.rs` usando `make_test_record` existente:

```rust
#[test]
fn trajectory_search_finds_call_identity() {
    let mut model = TrajectoryViewModel::new("test_chat");
    let mut record = make_test_record("r", 1, 0, TrajectoryLane::Tools,
        TrajectoryRecordKind::ToolCall { tool_name: "edit".into() },
        "Tool: edit", "Edit src/lib.rs");
    record.call_id = Some("unique-call-42".into());
    let id = record.id.clone();
    model.apply_watch_item(TrajectoryWatchItem::Snapshot {
        records: vec![record], watermark: None, degraded: vec![], has_more: false,
    });
    model.set_search("unique-call-42");
    assert!(!model.rows().iter().find(|r| r.record.as_ref() == Some(&id)).unwrap().dimmed);
}
```

2. Rodar `cargo test -p zeron-ui trajectory_search_finds_call_identity`; adicionar tabela para run/turn/step/parent/record IDs, resumo, erro, case-insensitive e clear. Busca usa somente metadados sanitizados e IDs; nunca pesquisa raw revelado.
3. Reutilizar o input nativo usado pelos pickers; manter entidade/subscription na view, renderizá-la na toolbar e encaminhar edição à ação Search. Clear sincroniza input e modelo sem loop de eventos ou roubo de foco. Foco/seleção de texto, colar e IME seguem o componente existente, sem teclado ad hoc.
4. Aceite nativo: digitar, colar call ID, apagar, limpar, alternar abas/Chat e fechar surface. Atualização live conserva consulta/foco. Nenhum envio de mensagem do Chat ao pressionar Enter dentro da busca.

### T04 — Inspector rolável e informação útil

**Files:** `inspector.rs`, `view.rs`, `model.rs`, `ledger.rs`. **Consumes:** record/summary sanitizados atuais; T06 posteriormente enriquece com operação. **Produces:** conteúdo alcançável e rótulos com contexto, mantendo row fixa.

1. Fixture nativa: 200 linhas numeradas em preview e Raw Reveal, UUID/call ID longo, larguras de 599 e 600 px, painel baixo. Registrar que a última linha não é alcançável antes do fix.
2. Tornar somente o corpo de cada aba rolável, com ID estável, altura restrita/min-height zero; header e tabs ficam fixos. Coordenar scroll por evento/aba; mudança de seleção vai ao início e limpa Reveal, delta do mesmo evento preserva a leitura.

```rust
// Corpo do Inspector: substituir o clipping sem scroll.
div().id("trajectory-inspector-body")
    .flex_1().min_h_0().overflow_y_scroll()
```

3. Testar labels puros com dados sanitizados: `edit — src/lib.rs — Completed`; resultado sem chamada conhecida conserva nome/estado conhecidos. Colocar ação/alvo/resultado no Summary antes de metadata extensa; IDs completos continuam acessíveis. Não tratar uma falha de tool recuperada como descrição inequívoca do desfecho terminal do run; distinguir resumo de erros de estado terminal quando ambos disponíveis.
4. Aceite: rolar ao marcador 200 em todas as abas aplicáveis, esconder/revelar, trocar seleção e retornar do modo narrow; ledger continua com 26 px e sem conteúdo privado novo nos rótulos.

### T05 — Derivar operações sem reescrever eventos

**Files:** `proto/trajectory.rs`, testes co-located. **Consumes:** eventos imutáveis + identidade de escopo. **Produces:** projeção pura de operação compartilhada, outcomes/completude e referências de entrada/saída.

1. Criar tabela de testes `trajectory_operation_contract`:

```text
start(a), result(a,success)              => Completed; sources=start/result
start(a), result(a,error)                => Error
start(a), start(b), result(b), result(a)  => two independent operations
start(a), Done                          => Unsettled only after complete history resolution
result(a), later start(a)               => authoritative result retained
start(a), result(a), repeated update(a)  => same outcome and original start
same id in two parent paths             => no cross-pairing
incompatible same-scope identities      => explicit ambiguity
```

2. Implementar uma única projeção em proto, índice por escopo e fontes por identidade. Não somar status de registros para inferir o resultado da operação. Não copiar payloads em cada render. Migração de callers exige referências completas antes da edição.
3. Rodar `cargo test -p zeron-proto trajectory_operation_contract` e serde old/new/unknown-field. Congelar tipos e documentar completude antes de passar T06 a outro worker. Eventos originais mantêm status/tempo históricos; o estado atual pertence à operação derivada.

### T06 — Consulta indexada e Inspector correlacionado

**Files:** RPC lib/method, engine store/rpc, UI model/view/inspector. **Consumes:** T05. **Produces:** `GetTrajectoryOperation` e apresentação coerente sem mudar seleção.

1. Teste `trajectory_operation_lookup`: chamada e resultado separados por 20 mil eventos; selecionar início, receber conclusão live enquanto lookup antigo está pendente, trocar seleção e entregar resposta antiga. Esperado: fontes corretas, revisão mais nova vence, nenhuma resposta troca o Inspector atual.
2. Implementar índice/lookup bounded em snapshot consistente; validar Chat/device/perfil e registro persistido. Continuação incompleta permanece Loading/Incomplete, não ausência concluída. Para servidor antigo, mostrar unsupported.
3. Conectar Summary/Payload/Result/Timing às fontes da operação e conservar Record ID do evento escolhido. Reveal de Payload usa a referência do início, Result a do resultado; consultar resultado nunca revela raw automaticamente.
4. Verificar `cargo test -p zeron-engine trajectory_operation_lookup`, `cargo test -p zeron-ui trajectory_operation_contract` e teste RPC round-trip/local-only. Native: selecionar antes da conclusão, observar update sem reclicar, reabrir e inspecionar ambas as pontas.

### T07 — Capturar Turns/Steps e escopos reais

**Files:** harness lib/OMP, sessions/store/proto e consumidores do stream mapeados. **Consumes:** T01,T05. **Produces:** envelope interno e IDs observados estáveis; metadados ausentes válidos para outros harnesses.

1. Teste `trajectory_hierarchy_contract` pelos frames: uma entrada consumida, duas respostas modelo/tools, ACK de steer, consumo posterior e subagentes intercalados. Esperado: dois Steps no primeiro Turn; ACK não abre turno; consumo abre uma vez; escopos não se misturam.
2. Introduzir envelope interno normalizado + metadados antes do fan-out. Metadata-only não duplica mensagens no Transcript e é ordenada deterministicamente. Não adicionar corpos completos ao `AgentEvent` serializável.
3. Persistir fronteiras/parent path e coalescers separados por escopo, com replay seguro. Sem fonte real, mostrar grupo desconhecido, sem fabricar `Turn 1` como observação. Não assumir que todo `turn_start` OMP é entrada de usuário: é fronteira de Step.
4. Verificar `cargo test -p zeron-harness --test omp_rpc`, `cargo test -p zeron-engine trajectory_capture`, `cargo test -p zeron-ui trajectory_hierarchy_contract`; build de todos os consumidores e paridade de Transcript/recovery.

### T08 — Folds, seleção e busca na mesma projeção

**Files:** UI model/ledger/view/timeline. **Consumes:** T03,T06,T07. **Produces:** fold de Calls próprio e seleção que revela o caminho mínimo.

1. Criar `trajectory_navigation_contract`: reasoning → tool → texto → tool; dobrar Calls mantém reasoning/texto. Dobrar Turn e selecionar um filho pela timeline torna a row visível; irmãos manualmente dobrados continuam dobrados.
2. Separar a política de fold de tool do fold de Step. Nunca reordenar resultados concorrentes para tornar grupos contíguos. Abrir ancestrais somente quando a ação explícita de seleção exige, sem alterar toggles globais.
3. Fazer dimming depender da consulta sobre todos os registros, não de `rows()` visíveis. Busca+fold conserva o mesmo conjunto de correspondências nas lanes; seleção continua distinguível.
4. Verificar `cargo test -p zeron-ui trajectory_navigation_contract` e módulos model/timeline/ledger. Native: repetir a sequência busca→fold→clique em span→clear com evento offscreen, nos dois layouts.

### T09 — Usar os tempos observados

**Files:** proto, sessions/store, UI timeline/inspector/toolbar. **Consumes:** T06,T07. **Produces:** instantes/intervalos com proveniência e fallback por segmento.

1. Teste `trajectory_recorded_contract`: start-only, end-only, final de mensagem, duas operações sobrepostas e run legado sem timestamps. Uma regressão de relógio não invalida os outros intervalos.
2. Preservar primeiro início no coalescer até finalização; medir elapsed novo por relógio monotônico onde disponível. Intervalo entre chegada de início/fim é observação do host, separado de duração reportada pelo runtime.
3. Renderizar instantes e intervalos medidos por run/segmento; intervalos desconhecidos identificados como sequência. Toolbar informa geometria efetiva/mista. Hit-testing/seleção continuam corretos para operações sobrepostas e instantes no fim do eixo.
4. Verificar `cargo test -p zeron-ui trajectory_recorded_contract`, overflow e zero medido em proto; native Sequence/Recorded na mesma seleção e mistura de histórico antigo/novo.

### T10 — Uso do modelo e contexto

**Files:** OMP mod/normalize/protocol, proto, sessions/store, inspector. **Consumes:** T07. **Produces:** consumo opcional com origem; ocupação/capacidade como snapshot distinto.

1. Oráculo `trajectory_usage_fidelity`: contexto 109686/272000 sem consumo => contexto conhecido, consumo ausente. Mensagem final M input=10/output=5 repetida em dois frames => uma contribuição. Zero explicitamente medido permanece zero.
2. Extrair usage final autoritativo por mensagem/escopo; não usar o zero literal do encerramento OMP como medição. Cache/total seguem definição da fonte; totais cumulativos de sessão não viram deltas. Não duplicar uso de subagentes no pai.
3. Expor uso no escopo mensagem/Step/run adequado e parcialidade; ferramenta sem métrica não herda tokens do run. Preservar indicador last-known de contexto do Chat e snapshots antigos após compactação/troca de modelo.
4. Verificar `cargo test -p zeron-engine trajectory_usage_fidelity`, frames OMP, replay/reopen/cache/absência e native com fonte real.

### T11 — Consentimento e contratos locais

**Files:** proto, RPC lib/method, engine sessions/rpc, UI toolbar/model/view. **Consumes:** T07. **Produces:** política next-run e referências versionadas de fonte diagnóstica.

1. Teste `trajectory_diagnostic_consent`: Off→Armed(A,X)→Capturing(runA)→Off; B não consome consentimento; restart/perfil/delete limpam; fechar surface não revoga. Revogar run ativo impede escritas enfileiradas ainda não autorizadas no writer.
2. Implementar política engine-owned e controles locais arm/disarm/revoke/delete; enum de fonte/fidelidade/indisponibilidade aditivo. Nunca ativar produtores completos enquanto T12 não passar.
3. UI mostra escopo, limites e estado antes de armar; captura não revela. Verificar `cargo test -p zeron-engine trajectory_diagnostic_consent` e rejeição RPC remota com/sem targetDeviceId; native com um Chat diferente em paralelo.

### T12 — Armazenamento diagnóstico e Reveal protegido

**Files:** novo trajectory_diagnostics.rs; engine lib/profile/rpc/workspace_host; UI inspector/view; testes RPC device_room. **Consumes:** T11. **Produces:** fonte local finita e leitura efêmera autorizada.

1. Testes `trajectory_diagnostics`: permissão owner-only, symlink/path escape, referência não anexada, fonte de outro perfil/Chat/field/versão. Fonte inexistente/corrupta/truncada não retorna sucesso vazio.
2. Implementar writer bounded e leitores independentes, fontes opacas e uma rotina serializada de retenção. Injetar relógio/orçamento de teste: expirar em sete dias, evict oldest-first a 128 MiB, limite de campo 1 MiB com tamanho/fidelidade explícitos.
3. Exercer queue full/writer failure/disk failure enquanto o run continua; gap explícito. Archive retém semântico; delete diagnostics preserva recovery/ledger; delete Chat local/sync/Space remove suas fontes. Revogação revalidada na escrita, leitura revalidada antes de responder.
4. Reusar autorização de Reveal e invalidar tarefas/texto ao trocar seleção, surface, perfil, Chat ou fonte. Expiração enquanto já revelado também limpa o estado. Retry explícito para falha transitória, sem trocar fonte silenciosamente.
5. Verificar `cargo test -p zeron-engine trajectory_diagnostics`, `cargo test -p zeron-engine trajectory_reveal` e testes de relay. Revisão independente da fronteira de privacidade é requisito de aceite desta entrega.

### T13 — Schema observado e argumentos/resultados originais

**Files:** OMP mod/normalize/protocol/process, proto, sessions/store/diagnostics/rpc, inspector/view. **Consumes:** T06,T07,T12. **Produces:** snapshots de schema com proveniência e fonte original recebida sob opt-in.

1. Testes `trajectory_source_fidelity`: args command/cwd/env/timeout e resultado com text/details/diff/image-metadata/artifact-reference; catálogo A antes da execução e B depois. Fonte revelada preserva A e representação recebida; não usa B retroativamente.
2. Capturar catálogo por `get_state.dumpTools` somente se suportado pelo executável verificado; deduplicar por conteúdo. Observar depois de registrar host tools e em mudanças de catálogo, não a cada token. Extrair apenas schema necessário, sem systemPrompt.
3. Capturar args/result antes de normalizar quando a política estiver ativa, sem clone/fila raw quando off. Não seguir paths de artifacts nem ler ambiente herdado/credenciais. Schema completo também obedece consentimento e Reveal; default omite descrições/examples sensíveis.
4. Exibir origem, horário/precisão de observação e fidelidade: original recebido, normalizado legado, sanitizado, truncado, expirado, não capturado. Schema estático nunca é vendido como schema efetivo. Fonte resumida pelo runtime não significa stdout ilimitado.
5. Verificar frames reais e `cargo test -p zeron-harness --test omp_rpc`, source-fidelity em engine e native. Sentinel exclusivo de metadados diagnósticos deve estar ausente de public AgentEvent, Run Journal, SQLite sanitizado/watch, Voice, sync, export e logs. Revisar novamente após ligar produtores.

### T14 — Enriquecer histórico sem reset

**Files:** store/run_journal/rpc/proto; trajectory_fidelity.rs e restart_resume.rs. **Consumes:** T02,T09,T10,T13. **Produces:** correlação imediata e enriquecimento recuperável com marcador independente da importação legada.

1. Perfil temporário antigo com call/result separados, contexto conhecido, zeros comprovadamente sintéticos, marker legado e rows nativas novas. Oráculo: correlação e contexto melhoram; journals iguais byte a byte; nada duplica.
2. Implementar lotes lazy com marcador/versionamento e writer/rev, retomada após crash e proteção de campos nativos mais novos. Não resetar `trajectory_legacy_imports`; reusar o mecanismo bounded de reparo T02 sem conflitar com seus marcadores.
3. Journal ausente/corrupto/oversized mantém histórico existente e diagnóstico localizado; schema/args/tempos que não existiam continuam ausentes. Nenhuma comparação com o catálogo atual reconstrói fonte histórica.
4. Verificar `cargo test -p zeron-engine --test trajectory_fidelity`, `cargo test -p zeron-engine --test restart_resume`, `cargo test -p zeron-engine trajectory_watch`; duas execuções do reparo, crash/reopen e subscriber numa rev antiga.

### T15 — Follow Live e escala observada

**Files:** view/model/toolbar/ledger/timeline e fixture capture.rs. **Consumes:** T08,T09,T14. **Produces:** navegação sem saltos e custo medido com 20.373 eventos/12 runs sintéticos.

1. Reproduzir scroll para trás com stream quieto. Tornar a ação explícita de voltar ao live-edge acessível mesmo sem eventos novos. Se observar offset fora do watch, usar o mesmo guard `live_jump_from`; preservar a decisão no watch antes de catch-up e a tolerância atual de duas linhas. Nunca rearmar só por chegar ao fim.
2. Testes `trajectory_live_navigation`: wheel/trackpad/drag/keyboard, seleção offscreen, salto programático pendente e delta mais rápido que frame. Viewport do usuário vence; pending_live conta novos eventos, não revisões da mesma row.
3. Medir build otimizada (`cargo build --release`, target próprio do checkout) com fixture de 20.373 eventos e stream sintético de 10 deltas/s por 60 s: abertura, busca, fold, seleção e p95 de atualização. Coletar pelo menos 100 amostras de input/seleção por execução; medir do recebimento da ação até o frame que apresenta seu efeito, e registrar intervalos entre frames com um probe limitado à fixture nativa. Publicar amostras/percentis e identificação de máquina/binário/perfil junto da evidência, removendo probes temporários no closeout; duração de função pura sozinha não substitui latência visível. Alvo de aceite proposto: resposta visível a input/seleção abaixo de 100 ms p95 e nenhuma pausa de UI maior que 250 ms durante esse exercício, na máquina de referência registrada. Falha exige otimização e nova medição, não apenas apontar que ledger é virtualizado.
4. Manter ledger virtualizado; evitar clone/reconstrução completa por frame quando registros não mudaram; invalidar projeções por revisão/consulta/fold. Se spans excederem resolução útil, agregar pintura por faixa de pixels preservando associação/hit-testing determinístico aos eventos e indicação de densidade. Consulta de uma operação não carrega todo o Chat. Não introduzir dashboard ou dependência nova.
5. Verificar `cargo test -p zeron-ui trajectory_live_navigation`, geometria/hit-testing e native benchmark antes/depois com mesmo perfil sintético. Escala temporal, seleção, erros e busca não podem mudar por causa da otimização.

### T16 — Comprovar, revisar e encerrar

**Files:** testes de integração, fixtures nativas e contratos afetados. **Consumes:** todas as entregas. **Produces:** evidência por requisito e aceite explícito de implementação.

1. Rodar suites focadas por entrega; ao integrar, uma passagem de `cargo fmt --all -- --check`, `cargo build`, `cargo test --workspace` e `scripts/e2e-smoke.sh`, após ler as instruções locais dos scripts. Corrigir falhas pertinentes e registrar qualquer bloqueio externo com comando/erro; não contar comandos com zero testes como prova.
2. Executar cenário nativo com fonte OMP real em perfil isolado: chamada selecionada antes de concluir; concorrência e falha; duas entradas e múltiplas respostas; reabertura/reconexão; busca digitada/call ID/clear; folds e seleção oculta; Inspector longo; narrow/back; timing misto; contexto/uso; schema; consentimento/revoke/delete/Reveal e resposta atrasada. Temas claro/escuro, 599/600 px e painel baixo. Registrar executável, HEAD, perfil e evidências sem raw do usuário.
3. Revisão independente de Spec, correção/regressões e privacidade sobre o diff integrado; findings bloqueantes voltam à entrega dona. Não aceitar apenas screenshot de fixture artificial ou teste puro enquanto a interação real continua quebrada.
4. Atualizar DOX/Test Coverage Matrix dos domínios alterados, `CONTEXT.md` somente se termos precisarem de ajuste, e trechos afetados de `ARCHITECTURE.md`, `DESIGN.md`, `FUNCTIONAL-BASELINE.html`, `fork_changelog.md`, `docs/streaming-guide.md`. Corrigir a documentação atual que equipara campo ausente em andamento a Unsettled e a que limita a observação de follow ao watch. Não documentar entregas pendentes como implementadas.
5. Preencher evidência R1–R22 no tasks ledger, validar OpenSpec e arquivar somente após implementação/aceite. Commit/publicação obedecem a autorização da execução e gates do repo; push somente ao fork. Nenhum restart do processo que hospeda trabalho ativo integra a validação isolada.

## Matriz de cobertura e aceite

| Requisito | Entregas | Evidência exigida |
|---|---|---|
| R1 operação completa | T05,T06 | início e resultado mostram mesma operação mantendo evento selecionado |
| R2 escopo/replay/páginas | T05,T06,T07 | concorrência, pai/run distintos, atualização repetida e histórico paginado |
| R3 desfecho honesto | T05,T06 | resultado ausente, error, result-only e chegada tardia |
| R4 fronteiras/folds | T07,T08 | consumo observado, Steps, unknown explícito e texto preservado |
| R5 timing | T09 | instantes, intervalos, mixed/legacy e proveniência |
| R6 uso/contexto | T10 | ocupação, consumo opcional, zero medido, cache/replay |
| R7 schema | T13 | catálogo observado com precisão, histórico e indisponibilidade |
| R8 original opt-in | T12,T13 | campos recebidos preservados sob consentimento |
| R9 consentimento | T11,T12 | next-run/Chat/perfil, off/revoke/restart |
| R10 isolamento | T12,T13 | sentinel fora de todos os caminhos públicos/normalizados |
| R11 Reveal | T06,T12,T13 | referências/ownership, limites e invalidação de resposta atrasada |
| R12 histórico | T14 | idempotência sem journal rewrite/reset |
| R13 convergência | T06,T14,T16 | live/reopen/rev/paging com seleção e scroll estáveis |
| R14 fail-open | T12,T14 | queue/writer/disk failures sem interromper run |
| R15 busca utilizável | T03 | input real, teclado/colar/clear e IDs pesquisáveis |
| R16 seleção/dimming | T08 | filho dobrado aparece; busca atenua mesmos eventos com folds |
| R17 leitura do Inspector | T04 | última linha alcançável em todas as abas e narrow |
| R18 sanitização legada | T02,T14 | import e rows antigas seguros antes do primeiro watch; repair persistido |
| R19 resumo útil | T04,T06 | ação/alvo/desfecho primeiro, metadata completa acessível |
| R20 follow quieto | T15 | retorno explícito disponível sem delta e sem rearme implícito |
| R21 escala | T15 | 20.373 eventos, benchmark nativo e identidades/hit-testing preservados |
| R22 comprovação nativa | T16 | cadeia runtime→adapter→store→watch→UI e controles reais |

Unit cobre projeção/estado/geometria; integração cobre adapter/store/RPC/lifecycle. O render GPUI exige prova nativa manual adicional: em cenários exclusivamente visuais o delta spec usa `Test: none` com motivo explícito, refletindo a matriz atual, sem alegar um harness de UI inexistente. Os testes propostos ainda serão escritos e executados na implementação.

## Limites e recuperação

Uma capability ausente no OMP instalado deve produzir indisponibilidade honesta e evidência da lacuna; não declarar completa a entrega de fidelidade correspondente sem uma fonte suportada. Resolver adaptação do runtime na fase dona sem expandir silenciosamente para o plano de paridade OMP inteiro.

Rollback desarma captura completa primeiro e preserva dados semânticos/recovery. Não reverter para um binário que volte a servir previews legados inseguros enquanto o reparo não tiver sido persistido; não remover schemas/marcadores por downgrade. Histórico irrecuperável permanece explicitamente ausente. Validação deste documento não é validação da implementação.

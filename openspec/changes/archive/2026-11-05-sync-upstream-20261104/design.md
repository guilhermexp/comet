# Design

## Context

Ver proposal.md e docs/upstream-sync.md. Base 0f5c6a34 (1.0.16), ancestral upstream d5a01522, alvo 35561ead. Merge-tree prevê 153 conflitos. Não há trail instalado nesta box; contexto usa DOX, históricos e diffs Git do intervalo fixo.

## Goals / Non-Goals

Incorporar o alvo como segundo pai de um merge, após commit próprio de planejamento e cargo fmt do fork. Preservar código exclusivo e adaptar APIs compartilhadas. Não ativar outro produto/interface, alterar preferências pessoais, publicar main/tags ou arquivar a mudança.

## Decisions

### D1 — interface dormente; D1a — runtime de plugins/templates ativo

Conservar apps/zgx fora do workspace padrão, sem trocar launch, shell gpui, Workers, Details, Usage ou terminal direito. Inset/split e a interface zgx permanecem dormentes. A capability zgx-interface-v1 não é anunciada: o cliente beta a usa para a interface completa, não como gate independente de plugins/templates.

D1a autorizado pelo brief de 2026-11-05: iniciar Plugins::start no boot, ligar attach_auth em set_auth, links em set_links e with_plugins no RPC normal. Plugins/templates ficam acessíveis pelos RPCs existentes, que os clientes chamam diretamente; não existe capability dedicada a anunciar. Templates conservam o requisito de assets locais previamente instalados, sem ativar renderer/instalação zgx. Falha de registro de identidade /plugin-keys (404, 501 ou rede) degrada com log; startup, sign-in e plugins locais continuam disponíveis. Os serviços adicionais do edge seguem dormentes.

Grants raiz e workers_mcp continuam autoritativos: drive_run limpa mcp e extra_mcp antes do retry, nenhum MCP nativo de plugin entra nas runs, autorização de plugin não concede sessions. SessionsEngine::set_plugins só guarda o seam de injeção MCP, sem consumidor no fork; não é ligado. Alternativa rejeitada: usar a ativação do catálogo de plugins para ampliar autoridade ou anunciar a interface beta.

### D2 — edge mantém o grafo publicado do fork

Preview/login e materialização do transcript entram vivos; directory/profile/badges/card-web, Cloud/vault/runner, plugin directory e bindings adicionais permanecem dormentes. wrangler/deploy e testes existentes do fork são preservados. Alternativa rejeitada: provisionar armazenamento/rotas públicas automaticamente. Aberta ao dono: eventual ativação desses serviços em change separado.

### D3 — Settings/Usage e apresentação do fork

Aceitar busca/scroll/atalhos/anotações/imagens e percentuais Cursor sobre Projects/Accounts, persistência apply_shell_settings, medidores Grok/Kimi/Antigravity e transcript compacto. Recusar a restauração do anel Usage/context do upstream e do terminal de rodapé. Theme tools devem compartilhar o catálogo do fork, não instalar um segundo modelo. Sinais de PR mantêm o runner limitado/cwd e obtêm viewer pela metadata autenticada do fork; seu cache account identifica credencial, não username.

### D4 — AccessMode, Fx e ACP registry com grants do fork

Adicionar opções aditivas de acesso/plano e Fx/registry; OMP/Kimi preservados e não prometem modos que não suportam. A engine limpa RunRequest.mcp e extra_mcp antes de capturar o retry e invocar o driver; chamadas diretas aos drivers conservam extra_mcp. A configuração de processo continua comparando grants e acesso efetivo, execution/update leases continuam vinculantes. MCP de plugins não é injetado globalmente. Native subagents continua derivado apenas do sessions grant.

### D5 — escopos IPC preservam os clientes autorizados

Aceitar hardening de identidade/Caller e bearer agent-scoped quando compatível; todos os métodos novos usam method.rs, sem lista concorrente de forwardables. Adaptar permissões para zeron mcp e controller Workers (registro único de projetos, atividade/espera), mantendo ambos os contratos de autenticação e credenciais que não alcançam Workers. Se o escopo novo não representar com segurança esses clientes, manter o bearer do fork nesse cliente e registrar a adaptação; nunca alargar grant raiz.

### D6 — ChildKind sem reparenting

Aceitar metadata aditiva de side chat/subagent, preservando parent_chat_id e origin_chat_id como fatos independentes. Ambos tornam o Chat não raiz; configured/inherit e recusa de netos do fork permanecem. Testes/literais recebem campos opcionais/defaults, sem apagar histórico.

### D7 — processos e output no lifecycle do fork

Aceitar lista/leitura/parada de processos em background e tails/exit code de tools, mantendo mailbox Worker, quiesce, Stop, runway e TurnSteps. ReadToolOutput autoriza host/Chat e mantém Run Journal privado; Write/Edit continuam sanitizados. Não transformar output de comando em acesso genérico ao journal.

R6 (round2): corrigir o seam nativo compartilhado para que o par harness/engine compile no Windows, com locks, privacidade e observação de processo reais. OMP usa o Command/Child do harness (Job na criação em Windows); o Host PTY/socket/FIFO do vendor segue explicitamente Unix, sem anunciar capacidade inexistente. Isso não ativa descoberta de preview, Cloud ou serviços dormentes.

### D8 — publicação e engine do fork

1.0.16 e workflows integrais do fork vencem; CLI/launch/channel upstream podem existir como código dormente. Main/daemon preservam engine in-process e IPC, sem zeron-engine background nem beta/feed zeron.sh. Sem force/tags/merge da PR. Alternativa rejeitada: adotar packaging beta/managed upstream implicitamente.

### D9 — zui com proveniência

API nova de WPE/dma-buf exige portar commits necessários em fonte separada dentro do worktree e copiar ao vendor com registro third_party/zui-upstream.toml, preservando patches locais. Verificar tanto line_wrapper quanto line_layout de closing punctuation. Nunca trocar para dependência git do zui.

### D10 — dois formatos de CreateWorkspaceEntry

O fork já usa parentPath/name e reply WorkspaceEntryMutation. O upstream adicionou um homônimo incompatível com operationId/path/expectedCheckoutId e reply WorkspaceMutationOutcome. Preservar o tipo legado e nomear o novo CreateWorkspaceEntryMutationRequest; dispatch por operationId no mesmo RPC. Ambos mantêm ownership/path gates, e o novo mantém checkout identity/no-replace. Teste integrado cobre ambos, destino existente e checkout stale. Alternativa rejeitada: quebrar o cliente Files já instalado.

## Risks / Trade-offs

- Grande delta de plugins/registry → D1a ativo com grants preservados, registro remoto best-effort e regressões focadas.
- API de zui mudou → fonte separada e proveniência; compilação downstream.
- Mudanças cross-device → campos defaultados e testes proto/doc/sync/edge.
- Compilação cara → target exclusivo, jobs=2/sccache preservados, df antes dos builds; parar abaixo de 20 GiB.
- QA macOS/iOS indisponível → limitação explícita, sem prova nativa inventada.

## Migration Plan

Planejamento → fmt → merge real → resolução por arquivo → check all targets/regressões/edge → auditoria additions apenas d5a01522..35561ead → docs-only commit → push branch/PR draft. Não arquivar; aceite e merge pertencem ao dono. Rollback eventual é decisão dele, sem reescrever história.

## Open Questions

Dono decide em trabalhos futuros se ativa zgx/Inset/split (D1; runtime D1a autorizado), directory/profile/badges/card-web (D2) ou canal beta/CLI/managed engine (D8). D1a fica ativo; as interfaces e serviços de D1/D2/D8 continuam dormentes. Aceite macOS/iOS e visual precede sair de draft.

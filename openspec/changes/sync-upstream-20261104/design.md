# Design

## Context

Ver proposal.md e docs/upstream-sync.md. Base 0f5c6a34 (1.0.16), ancestral upstream d5a01522, alvo 35561ead. Merge-tree prevê 153 conflitos. Não há trail instalado nesta box; contexto usa DOX, históricos e diffs Git do intervalo fixo.

## Goals / Non-Goals

Incorporar o alvo como segundo pai de um merge, após commit próprio de planejamento e cargo fmt do fork. Preservar código exclusivo e adaptar APIs compartilhadas. Não ativar outro produto/interface, alterar preferências pessoais, publicar main/tags ou arquivar a mudança.

## Decisions

### D1 — zgx, plugins e templates dormentes

Conservar apps/zgx como fonte fora do workspace padrão, sem trocar launch, shell gpui, Workers, Details, Usage ou terminal direito. Plugins/templates e serviços novos ficam sem ativação automática na engine do fork; autorização de plugin nunca substitui grant raiz. Alternativa rejeitada: migrar a interface e iniciar código de plugin sem aceite. Decisão aberta ao dono: se e quando iniciar migração separada para zgx/plugins/templates.

### D2 — edge mantém o grafo publicado do fork

Preview/login e materialização do transcript entram vivos; directory/profile/badges/card-web, Cloud/vault/runner, plugin directory e bindings adicionais permanecem dormentes. wrangler/deploy e testes existentes do fork são preservados. Alternativa rejeitada: provisionar armazenamento/rotas públicas automaticamente. Aberta ao dono: eventual ativação desses serviços em change separado.

### D3 — Settings/Usage e apresentação do fork

Aceitar busca/scroll/atalhos/anotações/imagens e percentuais Cursor sobre Projects/Accounts, persistência apply_shell_settings, medidores Grok/Kimi/Antigravity e transcript compacto. Recusar a restauração do anel Usage/context do upstream e do terminal de rodapé. Theme tools devem compartilhar o catálogo do fork, não instalar um segundo modelo.

### D4 — AccessMode, Fx e ACP registry com grants do fork

Adicionar opções aditivas de acesso/plano e Fx/registry; OMP/Kimi preservados e não prometem modos que não suportam. RunRequest.mcp continua vazio, configuração de processo continua comparando grants, execution/update leases continuam vinculantes. MCP de plugins não é injetado globalmente. Native subagents continua derivado apenas do sessions grant.

### D5 — escopos IPC preservam os clientes autorizados

Aceitar hardening de identidade/Caller e bearer agent-scoped quando compatível; todos os métodos novos usam method.rs, sem lista concorrente de forwardables. Adaptar permissões para zeron mcp e controller Workers (registro único de projetos, atividade/espera), mantendo ambos os contratos de autenticação e credenciais que não alcançam Workers. Se o escopo novo não representar com segurança esses clientes, manter o bearer do fork nesse cliente e registrar a adaptação; nunca alargar grant raiz.

### D6 — ChildKind sem reparenting

Aceitar metadata aditiva de side chat/subagent, preservando parent_chat_id e origin_chat_id como fatos independentes. Ambos tornam o Chat não raiz; configured/inherit e recusa de netos do fork permanecem. Testes/literais recebem campos opcionais/defaults, sem apagar histórico.

### D7 — processos e output no lifecycle do fork

Aceitar lista/leitura/parada de processos em background e tails/exit code de tools, mantendo mailbox Worker, quiesce, Stop, runway e TurnSteps. ReadToolOutput autoriza host/Chat e mantém Run Journal privado; Write/Edit continuam sanitizados. Não transformar output de comando em acesso genérico ao journal.

### D8 — publicação e engine do fork

1.0.16 e workflows integrais do fork vencem; CLI/launch/channel upstream podem existir como código dormente. Main/daemon preservam engine in-process e IPC, sem zeron-engine background nem beta/feed zeron.sh. Sem force/tags/merge da PR. Alternativa rejeitada: adotar packaging beta/managed upstream implicitamente.

### D9 — zui com proveniência

API nova de WPE/dma-buf exige portar commits necessários em fonte separada dentro do worktree e copiar ao vendor com registro third_party/zui-upstream.toml, preservando patches locais. Verificar tanto line_wrapper quanto line_layout de closing punctuation. Nunca trocar para dependência git do zui.

## Risks / Trade-offs

- Grande delta de plugins/registry → runtime dormente, gates e regressões focadas.
- API de zui mudou → fonte separada e proveniência; compilação downstream.
- Mudanças cross-device → campos defaultados e testes proto/doc/sync/edge.
- Compilação cara → target exclusivo, jobs=2/sccache preservados, df antes dos builds; parar abaixo de 20 GiB.
- QA macOS/iOS indisponível → limitação explícita, sem prova nativa inventada.

## Migration Plan

Planejamento → fmt → merge real → resolução por arquivo → check all targets/regressões/edge → auditoria additions apenas d5a01522..35561ead → docs-only commit → push branch/PR draft. Não arquivar; aceite e merge pertencem ao dono. Rollback eventual é decisão dele, sem reescrever história.

## Open Questions

Dono decide em trabalhos futuros se ativa zgx/plugins/templates (D1), directory/profile/badges/card-web (D2) ou canal beta (D8). Nesta PR a escolha já está fixada: permanecer dormente. Aceite macOS/iOS e visual precede sair de draft.

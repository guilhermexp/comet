# Design

## Context

Ver `proposal.md` para motivação. Pesquisa somente leitura encontrou:

- `crates/ui/src/todo_panel.rs`: `TodoPanelState.expanded: Option<bool>` e `is_expanded(finished)` usam `unwrap_or(!finished)`. `Composer.todo_panels` mantém estado por Chat em memória. `observe` recolhe uma vez quando todos concluem e o turno fica ocioso.
- `composer.rs`: o `@` já tem `MentionLevel::{Root, Projects}`, seleção por teclado/clique, filtro local e inserção por `replace_mention_link`; rascunho e histórico guardam Markdown privado.
- `composer/chip.rs`: chips de tipos distintos, inclusive Chat; um identificador não precisa representar filesystem.
- `proto/src/chat_mentions.rs`: precedente de URI canônica, parser estrito, label escapado e expansão para prompt legível. `invocation::harness_prompt` centraliza transformações usadas por envio e steer; Cursor/OpenCode têm consumidores próprios já existentes.
- Devices vêm de `AppState.listed_devices`; Project/Space tem `device_id` e path no host dono. As ferramentas existentes resolvem device por ID e projeto dentro do destino. A menção de projeto atual contém path, não seleciona host.

Projects worked foi integrado na main local `58d36b546f57b361535ce53959281371e5fbf059`, sobre a base inicial `122c3090f` desta change. A integração deve preservar `Chat.worked_projects`, seu witness causal, `EnsureChatWorkedProjects` e os metadados de UiSettings/sidebar ao reconciliar literals e merges. Nada desta change autoriza sobrescrever trabalho ou interromper targets/provas alheios.

## Goals / Non-Goals

**Goals:** reduzir ocupação inicial do Todo e facilitar referência inequívoca ao dispositivo de trabalho, preservando o transporte textual e o desenho vivo do composer.

**Non-Goals:** mover o Chat atual ao mencionar device, criar SSH/clone/sync automático, ampliar `sessions`/Workers/MCP, mudar grants/autenticação, criar preferência persistida do Todo, redesenhar popup ou incluir Cloud lógico/efêmero. Não publicar release nem substituir o app instalado nesta entrega sem autorização específica.

## Decisions

### D1 — Todo recolhido sem novo sistema de preferências

Trocar o fallback de `expanded = None` para recolhido. Preservar `Some(true/false)`, estado por Chat e reset de conclusão existente. Não recolher a cada frame, navegação ou snapshot: isso desfaria a escolha manual. Atualizar os comentários e `docs/todo-panel.md`; reescrever testes que exigem a abertura automática substituída, mantendo testes comportamentais de conclusão/dismiss/folds.

### D2 — Dispositivo é uma referência, não um path

Adicionar módulo puro `crates/proto/src/device_mentions.rs`, seguindo `chat_mentions`: URI `[nome](zeron-device:<id>)`, validação de identificador opaco e escaping canônico de label. Funções de link, parsing e expansão seguem o padrão existente, sem dependência de UI ou consulta ao catálogo. Nome é snapshot de apresentação; ID é identidade. Não usar `zeron-project:` nem tratar o ID como path.

Aplicar a transformação no pipeline compartilhado de `invocation::harness_prompt`. O agente recebe nome, `device id` e indicação curta de usar esse ID nas ferramentas de dispositivo existentes. Isso não é grant nem transferência automática. O path do projeto é descoberto no device de destino; presença de uma menção não autoriza replicar diretórios ou escolher outra máquina silenciosamente.

### D3 — Reuso do menu e dos chips

Estender o nível raiz com Devices e a enumeração de nível com a lista correspondente. Fonte: os mesmos `AppState.listed_devices` e `device_presence` usados por Settings → Devices; filtrar máquinas registradas com presença Online, incluindo o local, sem RPC por tecla, sem aposentados/Cloud. Não usar somente existência no registro nem o fallback otimista de IDs desconhecidos para declarar elegibilidade. Ordenar local primeiro e depois nome/ID; filtrar por nome/ID e desambiguar nomes iguais dentro da geometria existente. Atualizar o menu aberto pelas notificações/relógio de presença já existentes, sem heartbeat ou polling paralelo. Revalidar o ID na aceitação; expiração não pode selecionar o vizinho por mudança de índice. Preservar chips já inseridos no rascunho após desconexão. Esta regra substitui a opção inicial de permitir referências offline e foi explicitamente solicitada na aprovação do usuário.

Adicionar `ChipKind::Device` e seu ícone semântico existente ao fluxo de projeção. Cobrir draft, fila, edição, transcript e títulos no render compartilhado; tooltip/acessibilidade permitem inspecionar nome e ID, inclusive após rename. Click/Enter/Tab/Escape seguem o menu atual; não inventar ação de arquivo ou painel de gerenciamento no chip.

### D4 — Texto durável sem migração de wire

O link canônico permanece no prompt original, Run/Steer, fila e transcript. Só a cópia entregue ao provider é expandida. Não adicionar vetores de menções a RunRequest nem alterar CRDT/RPC: isso criaria vários caminhos de propagação para o mesmo dado.

Compatibilidade escolhida: em host antigo sem a expansão, a URI bruta ainda leva ID completo e label legível. Um cliente antigo pode mostrar Markdown comum, sem o novo chip; não prometer affordance nova nele. Não exigir capability nova só para interpretar texto; comprovar que os parsers anteriores não apagam nem reinterpretam esse link como arquivo. Nunca entregar apenas um nome após perder o ID.

### D5 — Alternativas consideradas

- Só inserir nome: insuficiente para nomes repetidos e rename; descartado.
- Selecionar automaticamente outro host/cwd ou ampliar APIs de criação: muda execução e autorização, além do pedido de referência no prompt; descartado.
- Campo estruturado novo no protocolo: duplicaria o fluxo já textual de fila/steer/reenvio e exigiria migração entre versões; descartado em favor da URI compartilhada.

### D6 — Execução e verificação proporcionais

Um único dono de integração no composer; implementação em Worker isolado por cruzar UI/proto, a partir da base atual do fork, depois de coordenar com o Chat Projects worked. Preservar trabalho/targets alheios. Antes de editar exports ou consumidores, levantar referências com LSP quando disponível. Ler cadeia DOX do target e Impeccable; o contexto visual foi carregado e DESIGN.md examinado integralmente pelo scout. Reutilizar Theme, popover, rows, foco e animações existentes; nenhuma tarefa de init/rebranding.

Agrupar verificações Cargo locais necessárias com comandos nativos diretos, mantendo jobs=2 e sccache, `CARGO_TARGET_DIR` exclusivo da tarefa e do checkout e avaliação do espaço livre antes de compilar. Reutilizar esse target nas provas da mesma fonte; não usar `scripts/cargo-verify.py`, seu mínimo de 60 GiB, heavy-run ou wrappers de serialização, nem repetir baterias apenas por ritual. Provar estado/UI por fixture GPUI e o caminho até o harness por consumidor real isolado, sem conta/provider externo. Observação visual nativa deve usar fixture que não inicialize integrações reais; não abrir cópia completa do app sem isolamento demonstrado dos providers, conforme DOX. Se a surface não puder ser exercitada com segurança, registrar `notProven`, sem fingir prova visual nem substituir o app do usuário.

## Risks / Trade-offs

- Presença stale ou nomes iguais → mesma fonte/critério de Settings, somente online na lista, desambiguação por ID, revalidação na aceitação e preservação das referências já inseridas; nenhuma execução na seleção.
- Perda da referência em fila/steer/replay ou parser de arquivo → testes comportamentais nos consumidores reais e integração compartilhada, não só snapshot do texto montado.
- Labels inválidos ou exemplos de código → parser canônico estrito, reaproveitando escaping existente.
- Incompatibilidade com host antigo → testar preservação literal de nome/ID sem a nova transformação; não mudar schema nem presumir versão do destino.
- Concorrência com a mudança de Projects worked → isolamento e integração serial do composer; não tocar no target exclusivo em uso.

## Migration Plan

Sem migração de dados ou settings. Aplicar o novo default de apresentação e os novos links como dados textuais no fluxo existente; históricos antigos continuam válidos. Preservar menções de arquivo/projeto/Chat/anexo e seu comportamento. Após provas, revisão e scanner, integrar apenas esta change; atualizar DOX, arquivar pelo CLI e fechar recursos próprios. Release/instalação não fazem parte do destino autorizado aqui.

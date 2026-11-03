# Tasks

Plano aprovado pelo usuário com o refinamento: Devices alinhado a Settings → Devices e somente dispositivos online. Destino autorizado: correção integrada no fork e prova local segura; release e substituição do app instalado não estão autorizadas nesta change.

## 1. Preservação e execução

- [x] 1.1 Coordenar o término/ownership do Chat Projects worked, registrar base atual do fork e preservar WIP/targets alheios; executar em Worker isolado com um dono de integração do composer. Verificar checkout/base e recibo do estado preservado antes de escrever; não absorver a mudança vizinha.

## 2. Todo inicialmente recolhido

- [x] 2.1 Criar prova RED de lista nova/progresso sem clique em `crates/ui/src/todo_panel.rs`, cobrindo corpo ausente e resumo atualizado; observar a falha com a regra atual. Mudar o fallback para recolhido, retirar parâmetros que se tornarem sem uso e migrar seus consumidores após levantamento de referências. Verificar GREEN, sem trocar o formato das tarefas ou criar settings novos.
- [x] 2.2 Provar na fixture GPUI o clique real no header, expansão conservada durante progresso, recolhimento manual, estado independente por Chat e conclusão/ociosidade existente. Preservar os testes úteis de dismiss/folds/lista vazia; atualizar `docs/todo-panel.md` e o contrato dono da UI, sem re-pin de testes incidentais.

## 3. Dispositivos no autocomplete e no prompt

- [x] 3.1 Acrescentar provas em `crates/proto/src/device_mentions.rs`: dois IDs sob mesmo nome, label escapado, ID inválido, exemplo em código e referência legível sem parser novo. Implementar o formato `zeron-device:<id>` e funções de link/parser/expansão seguindo `chat_mentions.rs`; integrar exports em `proto/src/lib.rs` e transformação em `proto/src/invocation.rs`. Verificar GREEN e documentar o contrato no DOX de proto, sem alterar schemas CRDT/RPC. O RED histórico por caso não foi observado; conforme autorização do dono, a prova substituta usa mutações comportamentais pós-implementação, fonte restaurada por hash e GREEN focado, com recibos em `.tmp/verify/refine-composer-todo-device-mentions/verification.md`.
- [x] 3.2 Estender `composer.rs` com nível Devices, catálogo/presença compartilhados com Settings → Devices e somente máquinas registradas online (incluindo local), filtro nome/ID e seleção por clique/Enter/Tab/Escape; adicionar tipo Device em `composer/chip.rs` e projeções compartilhadas. Provar IDs distintos com nomes iguais, exclusão de offline/retired/Cloud, atualização do menu aberto na expiração/reconexão/rename, revalidação do ID sem troca por índice e preservação de chips já inseridos após desconexão. Manter host/cwd e menções existentes; reutilizar geometria/tokens/foco/ícones/animações e atualizar o DOX.
- [x] 3.3 Exercitar envio inicial/steer e os consumidores Cursor/OpenCode já existentes, além de edição de fila, reenvio e restauração de falha. Verificar que a referência canônica persiste e o agente recebe o mesmo ID selecionado; usar consumidor real isolado/fixtures existentes, não teste permanente de cópia ou forwarding. Provar também a passagem textual por host sem a nova expansão, sem inventar resolução de path remoto.

## 4. Prova integrada e entrega

- [x] 4.1 Agrupar os checks focados necessários de proto/UI/harness com Cargo nativo direto, jobs=2 e sccache, `CARGO_TARGET_DIR` exclusivo da tarefa/checkout e preflight de disco. Reutilizar o target para a mesma fonte, sem `scripts/cargo-verify.py`, seu mínimo de 60 GiB, fila global ou wrappers de serialização, e sem repetir provas válidas por ritual. Conferir revisão, filtros, comando, exit e cenários efetivamente executados; usar CI canônico quando necessário, não iniciar no-mistakes automaticamente.
- [x] 4.2 Fazer smoke da superfície nativa: Todo nasce recolhido, header expande/recolhe e progresso respeita a escolha; `@` lista apenas os online pelo critério de Settings → Devices, acompanha presença no menu aberto, distingue nomes iguais e entrega o ID correto ao consumidor. Usar fixture sem engine/integrações reais e credenciais isoladas; não abrir app completo sem isolamento demonstrado nem substituir o instalado. Guardar capturas e prova do destino textual. Sem prova segura, registrar `notProven` e o bloqueio, não marcar esta tarefa concluída.
- [x] 4.3 Executar revisão pré-commit pelo `/Users/guilhermevarela/orchestrator.v5/scripts/review-pair.sh --repo <checkout> --base <base> --out <rodada-nova> --include-untracked --gate`; preservar scanner e snapshot exato. Tratar achados; indisponibilidade/incompletude comprovada exige contingência manual completa registrada, sem retry cego, `--agent` ou `--use-credits`. Registrar modalidade e summary reais antes do commit final.
- [ ] 4.4 Integrar somente esta change sobre a main local `58d36b546f57b361535ce53959281371e5fbf059`, no fork conforme o fluxo nativo do projeto, com commits/revisão/checks identificados. Preservar `Chat.worked_projects`, witness causal, `EnsureChatWorkedProjects` e os metadados de UiSettings/sidebar ao reconciliar literals/merges, sem sobrescrever trabalho/targets alheios. Verificar identidade da árvore integrada e registrar exatamente o destino atingido; não publicar tag/release nem instalar o app por inferência.

## Workflow follow-up

- Com os critérios cumpridos, arquivar pelo CLI e validar specs canônicas; não editar `openspec/specs/` manualmente.
- Executar sync dos AGENTS donos e cleanup de recursos próprios, preservando trabalho, targets, dados, perfis e recibos alheios. Informar remoções/retenções e espaço antes/depois sem atribuir variação concorrente integralmente à tarefa.
- A mudança Projects worked é entrega separada; esta change não autoriza refazê-la nem usar seus builds como prova das novas melhorias.

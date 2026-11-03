# Tasks

Plano aprovado pelo usuário com o refinamento: Devices alinhado a Settings → Devices e somente dispositivos online. Destino autorizado: correção integrada no fork e prova local segura; release e substituição do app instalado não estão autorizadas nesta change.

## 1. Preservação e execução

- [x] 1.1 Coordenar o término/ownership do Chat Projects worked, registrar base atual do fork e preservar WIP/targets alheios; executar em Worker isolado com um dono de integração do composer. Verificar checkout/base e recibo do estado preservado antes de escrever; não absorver a mudança vizinha.

## 2. Todo inicialmente recolhido

- [ ] 2.1 Criar prova RED de lista nova/progresso sem clique em `crates/ui/src/todo_panel.rs`, cobrindo corpo ausente e resumo atualizado; observar a falha com a regra atual. Mudar o fallback para recolhido, retirar parâmetros que se tornarem sem uso e migrar seus consumidores após levantamento de referências. Verificar GREEN, sem trocar o formato das tarefas ou criar settings novos.
- [x] 2.2 Provar na fixture GPUI o clique real no header, expansão conservada durante progresso, recolhimento manual, estado independente por Chat e conclusão/ociosidade existente. Preservar os testes úteis de dismiss/folds/lista vazia; atualizar `docs/todo-panel.md` e o contrato dono da UI, sem re-pin de testes incidentais.

## 3. Dispositivos no autocomplete e no prompt

- [ ] 3.1 Acrescentar provas RED em `crates/proto/src/device_mentions.rs`: dois IDs sob mesmo nome, label escapado, ID inválido, exemplo em código e referência legível sem parser novo. Implementar o formato `zeron-device:<id>` e funções de link/parser/expansão seguindo `chat_mentions.rs`; integrar exports em `proto/src/lib.rs` e transformação em `proto/src/invocation.rs`. Verificar GREEN e documentar o contrato no DOX de proto, sem alterar schemas CRDT/RPC.
- [ ] 3.2 Estender `composer.rs` com nível Devices, catálogo/presença compartilhados com Settings → Devices e somente máquinas registradas online (incluindo local), filtro nome/ID e seleção por clique/Enter/Tab/Escape; adicionar tipo Device em `composer/chip.rs` e projeções compartilhadas. Provar IDs distintos com nomes iguais, exclusão de offline/retired/Cloud, atualização do menu aberto na expiração/reconexão/rename, revalidação do ID sem troca por índice e preservação de chips já inseridos após desconexão. Manter host/cwd e menções existentes; reutilizar geometria/tokens/foco/ícones/animações e atualizar o DOX.
- [ ] 3.3 Exercitar envio inicial/steer e os consumidores Cursor/OpenCode já existentes, além de edição de fila, reenvio e restauração de falha. Verificar que a referência canônica persiste e o agente recebe o mesmo ID selecionado; usar consumidor real isolado/fixtures existentes, não teste permanente de cópia ou forwarding. Provar também a passagem textual por host sem a nova expansão, sem inventar resolução de path remoto.

## 4. Prova integrada e entrega

- [ ] 4.1 Agrupar os checks focados necessários de proto/UI/harness numa rodada de `scripts/cargo-verify.py`, jobs=2 e sccache, sem fila global e sem repetir provas válidas por ritual. Conferir revisão, filtros, comando, exit e cenários efetivamente executados; usar CI canônico quando necessário, não iniciar no-mistakes automaticamente.
- [ ] 4.2 Fazer smoke da superfície nativa: Todo nasce recolhido, header expande/recolhe e progresso respeita a escolha; `@` lista apenas os online pelo critério de Settings → Devices, acompanha presença no menu aberto, distingue nomes iguais e entrega o ID correto ao consumidor. Usar fixture sem engine/integrações reais e credenciais isoladas; não abrir app completo sem isolamento demonstrado nem substituir o instalado. Guardar capturas e prova do destino textual. Sem prova segura, registrar `notProven` e o bloqueio, não marcar esta tarefa concluída.
- [ ] 4.3 Executar revisão pré-commit pelo `/Users/guilhermevarela/orchestrator.v5/scripts/review-pair.sh --repo <checkout> --base <base> --out <rodada-nova> --include-untracked --gate`; preservar scanner e snapshot exato. Tratar achados; indisponibilidade/incompletude comprovada exige contingência manual completa registrada, sem retry cego, `--agent` ou `--use-credits`. Registrar modalidade e summary reais antes do commit final.
- [ ] 4.4 Integrar somente esta change no fork conforme o fluxo nativo do projeto, com commits/revisão/checks identificados, sem sobrescrever WIP/targets de Projects worked. Verificar identidade da árvore integrada e registrar exatamente o destino atingido; não publicar tag/release nem instalar o app por inferência.

## Workflow follow-up

- Com os critérios cumpridos, arquivar pelo CLI e validar specs canônicas; não editar `openspec/specs/` manualmente.
- Executar sync dos AGENTS donos e cleanup de recursos próprios, preservando trabalho, targets, dados, perfis e recibos alheios. Informar remoções/retenções e espaço antes/depois sem atribuir variação concorrente integralmente à tarefa.
- A mudança Projects worked é entrega separada; esta change não autoriza refazê-la nem usar seus builds como prova das novas melhorias.

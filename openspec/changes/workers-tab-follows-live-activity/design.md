## Context

`ChatWorkersWidgetState` guarda `dispatch_counts` e `latest_started` por aba e decide foco em
`sync_dispatch_with_recency`, chamado a cada render de `render_chat_workers`. O snapshot já carrega
o estado de execução de cada linha (`ChatWorkerRow::semantic`, `ChatActivityRow::status`) — ele só
nunca foi oferecido à decisão de foco.

## Goals / Non-Goals

**Goals:**
- Foco acompanha o que está rodando agora, não só o que nasceu agora.
- Worker reativado e aba que esvazia são eventos de foco.
- Nada de relógio novo: os sinais saem do snapshot que o widget já renderiza.

**Non-Goals:**
- Travar a regra automática atrás de seleção manual (decisão do usuário: automático sempre vence).
- Usar `updated_at_unix_ms` como atividade — proibido pelo contrato, anda com heartbeat.
- Persistir foco entre chats ou entre sessões do app.
- Mexer em ordenação de linha, disclosure ou shimmer.

## Decisions

1. **Atividade é um conjunto de ids, não um timestamp.** Cada aba entrega os ids que estão rodando
   (`semantic.is_active()` para workers, `status == Running` para workflows/subagentes). "Passou a
   rodar" é diferença de conjunto contra o sync anterior. Isso pega o worker reativado, que nenhum
   timestamp pegaria: `created_at` é antigo e `updated_at` é inconfiável por contrato.
2. **Dois eventos, nessa ordem.** Primeiro *ganho* (linha nova, início mais novo, ou linha que
   passou a rodar) — a aba que ganhou toma foco. Só se ninguém ganhou, vale o *esvaziamento*: se a
   aba em foco não tem nada rodando e outra tem, o foco migra. Ganho antes de esvaziamento porque
   um lançamento novo é intenção do usuário; migrar é só não deixá-lo olhando lista morta.
3. **Ordem de desempate preservada** (Workflows → Workers → Subagents) nos dois eventos. Um worker
   mint a subagentes embaixo dele; a aba que o usuário quis é a que ele despachou.
4. **Esvaziamento avalia a aba *efetiva***, não `selected_tab`. Com seleção ainda `None` o foco vem
   de `auto_tab_by_recency`, e é essa aba que precisa ser testada por vazio — senão o primeiro
   worker a ficar ocioso não migraria enquanto ninguém tivesse clicado em nada.
5. **Conjunto ativo anterior é `Option`, igual às contagens.** Primeiro sync de um chat grava
   baseline e não decide nada; `sync_context` zera junto com o resto. Sem isso, abrir um chat com
   worker rodando leria como reativação e arrancaria a aba de quem só queria ler.

## Risks / Trade-offs

- Mais troca de aba automática do que hoje: toda retomada de worker move o foco. É o comportamento
  pedido, e a alternativa (só despacho) é justamente o defeito.
- O conjunto de ids ativos vive em memória por chat. São ids de sessão/subagente de uma conversa,
  não há custo relevante, e `sync_context` limpa na troca.
- Uma lista que pisca entre rodando/parado (flapping de `semantic`) troca a aba junto. Não há
  evidência disso hoje; se aparecer, o amortecimento vai no `semantic`, não aqui.

## Migration

Nenhuma. Estado só de memória, derivado do snapshot a cada render.

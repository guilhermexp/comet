# Change: Add Idle Session Recap with Exact Orchestrator Parity

## Why

No `orchestrator.dev` (commit `c54828fb`), chats nativos possuem um indicador de resumo automático (*"※ recap:"*) no widget Workspace da Details Sidebar que resume onde o trabalho parou após a sessão ficar ociosa por um tempo configurado (default 240s).

No Comet, ao navegar por múltiplos chats e projetos na sidebar, o usuário não tem nenhuma indicação rápida do último estado ou próxima ação daquele chat, exigindo rolar o transcript inteiro para relembrar o contexto.

Este change implementa o **Idle Session Recap** no Comet com **paridade exata** ao `orchestrator.dev`:
1. **Desacoplamento total da sessão:** Leitura pura do tail do transcript do `SessionDoc`, geração one-shot via harness com o modelo mais leve (`cheapest_model`, ex: Haiku/Flash/Mini), sem criar turnos, sem tocar no CRDT sincronizado e sem custo de contexto para o chat.
2. **Guarda de época (`epoch = transcript.len()`):** Um recap só é válido para a exata quantidade de mensagens em que foi gerado; qualquer novo turno invalida imediatamente a linha para não exibir contexto desatualizado.
3. **Persistência local durável:** Recaps sobrevivem entre reinicializações do app em `ui-settings.json` (`DetailsSidebarPreferences`), com teto de 50 entradas e expiração de 24 horas (`prune_idle_recaps`), de modo que reabrir o app exibe imediatamente onde cada sessão parou.
4. **Renderização no widget Workspace:** Posicionado exatamente abaixo da seção *"Projects worked"* no card Workspace do `DetailsSidebar`:
   `※ recap: <texto>` acompanhado de timestamp formatado (`HH:MM` ou data curta).

## Decisions

- **D-01 (Engine RPC):** Adiciona o método RPC `GenerateChatRecap` (`LOCAL_ONLY`, params `{ chat_id }`, reply `{ recap: Option<String> }`). A engine local é a dona da leitura do `SessionDoc` e da execução one-shot do modelo via `HarnessRegistry`.
- **D-02 (Pipeline de geração isolado):** Em `crates/engine/src/recap.rs`, `select_recap_transcript` filtra apenas texto de `user` e `assistant` (máx. 40 mensagens, teto de 6.000 chars, até 800 chars por mensagem com quebra em fronteira de palavra). `build_recap_prompt` fixa o mesmo idioma da conversa e impõe o contrato (< 40 palavras, 1-2 frases, foco em objetivo + tarefa atual + próxima ação). `validate_recap` remove preâmbulos de LLM (*"Sure:"*, *"Here is..."*), limpa markdown e trunca em 280 caracteres.
- **D-03 (Timer e Política no Frontend):** O timer de inatividade é controlado na UI (`crates/ui/src/details_sidebar/idle_recap.rs`). A política pura `evaluate_idle_recap` avalia `is_streaming`, `is_compacting`, `has_draft`, `message_count` e `delay_seconds`. Se o usuário começar a digitar no composer ou o chat iniciar streaming, o timer é cancelado e o recap de época desatualizada é limpo.
- **D-04 (Ordem de checagem contra race conditions de hidratação):** `evaluate_idle_recap` verifica `entry.epoch == message_count` *antes* de testar `message_count == 0` ou `has_draft`. Isso previne que a montagem da janela antes da hidratação do transcript apague recaps válidos persistidos no storage.
- **D-05 (Persistência e Retenção):** `idle_recaps: HashMap<String, IdleRecapEntry>` é adicionado a `DetailsSidebarPreferences` e persistido em `ui-settings.json`. `prune_idle_recaps` retém as 50 entradas mais recentes em janela de 24h e descarta datas futuras decorrentes de skew de relógio.
- **D-06 (Widget Workspace):** `crates/ui/src/details_sidebar/view.rs` renderiza `IdleRecapRow` dentro de `workspace_body` abaixo de `worked_section` com separador visual `border_t_1` e opacidade suave.

## Impact

- `crates/rpc`: Novo método `GenerateChatRecap`.
- `crates/engine`: Novo módulo `recap.rs` e dispatch no `rpc.rs`.
- `crates/ui`: Novo módulo `details_sidebar/idle_recap.rs`, extensão de `DetailsSidebarPreferences` e renderização no `view.rs`.
- `openspec/`: Specs para idle-recap.
- Zero impacto no protocolo de sync CRDT edge ou no schema dos docs.

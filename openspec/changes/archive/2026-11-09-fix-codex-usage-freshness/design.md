# Design

## Context

Ver `proposal.md` para o problema e `../../../WORK-QUEUE.md` para a ordem de execução. Diagnóstico de 2026-11-08, sem prompts, troca de login ou edição manual de credenciais/cache.

### Evidência reproduzida

App instalado: `1.0.18`; CLI Codex: `0.167.0`. Na imagem, duas contas têm `Week 74%` e `Week 96%`. Seus registros em `~/.zeron/agent-accounts/usage-cache.json` e no RPC vivo `ListAgentAccounts` contêm exatamente esses saldos:

| Conta (rótulo sanitizado) | Saldo retido | Último sucesso UTC | Consulta direta em 08/11 17:56:40Z |
|---|---|---|---|
| A, ativa | 74% | 06/11 08:03:43 | HTTP 200, 5% consumido em 604800 segundos, portanto 95% restante |
| B, salva/inativa | 96% | 02/11 08:02:56 | HTTP 401 |

Um refresh normal pelo RPC do próprio app (`forceUsage:true`, 08/11 17:59:30Z) manteve os dois snapshots. Ele respeita backoff e não tem filtro por conta. Não foi usado logout, reset de quota, limpeza de cache, troca de conta ou refresh OAuth manual.

### Causas confirmadas

**F1 — resposta válida rejeitada na versão instalada.** Em `v1.0.18:crates/engine/src/agent_accounts.rs:3617-3622`, `codex_usage` compara `body.account_id` diretamente com `slot.account_key` e produz `NoCredentials::Missing` quando divergem. Na mesma tag, `parse_codex_auth` (`4450-4475`) constrói essa chave como `user::workspace`, enquanto o ID da resposta representa workspace. A conta A tem credencial que responde 200, mas esse guard impede aceitar sua quota. O cache registra repetidas falhas de credencial ausente; não é prova de token ausente.

**F2 — credencial da conta B rejeitada pelo provedor.** O GET dessa conta retorna 401. A causa específica da rejeição não foi estabelecida; não presumir que logout, troca de conta ou renovação manual sejam necessários. Usar o fluxo de recuperação que o produto já oferece e conferir o resultado sem afetar a conta A.

**F3 — última leitura válida apresentada como atual.** `UsageEntry` conserva `usage` e `fetched_at` após falha, de forma intencional. O consumidor `crates/ui/src/details_sidebar/usage.rs:268-357` ignora `usage_fetched_at`/`usage_error` e oculta warning nas rows com quota. `details_sidebar/view.rs:2009-2046` atualiza o horário local de sucesso do RPC, mesmo que o snapshot contenha quota antiga.

Os campos necessários já existem em `crates/proto/src/entities.rs:1618-1638`. A aritmética de `plan_usage.rs:44-70` e da row é correta: fração consumida vira saldo restante. A [documentação oficial OpenAI](https://learn.chatgpt.com/docs/app-server#6-rate-limits-chatgpt) também distingue percentual consumido, duração da janela e buckets; ela não torna um snapshot antigo atual.

**Árvore de origem confirmada:** `guilhermexp/comet` remoto `main` e o commit da tag `v1.0.18` apontam para `8c5898efc5438be60978d8dfd74468df95469e38`, conforme leitura da API GitHub nesta auditoria. O checkout local observado está em `main@28aa8628cca0fc6c977a7eb1c89550466a06557e` e diverge dessa linha (merge-base `3c5bb601d4cda85607c3c6176f26b91be35d0543`). Seu driver não tem o mesmo guard nem todo o suporte de reset credits da release. Isso não prova que a release esteja corrigida. A execução deve partir de `8c5898efc5438be60978d8dfd74468df95469e38`, ou de seu sucessor remoto revalidado, preservando os dois históricos e sem sobrescrever ou reescrever a branch local compartilhada.

## Goals / Non-Goals

**Goals:** corrigir F1 com validação real de identidade, tornar F2/F3 observáveis e recuperáveis e provar independência das duas contas. Preservar histórico útil e compatibilidade dos campos existentes.

**Non-Goals:** redesign do painel, novo OAuth, consumo de reset credits, merge de upstream, atualização automática do app, troca da conta ativa, apagar o vault/cache ou alterar outras propostas desta fila.

## Decisions

### D1. Separar identidade local e identidade do provedor

Manter `account_key` composta para isolamento entre usuários do mesmo workspace. Validar o ID de resposta contra o workspace esperado dos tokens autenticados usados no request, com tratamento explícito de ausência/identidade não comprovável. Uma resposta realmente alheia falha sem atualizar nenhuma quota. Não remover o guard simplesmente, nem reduzir a chave composta ou alterar os arquivos de contas como workaround.

A prova de regressão tem de partir da implementação da linha da release, cobrir resposta válida/estrangeira e preservar reset credits. Não inferir a identidade de resposta pelo e-mail ou comparar chaves de formatos diferentes.

### D2. Reusar metadados e controles existentes

Propagar erro de coleta ao `AgentAccount` mesmo quando há janelas retidas; distinguir ausência real de credencial de resposta inválida. Usar os controles existentes de Accounts para recuperação. Uma consulta de Usage não troca a conta ativa e não replica credenciais entre perfis. Backoff e limite de probes continuam ativos; credencial nova permite a recuperação já prevista no owner.

Não implementar um renovador OAuth paralelo nem assumir que 401 pode ser resolvido só por nova consulta. A recuperação da conta B pode exigir ação do usuário pelo login existente; até isso ocorrer, o dado continua histórico e identificado.

### D3. Frescor deriva da medição, não do transporte

Derivação pura da row combina `usage_fetched_at`, `usage_error` e `now`. Falha conhecida torna o saldo histórico imediatamente; idade acima de cinco minutos também, mesmo sem erro textual. Cinco minutos é o orçamento conservador desta proposta, acima do refetch atual de 120 segundos; o tick de 30 segundos basta para reclassificar sem rede. Timestamp ausente ou futuro produz frescor desconhecido, sem presumir idade zero.

No resumo colapsado, substituir a aparência de saldo atual por estado textual curto, como `Out of date` ou `Sign-in required`, usando linguagem e tokens existentes. Na expansão, conservar o saldo anterior como `Last known`, horário da medição e motivo/ação. Um sucesso posterior da conta remove o estado histórico somente com quota e timestamp novos para aquela identidade. Não derivar saldo atual após reset a partir da contagem de tempo.

Usar `/impeccable` e o `DESIGN.md` vigente na execução: refinement do componente existente, sem novas cores literais, mudança de layout global ou destaque baseado somente em cor. Os testes devem cobrir o texto visível no estado colapsado e na expansão.

### D4. Prova por camadas, com duas contas

Estender as fixtures da engine para os IDs compostos, respostas inválidas, 401, recuperação e geração de credencial. Estender as derivações e fixtures de render da UI para quota histórica com erro, antiga sem erro, timestamp ausente/futuro e recuperação. Uma conta saudável e uma rejeitada devem coexistir no mesmo snapshot. Preservar a semântica de saldo e janela, incluindo `5% consumed → Week 95% remaining` na fixture.

Testes existentes a reaproveitar: `a_failure_keeps_the_last_good_usage_and_backs_off`, `a_rejected_token_reprobes_once_the_credentials_change`, `the_usage_cache_file_round_trips` e `warning_replaces_the_generic_summary_only_on_rows_without_quota` (a expectativa de esconder o erro com quota deve mudar).

## Risks / Trade-offs

- Corrigir a fonte errada por divergência local → fixar SHA/base da linha instalada e remota antes do worker; preservar trabalho local e reset credits.
- Aceitar conta errada ao corrigir F1 → testes de identidade estrangeira e de dois usuários no mesmo workspace; cache composto continua intacto.
- Estado stale virar ruído a cada refetch → orçamento de cinco minutos para idade, mas erro conhecido continua observável; não substituir todo o card por erro global.
- Falha de autenticação permanecer sem recuperação concluída → mostrar ação existente e manter histórico explícito; não alegar que a conta foi restaurada sem uma coleta bem-sucedida.
- Testes verdes serem confundidos com aparência verificada → separar fixtures seguras da observação visual nativa, conforme as regras do projeto; não lançar cópia dev sem seu isolamento autorizado.

## Migration Plan

Sem migração destrutiva de perfis/cache ou wire novo previsto. Implementar na linha correta do fork, verificar regressões e revisão, e publicar somente quando autorizado. Clientes sem timestamp tratam frescor como desconhecido. Reversão pelo Git não requer restaurar contas; não apagar os dados antigos para simular correção.

## Refinamento de compatibilidade após a revisão

A regra de freshness também alcança quotas saudáveis dos coletores Kimi, Antigravity, Grok e Cursor. Seus caches existentes passam a conservar o horário UTC da coleta e propagá-lo pelo usage_fetched_at já existente; leituras de cache mantêm o instante original. Isso preserva os outros providers sem inventar horário na chegada do RPC, adicionar consultas, fluxo OAuth ou alterar formatos de credenciais persistidas.

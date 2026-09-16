## Context

`account-usage-widget-visibility` (pendente, já implementado) trocou o widget de "quatro
providers fixos, conta ativa de cada" por "uma linha por conta visível", e junto disso
decidiu (decisão 6) manter `ProviderUsageState::NotSignedIn` no enum mas parar de
produzi-lo. O efeito colateral é que o widget deixou de ser o roster de providers do
device e virou uma lista de contas — que some inteira quando não há conta.

Este change preserva tudo daquele (toggle, ordem, uma linha por conta, expand por
account id) e reverte só a decisão 6.

## Goals / Non-Goals

**Goals:**
- Todo provider de `accounts::PROVIDERS` tem linha, sempre.
- O motivo real de uma linha vazia é legível na própria linha.
- Kimi volta a render quota com o payload atual do endpoint.

**Non-Goals:**
- Mexer em login, switch, forget ou nos meters da página Accounts.
- Novo campo em `AgentAccount`, RPC, CRDT ou snapshot.
- Sincronizar o set de ocultos entre devices.
- Renovar credencial Antigravity sem OAuth client (`COMET_ANTIGRAVITY_CLIENT_ID` /
  `_SECRET` continuam sendo o gate; sem eles a linha mostra o warning).

## Decisions

1. **Placeholder por provider, não por conta.** `provider_usage_rows` continua iterando
   `PROVIDERS`; quando o `flat_map` de um provider produz zero linhas, ele emite uma
   linha `NotSignedIn` com `account_id: None`. `render_usage_row` já chaveia por
   `account_id.unwrap_or(label)`, então não há colisão de id entre placeholders.
2. **Placeholder é para ausência de credencial, não para opt-out.** O gate é
   `provider_accounts(...).is_empty()` — a lista ANTES do filtro de ocultos. Provider sem
   credencial no device vira placeholder; provider cujas contas o usuário destoglou sai
   do widget inteiro. O toggle é ordem explícita do usuário e precisa ter efeito visível;
   um placeholder no lugar da linha faria ele parecer quebrado. O warning do harness sai
   junto: quem pediu para não ver o provider não pediu para ver o erro dele.
3. **Warning por harness, não por conta.** `AgentAccountWarning` é por harness. A linha
   só o exibe quando não tem janela nem linha local — um provider que já mostra quota
   não vira canal de erro. Com várias contas do mesmo harness sem quota, o mesmo warning
   repete: é o dado que a engine tem.
4. **`ProviderUsageRow.warning: Option<String>`** em vez de reaproveitar
   `weekly_summary`. O resumo é derivação de quota e continua tonalizado por
   `weekly_tone`; o warning é texto de falha e o render escolhe entre os dois.
5. **Kimi: `used = limit - remaining` quando `used` falta.** Mantém o mesmo walk de
   `usage` + `limits[].detail` e o mesmo `usage_row`, em vez de passar a ler o objeto
   novo `usages.{limit_5h,limit_7d}.used_ratio` — uma fonte só de verdade por janela,
   e o caminho antigo com `used` segue funcionando se voltar.

## Risks / Trade-offs

- Widget mais alto: um piso de uma linha por provider sem credencial. "Not signed in" é
  informação, não ruído — e o card Usage tem toggle próprio no gear do Details.
- O widget volta a poder ficar vazio (todo provider detectado destoglado), então o
  empty-state continua no render em vez de virar código morto.
- Derivar `used` de `remaining` confia que `limit` e `remaining` são a mesma unidade;
  `usage_row` já rejeita `limit <= 0` e clampa a fração em `[0,1]`.

## Migration

Nenhuma. `usage_widget_hidden_account_ids` continua sendo lido com o mesmo shape e com a
mesma semântica de remoção.

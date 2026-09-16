## Why

O widget Usage deriva sua membership só de contas detectadas **e** visíveis, então um
provider sem login device-local (Grok) ou com todas as contas destogladas (Antigravity,
neste device) simplesmente some da lista. Mas as credenciais que alimentam o widget vêm
do Mac — Keychain, `~/.codex`, `~/.kimi`, `~/.cli-proxy-api`, `~/.cursor`, `~/.grok` —
e não de nada que o Comet fez. Linha ausente lê como "o Comet perdeu meu provider",
não como "não há login". O `openspec/specs/{kimi,antigravity}-managed-usage` de main
ainda promete o oposto do que o código faz hoje ("reports Kimi as unavailable or not
signed in"): a divergência entrou com `account-usage-widget-visibility`, ainda não
arquivado.

Pior, quando a sonda falha a linha mostra "No usage yet", que nomeia a causa errada.
Neste device o probe do Kimi falha com payload inválido e o do Antigravity não consegue
renovar credencial — e os dois pintam igual a um provider saudável sem tráfego.

E o payload do Kimi realmente mudou: `https://api.kimi.com/coding/v1/usages` devolve
`{"limit":"100","remaining":"100","resetTime":…}` em `usage` e em `limits[].detail`,
enquanto `usage_row()` exige a chave `used`, que não existe mais. Zero janelas parseadas
→ `UsagePayload` → "No usage yet" com quota cheia.

## What Changes

- O widget Usage renderiza **sempre** todo provider da ordem de Settings → Accounts
  (Claude, Codex, Kimi, Antigravity, Cursor, Grok), no mínimo uma linha por provider,
  qualquer que seja o snapshot.
- Provider **sem conta detectada** rende uma linha placeholder `NotSignedIn`.
- O toggle por conta continua **removendo**: destogar a última conta de um provider tira
  o provider do widget, sem placeholder. As duas maneiras de ficar sem conta visível não
  são a mesma coisa — "não há credencial neste Mac" é informação, "eu desliguei isso" é
  ordem, e deixar placeholder no segundo caso faz o toggle parecer quebrado.
- Linha sem dado de usage mostra o warning da engine para aquele harness quando existe,
  em vez do genérico "No usage yet".
- O parse de usage do Kimi aceita `remaining` (o shape que o endpoint devolve hoje) além
  de `used`.

## Capabilities

### Modified Capabilities

- `usage-widget-freshness-and-tone`: provider sem conta detectada ganha placeholder;
  hidden continua removendo; warning do harness substitui o resumo genérico.
- `kimi-managed-usage`: placeholder de volta quando não há credencial; janela derivada
  de `remaining` quando `used` está ausente.
- `antigravity-managed-usage`: placeholder de volta quando não há credencial.

## Impact

- `crates/engine/src/kimi_usage.rs`: `usage_row` deriva `used` de `limit - remaining`.
- `crates/ui/src/details_sidebar/usage.rs`: `provider_usage_rows` emite placeholder por
  provider; `ProviderUsageRow` ganha `warning`.
- `crates/ui/src/details_sidebar/view.rs`: resumo usa o warning; o empty-state
  "No accounts in Usage" fica, alcançável quando todo provider detectado está destoglado.
- DOX: `crates/ui/AGENTS.md` (contratos de Usage/Accounts), `crates/engine/AGENTS.md`
  (parse do Kimi).

# zeron-proto — tipos de fio e derivações compartilhadas

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

O vocabulário que todo mundo fala: `AgentEvent` (incluindo `ToolCallPreview` não durável), `ToolCall`, `RunRequest`, `Model`, `FileToolInputSnapshot` sanitizado, entidades, snapshots device-local de usage (`AgentUsageWindow`/`AgentUsageLine`), contratos e projeções puras de Trajectory (`crates/proto/src/trajectory.rs`) e envelopes de RPC (serde, framing ndjson). Além dos tipos, os módulos `view` e `trajectory` guardam as **derivações puras** que UI e engine precisam concordar — ordenação, gating de staleness, agrupamento, boot gate, classificação em lanes (Input/Model/Tools), precedência de erros, timing modes (`Recorded` vs `SequenceOnly`) e reconciliação idempotente de deltas.

## Ownership

Crate-base do workspace. Não depende de nenhuma outra crate do repo — se você precisou importar algo daqui pra cima, o tipo está no lugar errado.

## Local Contracts

- Todo tipo que cruza processo (UI↔engine, engine↔engine via DeviceRoom, engine↔edge) mora aqui.
- Mudar shape de tipo serializado é **breaking cross-device**: dois devices em versões diferentes falam o mesmo fio. Campo novo entra opcional/`#[serde(default)]`; remoção exige change no OpenSpec.
- `view` e `trajectory` são puros: sem I/O, sem tokio, sem gpui. É o que permite testar as regras sem subir engine nem janela.
- Trajectory types e snapshots contêm apenas representações sanitizadas e referências opacas; nunca duplicam payloads brutos nem entram no Loro/sync. **Sanitizado aqui é derivado, não truncado**: todo summary/preview passa por redação de formatos de segredo (tokens `ghp_`/`sk-`/`xox*`, `AKIA`, `Bearer`, header `Authorization`, atribuições `password=`/`token=`/`api_key=`, blobs opacos ≥32 chars, userinfo em URLs e query params sensíveis como `key=`/`sig=`/`secret=`/`code=`), input de MCP/tool desconhecido vira nome dos argumentos + byte count (ou size unavailable se ausente), e conteúdo de arquivo e resultado de tool nunca entram. O texto cru só existe atrás do Raw Reveal, que lê o Run Journal.
- `TrajectoryStatus`/`TrajectoryLane` têm variante `Unknown` e `TrajectoryRecordKind` desconhecido cai em `Custom { name }` via shim untagged: linha gravada por build mais novo degrada um campo, não reprova o record inteiro na volta pra build antiga. `default_raw_source_version()` é o literal `1`, nunca a constante corrente — senão bumpar a versão reinterpreta silenciosamente ref antiga.
- A projeção pura de `group_records` identifica runs legadas pelo prefixo `legacy` do `run_id` e numera apenas runs não-legadas sequencialmente (`Run 1`, `Run 2`, etc.), já que o formato de `TrajectoryRecord` não carrega campo `is_legacy`.
- Tipos de usage são compatíveis por serde e cruzam apenas engine↔UI; não são persistidos em Loro nem sincronizados pelo edge.
- `HarnessId` também chaveia providers device-local de conta/Usage. Uma variante não torna um runtime executável — só o registry de harness da engine publica descritores runnable. Snapshots do Kimi carregam apenas campos normalizados de conta/quota, nunca material de credencial.

## Work Guidance

- Lógica de apresentação que a UI e a engine derivam do mesmo estado pertence a `view`, não a `zeron-ui` — duplicar ali é como o comportamento diverge entre headed e headless.

## Verification

- Comandos: `cargo test -p zeron-proto`

| Camada / path | Tier exigido | Como rodar |
| `src/view` (derivações puras) | unit | `cargo test -p zeron-proto` |
| `src/trajectory.rs` (contratos e projeções puras de Trajectory) | unit | `cargo test -p zeron-proto trajectory` |
| `src/**` (tipos serde) | unit — roundtrip de serialização quando o shape tem regra | `cargo test -p zeron-proto` |
## Child DOX Index

Sem filhos.

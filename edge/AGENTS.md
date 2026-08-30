# edge — Worker Cloudflare e Durable Objects

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

O único componente não-Rust do produto: Worker + **SessionRoom DO** (uma sala por chat, também usada pelo workspace doc `ws2/{orgId}`) + **DeviceRoom DO** (uma por device, relay de controle) + R2 para anexos + auth WorkOS (JWKS, troca de código, refresh, orgs). Absorve o que no comet original eram `apps/server` e o stack Postgres/Hono/WebRTC — todos eliminados.

## Ownership

Dono do estado que vive fora dos devices: salas, blobs e sessão de auth. Não é dono da forma do documento — essa é de `crates/doc`, e o materializador daqui segue ela.

## Local Contracts

- Decisão registrada em `docs/research/durable-objects-language.md`: **os DOs ficam em TypeScript**. Tudo device-side é Rust. Não portar DO pra Rust sem revisitar essa decisão.
- `edge/src/session-doc/` é o gêmeo do schema de `crates/doc` — nome e shape de container casam byte a byte. Mudou lá, muda aqui, no mesmo commit.
- Fala `loro-protocol` no fio; o lado Rust usa a crate oficial equivalente. Frames são idênticos por contrato, não por coincidência.
- Auth: nunca logar fingerprint, prefixo ou qualquer derivado de refresh token (`auth-routes.ts` já teve finding disso).
- `edge/dist/` é build gerado e ignorado — não confundir com a `dist/` da raiz, que é asset-fonte de packaging.
- **Push na `main` que toque `edge/` dispara deploy do Worker** via `.github/workflows/deploy.yml`. Mudança aqui é publicação, não só código.

## Work Guidance

- Convergência com o cliente Rust se prova em `crates/sync/tests/edge_convergence.rs`, não por inspeção.
- Config de runtime em `wrangler.jsonc` e `env.ts`; segredo nunca entra no repo.

## Verification

- Comandos: `npm -C edge run test` · `npm -C edge run typecheck` · `npm -C edge run dev` · `npm -C edge run smoke`

| Camada / path | Tier exigido | Como rodar |
|---|---|---|
| `src/*.test.ts` (device frame, liveness) | unit — vitest | `npm -C edge run test` |
| `src/{session-room,device-room}.ts` (DOs) | integration — convergência pelo lado Rust | `cargo test -p zeron-sync` |
| `src/{auth,auth-routes,workos}.ts` | integration | `npm -C edge run test` + `npm -C edge run smoke` |
| Tipos (todo o `src/**`) | typecheck obrigatório | `npm -C edge run typecheck` |

## Child DOX Index

Sem filhos. `src/session-doc/` segue o contrato de [`../crates/doc/AGENTS.md`](../crates/doc/AGENTS.md).

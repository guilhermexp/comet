# scripts — dev, smoke e packaging

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

Os quatro scripts que fazem o repo rodar fora do `cargo`: `dev-demo.sh` (demo local offline), `e2e-smoke.sh` (smoke multi-device), `package-linux.sh` e `package-macos.sh` (distribuição).

## Ownership

Donos do fluxo de dev e do artefato de release. Não contêm lógica de produto — se um script começou a decidir comportamento, o lugar é uma crate.

## Local Contracts

- `dev-demo.sh` sobe daemon com **harness mock seeded** — offline, determinístico. `--slow` mostra o streaming. É a superfície onde mudança visual se valida.
- Captura de UI (rota/dialog/picker/gate/upload fabricado) exige `ZERON_UI_CAPTURE=1` junto da knob: `ZERON_UI_CAPTURE=1 ZERON_OPEN_ROUTE=settings/agents cargo run`. Sem o umbrella a knob é ignorada de propósito — ela ficava exportada no shell e sequestrava todo run seguinte.
- `e2e-smoke.sh` é o smoke multi-device; roda contra engine real.
- Os scripts de packaging **consomem `dist/` da raiz**: `package-macos.sh` lê `dist/macos/Info.plist` e gera o iconset de `dist/comet.png`; `package-linux.sh` instala `dist/comet.desktop` e `dist/comet.png`. Apagar essa pasta quebra release sem quebrar build.
- macOS packaging depende de `sips` — só roda num Mac.
- O workflow `release.yml` (tag `v*`) espera artefato nomeado `comet-<versão>-*` dentro de `dist/`. Renomear artefato quebra o gate de nome no CI.

## Work Guidance

- Comando novo de dev vira script aqui e entra na tabela de comandos do `../AGENTS.md`.

## Verification

- Comandos: `bash -n scripts/<script>.sh` (sintaxe) · execução real do script

| Camada / path | Tier exigido | Como rodar |
|---|---|---|
| `e2e-smoke.sh` | e2e — é o próprio teste | `scripts/e2e-smoke.sh` |
| `dev-demo.sh` | none — ferramenta de dev; validação é usar | `scripts/dev-demo.sh` |
| `package-*.sh` | none — sem suite; validação é gerar o pacote e abrir | execução manual |

## Child DOX Index

Sem filhos.

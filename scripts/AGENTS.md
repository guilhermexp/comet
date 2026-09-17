# scripts — dev, smoke e packaging

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

Os scripts que fazem o repo rodar fora do `cargo`: `dev-demo.sh` (demo local offline), `e2e-smoke.sh` (smoke multi-device), `package-linux.sh`/`package-macos.sh` (distribuição), `omp-dev` (launcher de OMP para desenvolvimento) e `lint-text-alpha.sh` (contraste de texto sem alpha empilhado sobre papel do tema).

## Ownership

Donos do fluxo de dev e do artefato de release. Não contêm lógica de produto — se um script começou a decidir comportamento, o lugar é uma crate.

## Local Contracts

- `lint-text-alpha.sh` falha se `crates/ui/src/**/*.rs` pinta `text_color` com `theme.text` / `text_muted` / `text_faint` + `.opacity(...)` sem `// a11y-ok: <motivo>` na mesma linha ou na imediatamente acima. Só bash/grep/sed.

- `run-macos-browser-fixture.sh` recebe `target/debug/{browser,preview}-fixture` compilado com `cargo build -p zeron --features browser-fixture --bin <fixture>` e empacota somente a fixture local com o Info.plist real para validar política HTTP no macOS. Não assina nem publica; evidência nativa continua separada de unit tests.

- `dev-demo.sh` sobe daemon com **harness mock seeded** — offline, determinístico. `--slow` mostra o streaming. É a superfície onde mudança visual se valida. O rig também isola `UNPEEL_HOME` e semeia um Worker OMP vinculado ao primeiro Chat, com telemetria multi-modelo, para o widget Workers ter um estado visual reproduzível sem ler dados reais do usuário.
- `dev-demo.sh` deve continuar compatível com o Bash 3.2 do macOS; não usar arrays associativos (`declare -A`).
- Captura de UI (rota/dialog/picker/gate/upload fabricado) exige `ZERON_UI_CAPTURE=1` junto da knob: `ZERON_UI_CAPTURE=1 ZERON_OPEN_ROUTE=settings/agents cargo run`. Sem o umbrella a knob é ignorada de propósito — ela ficava exportada no shell e sequestrava todo run seguinte.
- `e2e-smoke.sh` é o smoke multi-device; roda contra engine real.
- Os scripts de packaging **consomem `dist/` da raiz**: `package-macos.sh` lê `dist/macos/Info.plist` e gera o iconset de `dist/macos/icon-1024.png`; `package-linux.sh` instala `dist/zeron.desktop` e `dist/zeron.png`. Apagar essa pasta quebra release sem quebrar build.
- macOS packaging depende de `sips` — só roda num Mac.
- **`omp-dev` é opt-in.** `cargo run` usa o OMP instalado, resolvido pelo harness; `.cargo/config.toml` não impõe um checkout de OMP. Para desenvolver capabilities locais, usar explicitamente `OMP_EXECUTABLE="$PWD/scripts/omp-dev" cargo run`. O wrapper entrega `../oh-my-pi` (ou `ZERON_OMP_SOURCE_DIR`) somente com CLI executável e `node_modules/@oh-my-pi/pi-natives`; caso contrário usa o `omp` instalado. O catálogo de modelos e as capabilities são os do runtime escolhido: o checkout local pode ter Live Voice e ainda assim estar atrasado nos modelos. Não reintroduzir override implícito, nem interpretar o binário novo do Comet como atualização do OMP.
- O workflow `release.yml` (tag `v*`) espera artefato nomeado `zeron-<versão>-*` dentro de `dist/`. Renomear artefato quebra o gate de nome no CI.

## Work Guidance

- Comando novo de dev vira script aqui e entra na tabela de comandos do `../AGENTS.md`.

## Verification

- Comandos: `bash -n scripts/<script>.sh` (sintaxe) · execução real do script

| Camada / path | Tier exigido | Como rodar |
|---|---|---|
| `e2e-smoke.sh` | e2e — é o próprio teste | `scripts/e2e-smoke.sh` |
| `dev-demo.sh` | none — ferramenta de dev; validação é usar | `scripts/dev-demo.sh` |
| `seed-demo-workers.py` | integration — fixture consumido pelo bootstrap real | `cargo test -p zeron-workers-unpeel --test dev_demo_fixture` |
| `package-*.sh` | none — sem suite; validação é gerar o pacote e abrir | execução manual |
| `lint-text-alpha.sh` | unit — grep do anti-padrão `text_color`+opacity nos papéis de texto | `bash scripts/lint-text-alpha.sh` |
| `omp-dev` | none — wrapper de 1 decisão; validação é o handshake | `printf '{"type":"ping","id":"x"}\n' \| scripts/omp-dev --mode rpc-ui --auto-approve --allow-home --cwd "$HOME" \| head -1` (espera `"type":"ready"`) |

## Child DOX Index

Sem filhos.

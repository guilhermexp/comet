# scripts — dev, smoke e packaging

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

Os scripts de execução, verificação e distribuição, incluindo `cargo-verify.py` (target temporário e limite de disco para testes Cargo locais), o runner nativo `cargo-macos-runner` (identidade macOS), `dev-demo.sh` (demo local offline), `e2e-smoke.sh` (smoke multi-device), `package-linux.sh`/`package-macos.sh` (distribuição) e `omp-dev` (launcher de OMP para desenvolvimento).

## Ownership

Donos do fluxo de dev e do artefato de release. Não contêm lógica de produto — se um script começou a decidir comportamento, o lugar é uma crate.

## Local Contracts

- `.cargo/config.toml` usa `cargo-macos-runner` somente no macOS. O headed `zeron` sem argumentos ou com URL `zeron://` executa de `Zeron.app` ao lado do artefato Cargo, com manifest/ícone de `dist/macos`. Isso evita que cmux reconheça o binário cru como instância duplicada do terminal e o encerre. O runner faz exec direto, preserva perfil, cwd, ambiente, stdio, argumentos e sinais; CLI, fixtures e testes passam diretamente. Não usa `open`, não intercepta SIGTERM e não altera OMP. Requer Python 3 e ferramentas macOS `sips`/`iconutil`.

- `cargo-verify.py` envolve verificações Cargo **locais no macOS/Linux** em um target descartável de caminho fixo (`target-verify`, para reutilização pelo sccache) e usa lock por usuário para não compilar em vários checkouts ao mesmo tempo. Deixa `CARGO_INCREMENTAL` sob controle do ambiente/perfil e aplica `taskpolicy -b` no macOS, salvo `COMET_CARGO_VERIFY_FOREGROUND=1`. Confere espaço livre antes e durante o comando, envia SIGINT/SIGTERM/SIGHUP ao grupo filho e limpa o target mesmo quando o comando falha. O grupo filho é liderado por um watchdog que o mata quando o pipe do wrapper fecha, inclusive por SIGKILL; a próxima execução, já com o lock, remove targets de wrappers mortos (com ou sem `.owner-pgid`). O lock recusa uma segunda verificação em vez de criar outro target. Uma rodada com várias suites deve ser um único comando `bash -c` dentro do wrapper para compartilhar o target enquanto a rodada está ativa. Não usar o wrapper para `cargo run` de desenvolvimento, pois esse app precisa do target incremental persistente. Não aplica a CI remota nem ao Windows.

- `run-macos-browser-fixture.sh` recebe `target/debug/{browser,preview}-fixture` compilado com `cargo build -p zeron --features browser-fixture --bin <fixture>` e empacota somente a fixture local com o Info.plist real para validar política HTTP no macOS. Não assina nem publica; evidência nativa continua separada de unit tests.

- `dev-demo.sh` sobe daemon com **harness mock seeded** — offline, determinístico. `--slow` mostra o streaming. É a superfície onde mudança visual se valida. O rig também isola `UNPEEL_HOME` e semeia um Worker OMP vinculado ao primeiro Chat, com telemetria multi-modelo, para o widget Workers ter um estado visual reproduzível sem ler dados reais do usuário.
- `dev-demo.sh` deve continuar compatível com o Bash 3.2 do macOS; não usar arrays associativos (`declare -A`).
- `seed-project-identity-demo.py --home <diretório-vazio>` cria repositórios Git e Workers de demonstração isolados para Workers/Projects. Recusa sobrescrever perfis existentes. `--remove-observed-checkout` remove somente o checkout criado pela fixture, depois de o app registrar sua identidade, para comprovar retenção de histórico. Não aponta para `~/.unpeel` real.
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
| `cargo-macos-runner` | integration + QA nativo: bundle, argumentos, cwd/env/stdio/status, CLI/test passthrough, rebuild e sinais | `python3 scripts/test-cargo-macos-runner.py` · `cargo run` no cmux |
| `cargo-verify.py` | integration — target descartado no sucesso/falha/sinal, filho morto junto com wrapper em SIGKILL, recusa por falta de disco e exclusão mútua entre verificações | `python3 scripts/test-cargo-verify.py` (sem compilar Rust) |
| `dev-demo.sh` | none — ferramenta de dev; validação é usar | `scripts/dev-demo.sh` |
| `seed-demo-workers.py` | integration — fixture consumido pelo bootstrap real | `cargo test -p zeron-workers-unpeel --test dev_demo_fixture` |
| `package-*.sh` | none — sem suite; validação é gerar o pacote e abrir | execução manual |
| `omp-dev` | none — wrapper de 1 decisão; validação é o handshake | `printf '{"type":"ping","id":"x"}\n' \| scripts/omp-dev --mode rpc-ui --auto-approve --allow-home --cwd "$HOME" \| head -1` (espera `"type":"ready"`) |

## Child DOX Index

Sem filhos.

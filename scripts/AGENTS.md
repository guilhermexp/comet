# scripts — dev, smoke e packaging

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

Os scripts de execução, verificação e distribuição, incluindo `cargo-verify.py` (target temporário e limite de disco para testes Cargo locais), o runner nativo `cargo-macos-runner` (identidade macOS), `ui-sharpness-monitor.py` / `ui_sharpness.py` (observação nativa e classificação relativa de nitidez), `dev-demo.sh` (demo local offline), `e2e-smoke.sh` (smoke multi-device), `package-linux.sh`/`package-macos.sh` (distribuição) e `omp-dev` (launcher de OMP para desenvolvimento).

## Ownership

Donos do fluxo de dev e do artefato de release. Não contêm lógica de produto — se um script começou a decidir comportamento, o lugar é uma crate.

## Local Contracts

- `ui-sharpness-monitor.py` observa somente uma janela Zeron já aberta: não lança o app, não altera providers, preferências ou renderer e não envia capturas à rede. CoreGraphics identifica/valida a janela e `screencapture` produz PNG sem perdas; metadata registra bounds e razão observada entre pixels e pontos, não `contentsScale` interno nem contagem de repaints. Pillow/NumPy são dependências explícitas, sem instalação automática. `--analyze` reprocessa PNGs sem APIs nativas. O detector separa contraste, movimento/conteúdo e candidatos a blur espacial; pixels estáveis não comprovam ausência de desfoque constante. Enter marca incidente percebido. Logs e imagens ficam locais, privados e limitados por duração, disco e orçamento; não sobrescrever uma rodada existente. Uso e limites em [`../docs/ui-sharpness-monitor.md`](../docs/ui-sharpness-monitor.md).

- `.cargo/config.toml` usa `cargo-macos-runner` somente no macOS. O headed `zeron` sem argumentos ou com URL `zeron://` executa de `Zeron.app` ao lado do artefato Cargo, com manifest/ícone de `dist/macos`. Isso evita que cmux reconheça o binário cru como instância duplicada do terminal e o encerre. O runner faz exec direto, preserva perfil, cwd, ambiente, stdio, argumentos e sinais; CLI, fixtures e testes passam diretamente. Não usa `open`, não intercepta SIGTERM e não altera OMP. Requer Python 3 e ferramentas macOS `sips`/`iconutil`.

- `cargo-verify.py` envolve verificações Cargo **locais no macOS/Linux** em um target descartável de caminho fixo (`target-verify`, para reutilização pelo sccache) e usa lock por usuário para não compilar em vários checkouts ao mesmo tempo. Deixa `CARGO_INCREMENTAL` sob controle do ambiente/perfil e aplica `taskpolicy -b` no macOS, salvo `COMET_CARGO_VERIFY_FOREGROUND=1`. Confere espaço livre antes e durante o comando, envia SIGINT/SIGTERM/SIGHUP ao grupo filho e limpa o target mesmo quando o comando falha. O grupo filho é liderado por um watchdog que o mata quando o pipe do wrapper fecha, inclusive por SIGKILL; a próxima execução, já com o lock, remove targets de wrappers mortos (com ou sem `.owner-pgid`). O lock recusa uma segunda verificação em vez de criar outro target. Uma rodada com várias suites deve ser um único comando `bash -c` dentro do wrapper para compartilhar o target enquanto a rodada está ativa. Não usar o wrapper para `cargo run` de desenvolvimento, pois esse app precisa do target incremental persistente. Não aplica a CI remota nem ao Windows.

- `run-macos-browser-fixture.sh` recebe `target/debug/{browser,preview}-fixture` compilado com `cargo build -p zeron --features browser-fixture --bin <fixture>` e empacota somente a fixture local com o Info.plist real para validar política HTTP no macOS. Não assina nem publica; evidência nativa continua separada de unit tests.

- `dev-demo.sh` sobe daemon com **harness mock seeded** — offline, determinístico. `--slow` mostra o streaming. É a superfície onde mudança visual se valida. O rig também isola `UNPEEL_HOME` e semeia um Worker OMP vinculado ao primeiro Chat, com telemetria multi-modelo, para o widget Workers ter um estado visual reproduzível sem ler dados reais do usuário.
- `dev-demo.sh` deve continuar compatível com o Bash 3.2 do macOS; não usar arrays associativos (`declare -A`).
- `seed-project-identity-demo.py --home <diretório-vazio>` cria repositórios Git e Workers de demonstração isolados para Workers/Projects. Recusa sobrescrever perfis existentes. `--remove-observed-checkout` remove somente o checkout criado pela fixture, depois de o app registrar sua identidade, para comprovar retenção de histórico. Não aponta para `~/.unpeel` real.
- Captura de UI (rota/dialog/picker/gate/upload fabricado) exige `ZERON_UI_CAPTURE=1` junto da knob: `ZERON_UI_CAPTURE=1 ZERON_OPEN_ROUTE=settings/agents cargo run`. Sem o umbrella a knob é ignorada de propósito — ela ficava exportada no shell e sequestrava todo run seguinte.
- `e2e-smoke.sh` é o smoke multi-device; roda contra engine real.
- Os scripts de packaging **consomem `dist/` da raiz**: `package-macos.sh` lê `dist/macos/Info.plist` e gera o iconset de `dist/macos/icon-1024.png`; `package-linux.sh` instala `dist/zeron.desktop` e `dist/zeron.png`. Apagar essa pasta quebra release sem quebrar build.
- `ios/build-core.sh` respeita `CARGO_TARGET_DIR` fornecido (inclusive target descartável do cargo-verify), mantém limite de jobs e copia a lib/header para `target/ios-core` do checkout para linking do Xcode. Preserva `RUSTC` explícito para selecionar um toolchain com a stdlib iOS instalada; o ambiente limpo continua excluindo SDKROOT do Xcode das builds host. Não força `CARGO_INCREMENTAL`: conserva o override explícito ou deixa o perfil decidir. Se o sccache local rejeitar incremental do perfil mobile, usar `CARGO_INCREMENTAL=0 scripts/ios/build-core.sh iphonesimulator` dentro do wrapper; isso não dispensa o build/linking nem muda o default do app. As bindings Swift são regeneradas e normalizadas apenas em whitespace final antes de comparar; não editar o arquivo gerado à mão.
- Voz Codex usa o helper da instalação standalone do usuário; os pacotes do app não embutem Codex/GStreamer. `package-voice-runtime.py` é uma projeção de desenvolvimento de versão/checksum fixos e requer Python 3.12+; fixtures offline não precisam de provider nem áudio. `codex-voice-remote-smoke.py` exige opt-in `--live` antes de iniciar qualquer transporte; não rodar chamadas reais por padrão.
- macOS packaging depende de `sips` — só roda num Mac.
- Assinatura de release e dev usa `dist/macos/Zeron.entitlements`, sem sandbox
  de arquivos: arquivos dropped de qualquer pasta permanecem anexáveis. O
  entitlement de microfone cobre ditado e Codex Voice; não restaurar os antigos
  `Dictation.entitlements`/`voice.entitlements`. Templates de Info.plist optam
  fora do AutoFill de código de uso único no macOS.
- `package-macos.sh` fora de CI (`CI` vazio) instala o bundle gerado em `/Applications/Zeron.app` sem abri-lo: encerra a cópia instalada em execução (quit por Apple Event, como ⌘Q, com até 60s para drenar a engine/salvar arquivos; depois SIGTERM, que no app headed é parada abrupta; desiste com erro se ela não sair, nunca SIGKILL), copia para `/Applications/.Zeron.app.installing` e só então troca a instalação anterior, re-registra no LaunchServices e não roda `open`. `ZERON_PACKAGE_INSTALL=0` pula; `=1` força em CI. Não toca `Zeron Dev.app` nem o bundle de `cargo run`. Sob o `omp` o `osascript` do quit é morto, então ali vale o fallback SIGTERM. Deve copiar as licenças de `crates/ui/assets/fonts/licenses/` para o bundle, como os scripts de Linux/Windows — as fontes Geist vão embutidas no binário.
- **`omp-dev` é opt-in.** `cargo run` usa o OMP instalado, resolvido pelo harness; `.cargo/config.toml` não impõe um checkout de OMP. Para desenvolver capabilities locais, usar explicitamente `OMP_EXECUTABLE="$PWD/scripts/omp-dev" cargo run`. O wrapper entrega `../oh-my-pi` (ou `ZERON_OMP_SOURCE_DIR`) somente com CLI executável e `node_modules/@oh-my-pi/pi-natives`; caso contrário usa o `omp` instalado. O catálogo de modelos e as capabilities são os do runtime escolhido: o checkout local pode ter Live Voice e ainda assim estar atrasado nos modelos. Não reintroduzir override implícito, nem interpretar o binário novo do Comet como atualização do OMP.
- O workflow `release.yml` (tag `v*`) espera artefato nomeado `zeron-<versão>-*` dentro de `dist/`. Renomear artefato quebra o gate de nome no CI.

- The native Cargo runner disables app replacement with `ZERON_AUTO_UPDATE=0`, including direct exec; `run-macos-dev.sh` and Info-dev.plist carry the same policy. Demo fanout seeds coexist with the fork Worker fixture, isolated profile/port overrides and Bash 3.2-compatible scalar project IDs. Demo folders default to temporary paths, never real project folders.

## Work Guidance

- Comando novo de dev vira script aqui e entra na tabela de comandos do `../AGENTS.md`.

## Verification

- Comandos: `bash -n scripts/<script>.sh` (sintaxe) · execução real do script

- **QA nativo por lançamento de uma cópia do app está indisponível até existir isolamento comprovado das configurações de todos os providers. Não repetir essa receita.** `ZERON_DATA_DIR`, `ZERON_IPC_PORT`, `UNPEEL_HOME` e `COMET_WORKERS_HOOKS_DIR` exclusivos separam dados/IPC e assets de hooks, mas não redirecionam as configurações reais sob o home. O bootstrap de `LocalWorkersClient` e a migração de hooks podem modificá-las antes de qualquer interação, mesmo sem iniciar agentes reais. Copiar/renomear/assinar o bundle, usar harness mock, fazer backups ou comparar digest/mtime depois não constitui isolamento. Não abrir `/Applications/Zeron.app` como alternativa; registrar a observação visual como `notProven`.
- O incidente de QA de 2026-10-11 expôs configurações de Claude, Codex, Cursor, Gemini, Kiro e KimiCode. Os recibos `native-qa-observations.json`, `native-qa-additional-config-repairs.json` e `native-qa-resource-cleanup.json` da rodada sync-20261011 registram restauração exata somente dos três arquivos previamente capturados (`~/.claude/settings.json`, `~/.codex/hooks.json`, `~/.codex/config.toml`) e reparos semânticos de caminhos pertencentes ao QA em sete configurações adicionais; não provam restauração byte a byte dessas sete. Os processos de QA foram encerrados, o bundle desregistrado e a cópia/perfil removidos. Esses recibos documentam recuperação e limites, não autorizam novo lançamento.

| Camada / path | Tier exigido | Como rodar |
|---|---|---|
| `e2e-smoke.sh` | e2e — é o próprio teste | `scripts/e2e-smoke.sh` |
| `test-browser-storage.py` | integration — WebKit real, HTTP sintético, cookies/IndexedDB/localStorage, reabertura, perfis isolados, lock e permissões | `xvfb-run -a python3 scripts/test-browser-storage.py --helper <zeron-webkit>` (Linux) · `python3 scripts/test-browser-storage.py --fixture <browser-fixture>` (fixtures Linux/macOS isoladas) |
| `cargo-macos-runner` | integration + QA nativo: bundle, argumentos, cwd/env/stdio/status, CLI/test passthrough, rebuild e sinais | `python3 scripts/test-cargo-macos-runner.py` · `cargo run` no cmux |
| `cargo-verify.py` | integration — target descartado no sucesso/falha/sinal, filho morto junto com wrapper em SIGKILL, recusa por falta de disco e exclusão mútua entre verificações | `python3 scripts/test-cargo-verify.py` (sem compilar Rust) |
| `ui_sharpness.py` / `ui-sharpness-monitor.py` | unit + integration — Gaussian conhecido, contraste afim, movimento/conteúdo, PNGs reais, marcas, limites, perda do alvo e relatório; smoke nativo observa apenas o app já aberto | `python3 -m unittest discover -s scripts/tests -p 'test_ui_sharpness*.py' -v` · `python3 scripts/ui-sharpness-monitor.py --duration 5` |
| `dev-demo.sh` | none — ferramenta de dev; validação é usar | `scripts/dev-demo.sh` |
| `seed-demo-workers.py` | integration — fixture consumido pelo bootstrap real | `cargo test -p zeron-workers-unpeel --test dev_demo_fixture` |
| `package-voice-runtime.py` / `codex-voice-remote-smoke.py` | integration — inventário/checksum/plataforma, protocolo, timeout e cleanup offline | `python3.12 -m unittest discover -s scripts/tests -p 'test_voice_packaging.py'` · `python3.12 -m unittest discover -s scripts/tests -p 'test_voice_remote*.py'` |
| `package-*.sh` | none — sem suite; validação é gerar/inspecionar o pacote e instalação; no macOS, lançamento da cópia para QA está indisponível conforme limite acima | execução manual |
| `omp-dev` | none — wrapper de 1 decisão; validação é o handshake | `printf '{"type":"ping","id":"x"}\n' \| scripts/omp-dev --mode rpc-ui --auto-approve --allow-home --cwd "$HOME" \| head -1` (espera `"type":"ready"`) |

## Child DOX Index

Sem filhos.

`ci/test-core.sh` includes `zeron-visual`, `zeron-visual-gpu` and `zatex` unit targets alongside the existing core packages. The UI workflow path selectors include these new shared dependencies for core/native/iOS; a green recipe is not proof until the exact revision runs.

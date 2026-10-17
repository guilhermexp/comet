# Tasks

Cargo local sempre por `scripts/cargo-verify.py -- <comando>`, agrupando os
testes de uma rodada numa invocação.

## 1. Servidor `comet-computer-use`

- [x] 1.1 Criar `crates/computer-use-mcp` (stdio JSON-RPC: `initialize`, `tools/list`, `tools/call`, cancel) e despachar `__computer_use_mcp__` antes do clap em `apps/zeron/src/main.rs`; verificar com `printf` de `initialize` + `tools/list` no binário de debug respondendo JSON válido.
- [x] 1.2 Resolução do runtime (D5): versão maior por comparação numérica, `cua_repl` do `.mcp.json`, env com `CUA_REPL_ENABLED_SURFACES=computer`, runtime ausente → lista vazia + stderr; verificar com testes unitários cobrindo `26.915.31029` vs `26.1007.71326`, pasta ausente e `.mcp.json` sem `cua_repl`.
- [x] 1.3 Gate do setting (D2): leitura de `codexComputerUseEnabled` e `codexComputerUseScope` em `{data_dir}/ui-settings.json` (ausente → ligado, `false` → desligado, malformado → desligado; escopo ausente → `chatsAndWorkers`; `chats` + `--worker` → desligado) aplicada em `tools/list` e em cada `tools/call`; verificar com testes unitários desses casos e de chamada recusada sem spawn do runtime, com mensagem distinta para "desligado" e "limitado a chats".
- [x] 1.4 Cliente do `cua_repl` e as tools `computer_use`/`computer_use_reset` (D5): preâmbulo uma vez por chave de sessão, aviso de reinício, mime pelos bytes, screenshot em disco com path no texto, timeout que mata e permite nova chamada; verificar com teste de integração contra um `cua_repl` falso em `tests/fixtures` (script que responde `js`/`js_reset`, rótulo PNG com bytes JPEG, morre sob comando).
- [x] 1.5 Descrição da tool com a API do runtime, regras de erro, regra do app de iPad e regra de concorrência; verificar com teste que `tools/list` traz `lsappinfo info -only bundlepath` na descrição de `computer_use`.
- [x] 1.6 Teste live `#[ignore]` contra o ChatGPT.app instalado (`tools/list` com duas tools, `cua.getState()` com apps) e `crates/computer-use-mcp/AGENTS.md` com as suposições sobre o runtime externo; verificar rodando o teste com `--ignored` nesta máquina.

## 2. Botão em Settings

- [ ] 2.1 Campos `codex_computer_use_enabled` (default `true`, `codexComputerUseEnabled`) e `codex_computer_use_scope` (default `ChatsAndWorkers`, `codexComputerUseScope`) em `UiSettings` e setters no `SettingsStore`; verificar com teste de round-trip: arquivo sem as chaves carrega `true`/`ChatsAndWorkers`, valores salvos voltam iguais, escopo desconhecido cai no default.
- [ ] 2.2 Card "Computer Use" em Settings → Agents com switch acessível e seletor "Só Comet" / "Comet + Workers" inativo com o switch desligado (D6); verificar no app rodando: alternar os dois, reabrir o app e ver o estado persistido em `~/.zeron/ui-settings.json`.

## 3. Chats (Claude, Pi, ACP)

- [ ] 3.1 `RunRequest.enable_computer_use_mcp` (`#[serde(default)]`) e carimbo `true` nos mesmos pontos de chat de agente que hoje põem `enable_workers_mcp: true` (composer, spawn de chat filho/side, `doc_host`); títulos, recap, commit message e voz ficam `false`; verificar com os testes existentes de `titles.rs`, `recap.rs`, `commit_message.rs` e `voice_chat.rs` afirmando `false`.
- [ ] 3.2 `servers_for` acrescenta `comet-computer-use` (com `ZERON_DATA_DIR` e `COMET_COMPUTER_USE_SESSION` no env) quando o campo é verdadeiro, e `codex/mod.rs` filtra esse servidor; verificar com testes em `workers_mcp.rs` (Claude/ACP/Pi contêm, request interno não contém) e em `codex/mod.rs` (overrides sem `mcp_servers.comet-computer-use`).
- [ ] 3.3 Atualizar `crates/harness/AGENTS.md` (lista de servidores e exceção do Codex); verificar lendo a seção de `workers_mcp.rs` revisada.

## 4. Chat OMP

- [ ] 4.1 `WorkersBridge::start_computer_use` (deadline 330 s), registro das tools como host tools e ramo próprio no roteamento por nome e no cancel em `omp/mod.rs`; verificar com teste em `tests/omp_rpc.rs` com servidor falso: `computer_use` chega ao servidor certo e não ao `zeron`/workers.
- [ ] 4.2 Com botão desligado, OMP não registra as tools; verificar no mesmo teste com o gate desligado (lista vazia → nenhum host tool novo).

## 5. Workers

- [ ] 5.1 Provar a ponte MCP de `crates/harness/src/pi/mcp.mjs` dentro do `omp` e do `pi` atuais (registro de tool e resultado com imagem) apontando para `zeron __computer_use_mcp__`; se o omp divergir, criar a variante do asset; verificar com `omp -p` e `pi -p` chamando `computer_use` com `cua.getState()` e mostrando a saída.
- [ ] 5.2 Adapters pi-family e `pi` carregam a ponte por `--extension` (asset instalado pelo `install_runtime_support`), apontando para `zeron __computer_use_mcp__ --worker`; verificar com teste de `startup_command` e com `setup_conformance_tests.rs`.
- [ ] 5.3 Adapters claude-code, cursor-agent, cline e kimi escrevem `comet-computer-use` (args `["__computer_use_mcp__", "--worker"]`) ao lado de `unpeel`, mantendo "config do usuário lança intocado"; codex sem mudança; verificar com os testes de cada adapter e `setup_conformance_tests.rs`.
- [ ] 5.4 Atualizar `third_party/unpeel-upstream.toml` e `crates/workers-unpeel/AGENTS.md` (seam de MCP por adapter e o segundo servidor); verificar lendo os dois arquivos no mesmo commit das mudanças vendorizadas.

## 6. Prova ponta a ponta

- [ ] 6.1 No app de dev com ChatGPT.app instalado: chat Claude, chat OMP, Worker claude e Worker omp chamam `computer_use` com `cua.getApp("Finder")` e devolvem a árvore e um screenshot salvo em disco; registrar a saída de cada um.
- [ ] 6.2 Desligar o botão: chat novo e Worker novo não listam as tools; sessão já aberta recebe a recusa citando Settings; religar e ver as tools voltarem em sessão nova; registrar a evidência.
- [ ] 6.3 Seletor em "Só Comet": chat novo lista as tools, Worker novo não lista, Worker já aberto recebe a recusa citando o alcance; voltar para "Comet + Workers" e ver o Worker novo listar; registrar a evidência.
- [ ] 6.4 Chat Codex e Worker codex continuam com o Computer Use nativo e sem `comet-computer-use`; registrar a lista de tools/MCP de cada um.

## 7. Encerramento local

- [ ] 7.1 Desinstalar `omp-codex-computer-use` do omp desta máquina (`omp plugin uninstall omp-codex-computer-use`) depois de 6.1 e 6.2; verificar que um chat OMP novo lista `computer_use` e não lista `codex_computer_use`.

## Workflow follow-up

- `/requesting-code-review` antes do merge.
- Arquivar a change com `/openspec-archive-change`, rodar `/sync` e `/cleanup`.

# Design

## Context

Motivação em `proposal.md`. Estado atual que molda o desenho:

- **Chats.** `crates/harness/src/workers_mcp.rs::servers_for` monta os
  servidores do Comet a partir do `RunRequest` e cada harness os traduz para o
  próprio formato: Claude `--mcp-config` (`claude/mod.rs:144`), Codex
  `-c mcp_servers.*` (`codex/mod.rs:144`), ACP `mcpServers` (`acp/mod.rs:70`),
  Pi uma extensão-ponte por servidor (`pi/mod.rs:371`, `pi/mcp.rs`). OMP não
  usa `servers_for`: `omp/workers_bridge.rs` sobe cada servidor, registra as
  tools como host tools e `omp/mod.rs` (~1063-1076) roteia por nome
  (`sessions` → bridge sessions; `zeron.serves(name)` → zeron; resto →
  workers). Hoje todo servidor do Comet depende de grant
  (`enable_workers_mcp` ou `sessions`). Runs internos (títulos, recap, commit
  message, voz) nascem com `enable_workers_mcp: false`.
- **Workers.** O Worker é a CLI do preset rodando sob um Session Host do
  Unpeel. O único ponto que monta argv/env é `session_host::run_host`; o
  acréscimo por runtime sai de `integrations::startup_command` /
  `configure_host_command` / `prepare_runtime_launch`, um adapter por runtime
  em `third_party/unpeel/runtimes/<pkg>/adapter`. Hoje esses adapters
  registram um servidor só, `unpeel` (`zeron __mcp__`), em claude-code,
  codex, cursor-agent, cline, kimi e kiro. omp e pi não recebem MCP nenhum.
  O Worker não chama `servers_for`. `third_party/unpeel` é vendorizado e se
  edita aqui, atualizando `third_party/unpeel-upstream.toml` no mesmo commit.
- **Domínio computer do Unpeel.** `unpeel-core/src/computer_mcp.rs` já tem um
  domínio `computer`, mas com outro motor (cua-driver servido por daemon do
  Unpeel.app nativo), que o Comet não sobe, e `spawn_session` grava
  `computer_mcp_enabled: false` sempre (`session_ops.rs:1621`).
- **Settings.** `UiSettings` (`crates/ui/src/settings.rs`) vive em
  `{data_dir}/ui-settings.json` (`$ZERON_DATA_DIR` ou `~/.zeron`,
  `apps/zeron/src/paths.rs`), gravado por temp + rename, com
  `#[serde(default)]` no struct. O engine não lê esse arquivo.
- **Runtime externo.** `~/.codex/plugins/cache/openai-bundled/unified-computer-use/<versão>/.mcp.json`
  declara `mcpServers.cua_repl` (`command`, `args`, `env`), binários dentro do
  ChatGPT.app. Ferramentas `js` e `js_reset`. A extensão do omp
  (`omp-codex-computer-use/index.ts`) é a referência de comportamento.

## Goals / Non-Goals

**Goals:**
- Uma implementação do proxy, um nome de servidor e um par de tools para
  chats e Workers.
- Um único ponto que decide se o Computer Use está ligado.
- Nenhuma escrita no config global de CLI do usuário além do que os adapters
  do Unpeel já fazem hoje.

**Non-Goals:**
- Reaproveitar ou ligar o domínio `computer` (cua-driver) do Unpeel. Fica
  como está, desligado.
- Gate por chamada (Off/Ask/Allow). O pedido é liga/desliga mais o alcance
  (só chats ou chats + Workers).
- Desligar o Computer Use nativo do Codex.
- Runtimes de Worker sem seam MCP.
- Empacotar o runtime: sem ChatGPT.app não há Computer Use.

## Decisions

### D1. Servidor próprio, não domínio do `unpeel __mcp__`

`zeron __computer_use_mcp__`, nome `comet-computer-use`, despachado antes do
clap em `apps/zeron/src/main.rs` como `__sessions_mcp__`. Código num crate
novo `crates/computer-use-mcp` (stdio JSON-RPC, cliente do `cua_repl`).

Alternativa: trocar o motor do domínio `computer` do `unpeel __mcp__` pelo
`cua_repl`. Daria Workers claude/cursor/cline/kimi sem mexer em adapter, mas o
`__mcp__` depende de manifest de Session Host (não serve chat), a superfície
do domínio é a do cua-driver (ações por janela, não REPL JS) e omp/pi
continuariam sem MCP. Teríamos dois caminhos para o mesmo motor.

### D2. O gate mora no servidor, não na montagem

O servidor é montado sempre (exceto Codex e runs internos). Ele lê o setting:
- em `tools/list`: desligado ou runtime ausente → lista vazia;
- em cada `tools/call`: desligado → resultado de erro, sem tocar o runtime.

Leitura: `{data_dir}/ui-settings.json`, só as chaves `codexComputerUseEnabled`
e `codexComputerUseScope` (struct mínimo com `Option`s, sem depender do crate
de UI). Ausente → ligado; `false` → desligado; arquivo existente mas
ilegível/malformado → desligado. Escopo ausente ou valor desconhecido →
`chatsAndWorkers`; `chats` → o servidor de um Worker age como desligado.

Origem: a montagem diz ao servidor de onde ele é. Chats passam só
`["__computer_use_mcp__"]`; Workers (adapters e ponte omp/pi) passam
`["__computer_use_mcp__", "--worker"]`. Assim o mesmo gate cobre o seletor sem
canal novo; o arg é estático e cabe nos configs fixos dos adapters. `data_dir` é resolvido pela mesma regra de `paths.rs`; quem monta
passa `ZERON_DATA_DIR` explícito no env do servidor quando o conhece, para o
filho não depender do env herdado.

Alternativa: carimbar um flag no `RunRequest` e no launch do Worker. Exigiria
o engine ler o setting da UI (hoje não lê), o Session Host receber o valor por
launch e os configs estáticos dos adapters (ex.: `claude-unpeel-mcp.json`)
virarem por sessão. Dois canais para um booleano. O custo do gate no servidor
é um processo ocioso por sessão com o botão desligado.

### D3. Quem é "run de agente" nos chats

Campo novo `RunRequest.enable_computer_use_mcp` (`#[serde(default)]`, falso no
fio), posto `true` onde hoje o engine/composer põe `enable_workers_mcp: true`
para chat de agente (composer, `SpawnChat`/side chat, `doc_host`). Títulos,
recap, commit message e voz continuam falsos. `servers_for` acrescenta o
servidor quando o campo é verdadeiro; `codex/mod.rs` filtra
`comet-computer-use` do que monta. OMP ganha `WorkersBridge::start_computer_use`
e um ramo próprio no roteamento por nome de tool e no cancel.

Alternativa: reaproveitar `enable_workers_mcp`. Acoplaria ao
`ZERON_DISABLE_WORKERS_MCP` e ao significado "pode controlar Workers".

### D4. Workers: segundo servidor nos adapters existentes + ponte omp/pi

- claude-code, cursor-agent, cline, kimi: onde o adapter escreve a entrada
  `unpeel`, escreve também `comet-computer-use` (mesmo executável, args
  `["__computer_use_mcp__"]`). As regras atuais de "comando do usuário com
  config MCP próprio lança intocado" valem igual.
- codex: nada.
- omp e pi: o adapter pi-family (e o de `pi`) acrescenta `--extension` com a
  ponte MCP. A ponte de chat Pi (`crates/harness/src/pi/mcp.mjs`) é a base;
  ela usa a API de extensão do Pi. Se a API do omp divergir no que a ponte usa
  (registro de tool, resultado com imagem), o asset ganha uma variante omp —
  a tarefa 5.1 decide pelo teste, não por suposição. O asset é instalado pelo
  `install_runtime_support` do adapter, ao lado da extensão de lifecycle.

### D5. Comportamento do proxy (porte da extensão do omp)

- Versão do plugin: maior por comparação numérica das partes separadas por
  `.`; nomes não numéricos perdem para numéricos.
- Env do `cua_repl`: o do `.mcp.json` mais `CUA_REPL_ENABLED_SURFACES=computer`.
- Chave de sessão para o estado durável (preâmbulo já enviado, último pid do
  `cua_repl`): `COMET_COMPUTER_USE_SESSION` (chat id, posto pela montagem do
  chat) → `UNPEEL_SESSION_ID` (Worker) → pid do servidor. Estado em
  `$TMPDIR/comet-computer-use/<chave>/state.json`; screenshots em
  `$TMPDIR/comet-computer-use/<chave>/shots/`.
- Preâmbulo: o bloco que o runtime manda começando em `## Computer Use` é
  removido de toda resposta após a primeira da sessão.
- Reinício: se o `cua_repl` morreu entre chamadas, a próxima resposta abre com
  a linha de aviso.
- Imagem: mimeType pelo magic number (PNG/JPEG/WebP/GIF).
- Timeout por chamada `timeout_ms` (padrão 30 s, teto 300 s); ao estourar,
  mata o `cua_repl` (o próximo call reinicia e avisa).
- Deadline do cliente: OMP bridge usa 330 s para este servidor; Claude já tem
  o `MCP_TOOL_TIMEOUT` elevado pelo Workers.
- Descrição da tool: superfície da API do runtime, regras que evitam erro,
  regra do app de iPad (`lsappinfo info -only bundlepath`) e a regra de
  concorrência (um agente por app alvo; sem `paste` em paralelo).

### D6. Settings

Toggle em Settings → Agents (`settings/harnesses.rs`), card próprio "Computer
Use" acima da lista de runtimes, no padrão do toggle de habilitar runtime
(`Role::Switch`, `aria_label`, Enter/Space). Descrição: requer ChatGPT.app e
permissão de Acessibilidade; não afeta o Codex; vale para sessões novas.
Campo `codex_computer_use_enabled: bool` em `UiSettings`, serializado como
`codexComputerUseEnabled`, `Default` = `true`.

Seletor de alcance (decisão do dono, 2026-10-17) logo abaixo do switch, no
padrão de segmented control já usado em Settings: "Só Comet" / "Comet +
Workers". Campo `codex_computer_use_scope: ComputerUseScope` (`Chats` |
`ChatsAndWorkers`, serde `chats` / `chatsAndWorkers`), serializado como
`codexComputerUseScope`, `Default` = `ChatsAndWorkers`. Com o switch desligado
o seletor fica esmaecido e não clicável; o valor salvo é mantido.

Escopo por máquina (decisão do dono, 2026-10-17): o servidor lê o arquivo do
host onde o agente roda, então o botão governa só as sessões daquele host; host
sem UI fica ligado. A descrição do card diz que vale para esta máquina e que
não afeta o Computer Use nativo do Codex.

Fora de escopo (decisão do dono): chats Cursor e OpenCode, e Workers kiro e
demais presets não usados.

## Risks / Trade-offs

- [O runtime é da OpenAI e muda sem aviso: formato do `.mcp.json`, nomes
  `js`/`js_reset`, preâmbulo] → teste live `#[ignore]` que roda o servidor
  contra o runtime instalado (lista tools, executa `cua.getState()`), mais o
  registro das suposições no AGENTS.md do crate.
- [Dois agentes no mesmo app ou colando ao mesmo tempo se atropelam] → regra
  na descrição da tool; não há lock entre processos.
- [Computer Use não passa pela guarda de shell] → aceito pelo usuário; o
  botão é a forma de desligar.
- [Processo ocioso por sessão com o botão desligado] → aceito em troca de um
  gate só (D2).
- [Ponte omp pode não servir com a API atual do omp] → tarefa 5.1 prova antes
  de ligar o adapter.
- [Durante a transição, OMP vê `codex_computer_use` (extensão) e
  `computer_use` (Comet)] → nomes diferentes, sem colisão; a extensão sai na
  tarefa 7.1.

## Migration Plan

1. Entra desligável e ligado por padrão; nada a migrar em disco (campo
   ausente = ligado).
2. Depois de provado em chat OMP e Worker OMP, desinstalar
   `omp-codex-computer-use` do omp local.
3. Rollback: desligar o botão; em último caso reverter o commit.

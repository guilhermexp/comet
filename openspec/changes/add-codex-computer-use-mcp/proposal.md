# Proposal

## Why

O Computer Use do Codex (o runtime `cua_repl` que o ChatGPT.app instala) só
chega hoje aos agentes OMP, por uma extensão pessoal do omp
(`omp-codex-computer-use`). Chats Claude/Pi/ACP e Workers não têm a tool, embora
ela sirva a qualquer agente: abrir app, ler a árvore de acessibilidade, tirar
screenshot e clicar sem roubar o foco do usuário. O Comet já monta servidores
MCP próprios em todos os runtimes; o Computer Use deve entrar pelo mesmo
caminho, ligado por padrão e com um botão em Settings para desligar.

## What Changes

- Novo servidor MCP embutido no binário, `zeron __computer_use_mcp__`
  (`comet-computer-use`), que faz proxy para o `cua_repl` do ChatGPT.app e
  expõe duas tools: `computer_use` (roda JavaScript no runtime) e
  `computer_use_reset`. Traz para o Comet o que a extensão do omp já resolvia:
  preâmbulo de instruções do runtime uma vez por sessão, aviso quando o
  `cua_repl` reinicia, mimeType da imagem tirado dos bytes, screenshot gravado
  em disco com o path no resultado. Corrige a escolha da versão do plugin, que
  hoje ordena como texto (`26.915…` ganhava de `26.1007…`).
- Montagem em chats: Claude, Pi, ACP e OMP recebem o servidor. Codex não: ele
  já tem o Computer Use nativo pelo `~/.codex/config.toml` e segue o próprio
  config.
- Montagem em Workers: claude, cursor-agent, cline e kimi pelo seam MCP que o
  Unpeel já tem por adapter; OMP e Pi por uma extensão-ponte. Codex de fora,
  pelo mesmo motivo. Os demais Workers (gemini, grok, opencode, amp, agy,
  muse, kiro, github-copilot, prime-agent) ficam de fora por decisão do dono:
  não são usados aqui. O kiro até tem seam MCP (`~/.kiro/settings/mcp.json`),
  mas fica de fora pelo mesmo motivo.
- Fora de escopo nos chats: os runtimes Cursor e OpenCode, que hoje não recebem
  nenhum MCP do Comet. O uso principal é OMP.
- Botão "Computer Use" em Settings → Agents, ligado por padrão, persistido no
  `ui-settings.json` do device. Vale por máquina: cada host segue o próprio
  arquivo (sessões num host remoto seguem o botão daquele host; host sem UI
  fica ligado). O botão só liga/desliga o servidor do Comet; o Computer Use
  nativo do Codex não muda, e o texto do botão diz isso. Desligado: sessão nova não vê a tool, e chamada
  de sessão já aberta é recusada com mensagem clara.
- Junto do botão, um seletor de alcance com duas opções: "Só Comet" (só os
  chats) e "Comet + Workers" (padrão). Em "Só Comet", Workers novos não veem a
  tool e chamada de Worker já aberto é recusada. Com o botão desligado o
  seletor fica inativo.
- Sem ChatGPT.app (runtime ausente), o servidor não anuncia tool nenhuma.
- Depois que a versão do Comet estiver provada em chat e Worker OMP, a extensão
  `omp-codex-computer-use` sai do omp desta máquina, para não haver tool
  duplicada.

## Capabilities

### New Capabilities

- `codex-computer-use`: disponibilidade do Computer Use do Codex nos runtimes
  que o Comet lança (chats e Workers), o botão que liga e desliga, a resolução
  do runtime do ChatGPT.app e o contrato das tools `computer_use` /
  `computer_use_reset`.

### Modified Capabilities

(nenhuma)

## Impact

- Código novo: crate do servidor (proxy stdio MCP) e despacho pré-clap em
  `apps/zeron/src/main.rs`.
- `crates/harness`: `workers_mcp.rs` (`servers_for` ganha o servidor, exceto
  Codex), `omp/workers_bridge.rs` e `omp/mod.rs` (bridge e roteamento por nome
  de tool), `pi/mcp.rs` (sem mudança de forma; recebe mais um servidor).
- `third_party/unpeel` (vendorizado, editável aqui): adapters de claude-code,
  cursor-agent, cline e kimi registram o segundo servidor; adapter pi-family e
  pi carregam a extensão-ponte; `third_party/unpeel-upstream.toml` atualizado.
- `crates/ui`: campo novo em `UiSettings` e linha com toggle em
  `settings/harnesses.rs`.
- Dependência externa (não empacotada): ChatGPT.app com o plugin
  `openai-bundled/unified-computer-use` e a permissão de Acessibilidade do
  "Codex Computer Use.app".
- Fora do repo: desinstalar o plugin `omp-codex-computer-use` do omp local.

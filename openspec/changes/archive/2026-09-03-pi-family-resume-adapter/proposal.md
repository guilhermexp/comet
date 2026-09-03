## Why

Restart em um Worker `omp` ou `prime-agent` sobe um agente novo sem histórico,
enquanto `pi`, `claude`, `codex` e os outros 12 runtimes do catálogo vendorizado
retomam a conversa. Os dois runtimes sem resume são exatamente os dois presets
que o Comet pina como seus (`COMET_WORKERS_PRESET_V1_IDS`), e sem resume barato
ninguém para um Worker ocioso: hoje 36 de 39 session hosts na máquina de
desenvolvimento estão parados há mais de 20 h segurando ~4,3 GB porque parar
custa o contexto inteiro. Este é o pré-requisito da hibernação de Workers.

## What Changes

- O adapter compartilhado da família pi (`omp`, `prime-agent`) passa a
  registrar uma receita de resume, reaproveitando a do `pi` com os flags que
  os dois CLIs realmente aceitam (`--resume <id>`, `--continue`,
  `--session-dir`).
- Novas sessões da família pi nascem com armazenamento de sessão pinado por
  Worker (`--session-dir` gerenciado), para que `--continue` seja exato mesmo
  com vários Workers no mesmo diretório.
- Restart de um Worker da família pi retoma a conversa usando o id de provider
  já capturado pela extensão de lifecycle (`provider-session.json`), com
  fallback para `--continue` no diretório pinado.
- `omp` e `prime-agent` declaram as capabilities `resume` e `restart_agent`,
  ligando os botões Restart / Resume Agent já existentes na UI e a ação
  `Restart` já existente no frontier.
- Sessões criadas antes desta mudança, sem `--session-dir` pinado, retomam
  pelo id do marker quando ele existir; sem marker, ficam explicitamente como
  "reinício limpo", nunca como resume silencioso da conversa errada.

## Capabilities

### New Capabilities

- `pi-family-session-resume`: relançar um Worker `omp` ou `prime-agent`
  retoma a conversa anterior, com o mesmo contrato que os demais runtimes
  já cumprem.

### Modified Capabilities

Nenhuma.

## Impact

- `third_party/unpeel/runtimes/_shared/pi-family/adapter/` (nova receita de
  resume), `runtimes/omp/runtime.toml` e `runtimes/prime-agent/runtime.toml`
  (capabilities), `third_party/unpeel-upstream.toml` (proveniência do
  vendorizado, mesmo commit).
- Nenhuma mudança de wire, CRDT, edge ou protocolo do Host: as capabilities
  `restart` / `resume_agent` já trafegam em `WorkersSessionCapabilities`.
- Sessões existentes não são migradas; o comportamento delas muda só no
  próximo Restart.
- DOX: `crates/workers-unpeel/AGENTS.md` (contrato de presets e catálogo).

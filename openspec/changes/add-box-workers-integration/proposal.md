# Proposal

## Why

O Orquestrador já consegue operar a box e receber avisos por uma ponte externa, mas as sessões remotas não aparecem no widget de Workers nem abrem dentro do Comet. Uma integração opcional com Shipyard deve unir ferramentas, acompanhamento e terminal, sem depender de o agente registrar manualmente cada aviso.

## What Changes

- Adicionar a integração habilitável **Box / Shipyard**, desligada por padrão, com configuração e estado locais ao device host do Chat.
- Reutilizar SSH e a API autenticada existente do Shipyard: MCP por stdio remoto; eventos e terminal pela API privada, sem publicar portas ou instalar uma gateway nova.
- Disponibilizar o catálogo do MCP nativo da box aos harnesses que recebem MCP pelo Comet, incluindo OMP, por um adaptador que registra o Chat pai das sessões iniciadas. Não importar configurações ou credenciais dos agentes remotos.
- Exibir as sessões vinculadas ao Chat no widget e na lista de Workers com tag `box`, estado remoto real e acesso ao terminal dentro do Comet.
- Acrescentar **Cloud boxes** em Settings → Devices, separada de This device / Other devices: nome e tag `box`, sistema/arquitetura, conexão, última atualização, CPUs/carga, memória e disco. Oferecer acesso às sessões acompanhadas e à configuração da conexão, sem registrar a box como device sincronizado nem oferecer Retire.
- Persistir vínculo, cursor de eventos e entrega de notificações; recuperar acompanhamento após reconexão/reinício sem duplicar avisos.
- Separar workers remotos do runtime Unpeel local: nenhuma simulação de PID, manifesto, checkout local ou evidência de conclusão em arquivos locais.
- Desativar a integração ou fechar um terminal desconecta o cliente, mas não encerra sessões na box.
- Entregar em três fases: (1) MCP da box no Chat, workers com tag `box` e aviso ao Chat pai; (2) terminal remoto dentro do Comet; (3) Cloud boxes em Devices. Cada fase é utilizável sozinha.

## Capabilities

### New Capabilities

- `box-workers`: conexão privada opcional ao Shipyard, informações da máquina cloud em Devices, ferramentas MCP com contexto de Chat, identidade remota, lifecycle, notificações e terminal remoto.

### Modified Capabilities

- `workers-widget-interaction`: apresentar workers da box ao lado dos locais, com origem explícita, sem inferir sucesso de ociosidade ou desconexão, e abrir a sessão pelo backend correto.

## Impact

- `crates/proto`, `crates/engine`, `crates/harness`, `crates/ui` e `apps/zeron`: configuração, fronteira de execução, MCP, notificações, projeção e terminal. `crates/workers-unpeel` permanece dono apenas dos workers locais.
- Um adaptador Shipyard específico, não um SDK/marketplace genérico de plugins; implementação Rust dentro do Comet, com ativação de produto em Settings.
- A box precisa do acesso SSH existente e do Shipyard com MCP, sessões, turnos e journal. A descoberta de capacidades precede a ativação; não atualizar nem reconfigurar a box automaticamente.
- Preservar protocolos de workers locais e campos existentes; novos campos RPC são opcionais. Credenciais e registros da integração não entram no CRDT.
- Reaproveitar o caminho já em `main` do Computer Use (`add-codex-computer-use-mcp`): montagem via `enable_workers_mcp` no Chat, ao lado de `comet-computer-use`, sem reimplementar aquele proxy.

## Non-goals

Marketplace de plugins, nova gateway pública, instalação de agentes, login nos providers, importação de MCPs arbitrários dos agentes da box, adoção automática de sessões alheias, sincronização de workers entre devices, aplicação iOS, migração de worktrees ou alteração das políticas de hibernação dos workers locais. A integração não certifica que o trabalho foi entregue corretamente: término de turno e aceite do Orquestrador continuam distintos.

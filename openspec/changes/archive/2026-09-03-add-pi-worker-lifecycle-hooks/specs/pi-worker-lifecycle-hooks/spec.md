## Purpose

Garantir que um Worker `pi` reporte lifecycle por hook — como `omp`,
`prime-agent` e os demais runtimes hook-owned — e que a instalação dos hooks
gerenciados sobreviva a uma reinstalação limpa do CLI.

## ADDED Requirements

### Requirement: Worker `pi` reporta lifecycle por hook

O sistema SHALL lançar um Worker `pi` com a extensão de lifecycle da família pi
carregada, e SHALL declarar para o runtime `pi` as capabilities de lifecycle por
hook e de notificação de conclusão.

#### Scenario: Lançamento carrega a extensão uma vez
Test: unit — comando de startup do adapter `pi`.

- **WHEN** um Worker `pi` é lançado com o comando do preset
- **THEN** o comando efetivo carrega a extensão de lifecycle exatamente uma vez
- **AND** aplicar a preparação de novo sobre o comando já preparado não duplica
  o flag
- **AND** o resto do comando do usuário é preservado

#### Scenario: Início e fim de turno chegam como evento
Test: unit — extensão de lifecycle sob o host de extensão real.

- **WHEN** o agente de um Worker `pi` inicia e termina um turno
- **THEN** o transporte de notificação recebe um evento de início e um de fim
- **AND** cada evento carrega o id da conversa do provider e o caminho do
  transcript

#### Scenario: Atividade do painel vem do hook, não do output
Test: unit — `derive_activity` para a família pi.

- **WHEN** um Worker `pi` recebe o evento de início e depois o de fim
- **THEN** o painel mostra o Worker trabalhando e volta a ocioso imediatamente
  no fim, mesmo com o terminal ainda repintando
- **AND** um runtime sem hooks no catálogo continua derivando atividade do wire

#### Scenario: Conclusão notificável exposta ao painel
Test: unit — capabilities por sessão a partir do catálogo pinado.

- **WHEN** o painel Workers carrega uma sessão `pi`
- **THEN** a sessão expõe notificação de conclusão como disponível, no mesmo
  formato de uma sessão `omp`

### Requirement: Instalação de hooks gerenciados sobrevive a um root apagado

O sistema MUST instalar um asset de hook gerenciado mesmo quando o diretório
raiz dele não existe, e MUST tentar todos os runtimes do catálogo mesmo quando a
instalação de um deles falha.

#### Scenario: Root apagado é recriado
Test: unit — escrita atômica de asset gerenciado.

- **WHEN** um asset gerenciado é instalado e o diretório pai não existe
- **THEN** o diretório é criado e o asset fica no lugar
- **AND** a instalação não falha por diretório ausente

#### Scenario: Falha de um runtime não silencia os outros
Test: unit — `install_runtime_hooks_with` recebe a lista de runtimes e o
instalador como closure; a regressão injeta falha no alias do meio e prova que
todos os aliases foram tentados em ordem e que o erro nomeia o runtime. A
composição install+prune (`combine_migration_outcome`) tem regressão própria
provando que o motivo raiz da instalação não é mascarado pelo erro da poda.

- **WHEN** a instalação de um runtime falha
- **THEN** os runtimes restantes ainda são instalados
- **AND** o erro relatado nomeia o runtime que falhou

#### Scenario: Hook de outra ferramenta sob root temporário não bloqueia
Test: integration — verificação do root de hooks legado (`hook_migration`).

- **WHEN** um config de provider contém um hook gerenciado válido e, em outro
  comando, um hook de outra ferramenta sob um diretório temporário
- **THEN** a migração do root legado prossegue
- **AND** um hook gerenciado nosso deixado sob um root temporário continua
  bloqueando a migração

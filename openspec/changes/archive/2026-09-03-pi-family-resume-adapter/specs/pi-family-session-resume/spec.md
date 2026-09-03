## Purpose

Garantir que relançar um Worker `omp` ou `prime-agent` retome a conversa
anterior em vez de iniciar um agente limpo, com o mesmo contrato de resume
que os demais runtimes do catálogo já cumprem.

## ADDED Requirements

### Requirement: Restart de um Worker da família pi retoma a conversa

O sistema SHALL relançar um Worker `omp` ou `prime-agent` que já recebeu
input de forma que a conversa anterior seja retomada, e SHALL declarar para
esses runtimes as mesmas capabilities de restart e resume que os demais
runtimes com resume declaram.

#### Scenario: Restart com id de provider capturado
Test: unit — receita de resume da família pi com id de provider.

- **WHEN** um Worker `omp` cuja extensão de lifecycle já publicou o id da
  conversa de provider é reiniciado
- **THEN** o comando de relançamento aponta explicitamente para essa conversa
  pelo id
- **AND** o comando original é preservado fora dos flags de resume (modelo,
  modo de aprovação, extensão de lifecycle)

#### Scenario: Restart sem id mas com armazenamento pinado
Test: unit — receita de resume da família pi sem id de provider.

- **WHEN** um Worker `omp` sem id de provider publicado, cujo lançamento pinou
  um diretório de sessão gerenciado, é reiniciado
- **THEN** o comando de relançamento continua a sessão mais recente desse
  diretório pinado
- **AND** nunca continua uma sessão de outro diretório ou de outro Worker

#### Scenario: Worker nunca escrito reinicia limpo
Test: integration — relaunch de sessão sem input (`session_actions`).

- **WHEN** um Worker da família pi que nunca recebeu input é reiniciado
- **THEN** o relançamento inicia um agente limpo, sem flags de resume

#### Scenario: Capabilities expostas ao painel
Test: integration — bootstrap do Host lista capabilities por sessão.

- **WHEN** o painel Workers carrega uma sessão `omp` ou `prime-agent`
- **THEN** a sessão expõe `restart` e `resume_agent` como disponíveis, no
  mesmo formato que uma sessão `pi`

### Requirement: Novos Workers da família pi têm armazenamento de sessão isolado

O sistema MUST pinar, no lançamento de um novo Worker `omp` ou `prime-agent`,
um diretório de sessão gerenciado e exclusivo desse Worker sob o home do
Unpeel, exceto quando o comando do usuário já fixa explicitamente sessão,
diretório de sessão ou modo sem sessão.

#### Scenario: Lançamento pina diretório por Worker
Test: unit — preparação de novo lançamento da família pi.

- **WHEN** um Worker `omp` é lançado sem flags de sessão no comando
- **THEN** o comando efetivo inclui um diretório de sessão exclusivo desse
  Worker sob o home do Unpeel
- **AND** o Worker registra esse diretório como armazenamento gerenciado, para
  que remover o Worker remova também a sessão do provider

#### Scenario: Comando do usuário já fixa sessão
Test: unit — preparação de novo lançamento com flags explícitos.

- **WHEN** o comando de lançamento já contém `--continue`, `--resume`,
  `--session-dir` ou `--no-session`
- **THEN** o comando é lançado sem alteração

### Requirement: Sessões pré-existentes não retomam a conversa errada

Para um Worker da família pi criado antes desta capability, sem diretório
pinado e sem id de provider publicado, o sistema MUST tratar o Restart como
reinício limpo em vez de continuar a sessão mais recente do diretório de
trabalho compartilhado.

#### Scenario: Sessão legada sem marker e sem diretório pinado
Test: unit — receita de resume sem id e sem `--session-dir`.

- **WHEN** um Worker `omp` sem id de provider e sem `--session-dir` no comando
  é reiniciado
- **THEN** o relançamento não adiciona `--continue`
- **AND** a UI apresenta o resultado como reinício limpo, não como resume

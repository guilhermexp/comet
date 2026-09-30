## Purpose

Mencionar um projeto conhecido pelo `@` do composer. A menção é uma referência textual com chip — o prompt ganha o path absoluto do projeto, e nada do alvo da sessão muda.

## ADDED Requirements

### Requirement: O `@` abre um menu com Projects acima dos arquivos

O popup de menção SHALL abrir num nível raiz com uma entrada "Projects" acima dos resultados de arquivo, separada deles por uma hairline. Ativar a entrada — clique ou Enter/Tab sobre ela — SHALL trocar o conteúdo do popup pela lista de projetos, sem inserir nada no prompt. Escape dentro da lista de projetos SHALL voltar ao nível raiz; no nível raiz, SHALL fechar o popup.

#### Scenario: A raiz oferece Projects e os arquivos
Test: unit — `composer.rs`, contagem de linhas por nível.

- **WHEN** o popup do `@` abre
- **THEN** a primeira linha é "Projects"
- **AND** as demais linhas são os resultados de arquivo

#### Scenario: Abrir Projects não escreve no prompt
Test: unit — `composer.rs`, aceitação da linha 0 no nível raiz.

- **WHEN** a entrada "Projects" é ativada
- **THEN** o popup passa a listar projetos
- **AND** o texto do input não muda

### Requirement: A lista de projetos é a de Settings → Projects

A lista SHALL vir do registro único de projetos — os Spaces de todos os devices, os mesmos ids que o `list_projects` do chat MCP devolve —, ordenada da atividade mais recente para a mais antiga, exibindo nome, path absoluto e o device dono de cada um. Ela SHALL ser relida do snapshot de Spaces que o app já observa a cada abertura do `@`, e SHALL NOT depender do daemon de Workers nem do ledger de checkouts. Sem snapshot, a lista SHALL ficar vazia, nunca um popup quebrado.

#### Scenario: Os projetos oferecidos são os do registro, com device
Test: unit — `composer.rs`, `project_mentions_are_the_registry_projects_with_their_device`.

- **WHEN** o usuário abre Projects no `@`
- **THEN** a lista contém os projetos do registro de todos os devices, mais recentes primeiro
- **AND** cada linha mostra o nome do device dono

#### Scenario: Query filtra por nome ou por path
Test: unit — `composer.rs`, filtro de projetos.

- **WHEN** o usuário continua digitando depois do `@`
- **THEN** a lista mantém apenas os projetos cujo nome ou path contém a query (case-insensitive)
- **AND** uma query vazia lista todos os projetos conhecidos

#### Scenario: Nenhum projeto casa
Test: unit — `composer.rs`, filtro de projetos.

- **WHEN** nenhum projeto conhecido casa a query
- **THEN** a lista mostra "No matching projects"

### Requirement: Escolher um projeto insere um chip com o path absoluto

Escolher um projeto SHALL substituir o token `@` por um link local `[<nome>](zeron-project:<path absoluto>/)`, projetado como chip com o nome do projeto e tratado como pasta. A escolha SHALL NOT alterar cwd, device ou projeto-alvo do chat.

#### Scenario: Chip mostra o nome, prompt carrega o path
Test: unit — `composer.rs`, round-trip do link de projeto.

- **WHEN** o usuário escolhe o projeto `.orchestrator` em `/Users/x/.orchestrator`
- **THEN** o texto do input contém `[.orchestrator](zeron-project:/Users/x/.orchestrator/)`
- **AND** o chip projetado exibe `@.orchestrator`

#### Scenario: A sessão não é redirecionada
Test: unit — `composer.rs`, aceitação não toca o picker.

- **WHEN** um projeto é mencionado em um chat existente
- **THEN** o cwd, o device e o projeto do chat permanecem os mesmos

#### Scenario: Link inválido não vira chip
Test: unit — `composer.rs`, parse rejeita path não absoluto ou com `..`.

- **WHEN** o texto contém `zeron-project:` com path relativo ou contendo `..`
- **THEN** nenhum chip é projetado e o texto passa como Markdown comum

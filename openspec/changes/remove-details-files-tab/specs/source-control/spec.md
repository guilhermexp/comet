## RENAMED Requirements
- FROM: `### Requirement: Superfície de Source Control na Details Sidebar`
- TO: `### Requirement: Superfície de Source Control no explorer Files`

## MODIFIED Requirements

### Requirement: Superfície de Source Control no explorer Files
O explorer Files SHALL ter uma aba Changes ao lado de Explorer, com badge de quantidade de arquivos mudados; projeto sem raiz git não mostra a aba. A aba SHALL mostrar branch e ahead/behind no cabeçalho, a caixa de mensagem, os botões Commit e Sync/Publish, e as seções Staged Changes e Changes com a letra de status por arquivo. Cada arquivo SHALL oferecer stage ou unstage e discard; cada seção SHALL oferecer a ação em massa correspondente. Clicar num arquivo SHALL abrir o diff dele no painel Changes existente, no escopo working tree, sem abrir um segundo visualizador. Checkout sem repositório git SHALL mostrar estado vazio explícito, sem oferecer ação.

#### Scenario: Badge acompanha a quantidade de mudanças
- Test: unit — derivação do badge a partir do estado de git.
- **WHEN** o checkout passa a ter três arquivos mudados
- **THEN** a aba Changes mostra três no badge

#### Scenario: Arquivo abre no diff existente
- Test: unit — evento emitido ao clicar na row.
- **WHEN** o usuário clica num arquivo da seção Changes
- **THEN** o painel Changes existente passa a mostrar o diff daquele arquivo no escopo working tree

#### Scenario: Diretório sem git não oferece ação
- Test: unit — composição da aba para checkout sem repositório.
- **WHEN** o contexto aponta para um diretório que não é repositório git
- **THEN** a aba mostra estado vazio e nenhum botão de commit ou sync

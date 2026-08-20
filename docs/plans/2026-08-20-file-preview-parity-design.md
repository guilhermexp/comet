# File Preview 1:1 — Design

## Objetivo

Replicar no Comet o file preview do Orchestrator.dev como uma superfície nativa GPUI, mantendo a árvore Files à direita e abrindo os arquivos no painel lateral compartilhado de Terminal/Diff, com abas, cabeçalho, ações e renderização específica por tipo.

## Autoridade visual e comportamental

O Orchestrator.dev em `/Users/guilhermevarela/Documents/Orchestrator.dev` é a fonte de verdade. Os screenshots fornecidos em 2026-08-20 definem a geometria, hierarquia, estados ativos e interações. O preview não substitui a sidebar Details/Files: ele reutiliza o painel Terminal/Git, mantém o chat montado e visível no modo lateral e funciona nos modos Orchestrator e Workers.

## Arquitetura

- `details_sidebar` continua responsável pela árvore e emite uma intenção de abrir arquivo.
- Cada arquivo aberto vira uma `RightSurface`, no mesmo host e na mesma faixa de abas já usada por Terminal e Diff.
- `Shell` mantém uma coleção ordenada de abas por contexto e o arquivo ativo; não existe um terceiro host de painel.
- Um módulo `file_preview` classifica cada caminho e carrega seu conteúdo de forma limitada e somente leitura.
- O painel existente escolhe um viewer por tipo: Markdown, código/texto, HTML, imagem, PDF, CSV/planilha ou unsupported.
- O motor Markdown e o syntax highlighter existentes no workspace são reutilizados; não haverá WebView React embutida nem um segundo frontend.
- Estado de abas e arquivo ativo é isolado por contexto para impedir que um projeto reutilize abas de outro.

## Comportamento

1. Clicar em um arquivo na árvore abre ou ativa sua aba junto às abas Terminal/Diff do painel existente; o chat permanece montado e visível.
2. A aba recebe o mesmo ícone Material Icon Theme da árvore e pode ser fechada.
3. Fechar a aba ativa seleciona a vizinha anterior; fechar a última restaura o conteúdo anterior.
4. O cabeçalho mostra modo/fechar, ícone, nome, expandir, abrir externamente e copiar caminho.
5. Markdown é renderizado; código/texto mostra linhas e syntax highlighting; HTML possui preview; imagens e PDF são exibidos; CSV/TSV e a primeira planilha de arquivos XLS/XLSX são tabelas somente leitura. HTML e PDF usam o viewer nativo no macOS e mostram o fallback de abertura externa nas demais plataformas.
6. Arquivos binários não suportados exibem um estado de recuperação claro sem travar o app.
7. Leituras são confinadas ao checkout, limitadas por tamanho e executadas fora da thread de UI.

## Layout

- As abas de arquivo usam a faixa `RightSurface` existente, junto de Terminal/Diff; o preview não desenha uma segunda faixa interna.
- Cabeçalho do arquivo de 44 px, com divisória sutil.
- Conteúdo preenche o restante e possui scroll próprio.
- A sidebar Files permanece visível e sincroniza o destaque com o arquivo ativo. Sua coluna não possui linha divisória externa; apenas cards/widgets e seus separadores internos desenham borda.
- O controle de captura pertence ao cabeçalho do chat e permanece fora do painel de superfícies.
- No modo expandido o preview usa toda a área central, preservando a sidebar Files.
- O contrato visual de fundo da coluna permanece centralizado no
  [design da sidebar Details/Files](2026-08-20-details-files-sidebar-design.md#visual-contract).

## Testes

- Unitários: classificação, segurança de path, limites de leitura, ciclo das abas e persistência por contexto.
- Render: existência das abas, cabeçalho, estado vazio/erro e seleção sincronizada.
- Integração: abrir, alternar e fechar Markdown, código, HTML e imagem.
- Gate: `cargo test -p zeron-ui`, `cargo fmt --all --check`, `cargo check --workspace`.
- QA visual: app nativo lado a lado com o Orchestrator.dev usando os mesmos arquivos.

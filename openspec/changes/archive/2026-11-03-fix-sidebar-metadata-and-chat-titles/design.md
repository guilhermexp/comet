# Design

## Context

Ver `proposal.md` para motivação e escopo. Base inspecionada: main `4556e3a9faf9b777cc2edcb03d105da6665a440c` (v1.0.15). Os três itens do usuário são: adjacência da metadata, investigação/correção dos títulos automáticos e apresentação do título superior.

- `shell.rs:11854-11949`: branch e projetos usam crescimento flexível; a caixa da branch curta ocupa espaço vazio. `sidebar_faded_label` aplica `flex_1` somente quando `fill=true`.
- `shell/tabs.rs:282-304`: o topo aplica apenas `single_line` ao título cru. Sidebar, hover e side-chat lanes já usam os presenters de menções em `composer.rs:2316-2355`.
- `titles.rs:95-147`: qualquer título existente bloqueia a geração; a ausência de resultado vira `fallback_title(prompt)` e é persistida como definitiva. Esse mecanismo e os gatilhos não mudaram entre v1.0.14 e v1.0.15.
- Não há proveniência manual/modelo/fallback no título atual. O Chat Studio mostrado na imagem contém a URI no valor persistido; não foi examinada a janela de logs nem atribuída sua falha inicial a um provedor.
- `shell.rs::test_shell` usa Workers vivos fora de `cfg(test)`. `WorkersModel::detached`, em `workers/model.rs:561-564`, é hoje somente de teste. O example genérico de sidebar não é prova segura de isolamento nativo.

## Goals / Non-Goals

**Goals:** mudanças locais nos presenters/layout; recuperação do fluxo normal depois de uma geração malsucedida; preservação dos títulos manuais; prova visual e comportamental sem contas reais.

**Non-Goals:** novos modelos/provedores, failover, novos timers/retentativas, mudanças de orçamento, schema ou RPC; varredura de logs privados; renomear Chats antigos; corrigir captions e índices de busca fora do topo solicitado; redesenho, release ou instalação.

## Decisions

### D1. Largura natural da branch curta

A branch não cresce para preencher a linha. Quando há projetos, seu conteúdo continua encolhível/com fade e tem teto de metade da linha; uma branch curta ocupa somente sua largura natural. O contêiner dos projetos usa o restante e o PR mantém sua reserva à direita. O espaçamento continua o existente, 4px na linha de metadata, sem coluna artificial, novos paddings ou alteração de altura.

Implementar a restrição no callsite da branch, não mudar o significado global de `sidebar_faded_label`. O teto conserva uma porção legível para projetos quando a branch é longa; não reservar metade para uma branch curta. Branch off, projetos vazios e Compact seguem seus caminhos existentes.

**Alternativas:** reduzir o gap não corrige o crescimento da caixa; aplicar largura fixa repetiria o vazio; usar `flex_none` no conjunto impediria shrink em janela estreita. A proposta é a composição natural com limite, não essas alternativas.

### D2. Topo textual compacto, sem novo renderer

Aplicar `composer::title_mention_plain` antes de `transcript::single_line` na projeção de `render_session_title_bar`. Emprestar o título via `as_deref`, evitando uma cópia do valor bruto. Conservar 12px/medium, ícone do harness de 14px, gap 6px, truncamento, região de arraste e ações atuais. A sidebar continua com chips; o topo mostra o mesmo rótulo legível como texto, sem importar suas métricas.

Não alimentar o editor de rename, persistência ou exportação com o texto projetado. Reutilizar o suporte existente a menções completas e legadas cortadas; não criar regex/parser novo. Um renderer Markdown ou chips interativos no topo ampliaria o comportamento sem necessidade.

### D3. Falha não é título definitivo

`TitleGenerator::generate` retorna sem gravar título ou renomear branch quando não há saída gerada utilizável. O placeholder visual existente (`New session`) continua; uma chamada normal futura, já disparada em dispatch/completion, continua elegível. Manter seleção/configuração do harness, isolamento, budget e ladder atuais, sem criar novo agendamento. O log de título aplicado só acompanha um título realmente aplicado.

Remover `fallback_title` e seus testes/imports se ficarem sem consumidor real. Não conservar função morta somente para seus próprios testes. Preservar o prompt de geração na língua do usuário e os controles de execução restrita.

Para a confirmação, acrescentar `pub(crate) fn rename_chat_if_untitled(&self, chat_id: &str, title: &str) -> Result<bool, EngineError>` em `WorkspaceHost`. A leitura da existência/título e a escrita usam a mesma mutação do workspace, sem await entre elas. Retornar false se ausente ou já nomeado; não alterar a API de rename manual. A geração só prossegue para os efeitos existentes de renome de worktree quando essa aplicação é aceita, com os guards atuais de contexto/Git. Uma edição manual posterior continua sendo uma edição posterior normal.

**Alternativas:** persistir fallback sem origem mantém o defeito; inferir origem por igualdade com o prompt pode sobrescrever um nome manual. Acrescentar proveniência ao CRDT para preservar fallback durável exige compatibilidade/migração e não é necessário a esta correção. A opção proposta mantém o schema e todos os títulos já salvos.

### D4. Provas isoladas, uma rodada agrupada

Unidades GPUI usam Shell/Workers detached e dados sintéticos. Para smoke apresentado no macOS, acrescentar uma entrada de fixture explicitamente detached sob `appshots-fixture`, reutilizando `Shell::new(state, boot, workers, cx)` e o setup de resource global já existente. Disponibilizar o construtor detached existente também para essa feature, mantendo sua visibilidade interna. Não mudar silenciosamente `test_shell` nem seus consumidores legados para outro modo.

A fixture dedicada `examples/sidebar-title-fixture.rs`, com entrada Cargo `sidebar-title-fixture` condicionada a `appshots-fixture`, usa somente estado/settings temporários e o Shell detached. Antes de abrir a janela, o executor precisa demonstrar que não inicializa engine, daemon, descoberta de providers nem configuração real de harness. Não executar o example legado com Workers vivos, `cargo run` do app ou receita de perfis que só redirecione ZERON_DATA_DIR.

Capturar em lote dark/light e larguras estreita/larga, incluindo branch curta, branch longa com PR, título com menção e modo Compact/Archived representativo. Inspecionar uma rodada e, se houver defeitos, fazer um único lote de ajustes seguido de confirmação. As combinações restantes ficam nas unidades GPUI, não em uma explosão de capturas.

## Ownership e verificação

Um Worker de implementação possui a integração das alterações. Subagentes de pesquisa permanecem somente leitura. Cargo nativo, jobs=2 e sccache, com um target exclusivo e reutilizado para este checkout e preflight de disco. Não reintroduzir fila global/cargo-verify nem iniciar no-mistakes.

Os alvos de produção são shell.rs, shell/tabs.rs, titles.rs e workspace_host.rs; o seam de fixture fica em workers/model.rs/shell.rs, condicionado a teste/feature. Fixtures e testes acompanham o dono, sem novas dependências. Atualizar os contratos UI/engine e changelog; não reescrever DESIGN.md nem criar PRODUCT.md para este refinamento. As larguras atuais 242/256/400px de settings prevalecem no teste; o valor histórico 208px no guia visual não será corrigido fora do escopo.

## Risks / Trade-offs

- R1: limitar branch pode esconder projetos ou deslocar PR. Mitigação: bounds reais e clique de PR nas três larguras, branch curta/longa, toggles e Archived; manter fade/tooltip.
- R2: simplificação de menção pode apagar texto comum ou alterar o dado. Mitigação: presenter existente, Unicode/colchetes/cortes legados e comparação do título durável antes/depois.
- R3: geração pode continuar indisponível por configuração, login, cota ou updater. Esta correção não promete sucesso de provedor nem atribui a causa inicial sem logs; apenas deixa de congelar a falha como título definitivo.
- R4: títulos antigos continuam intactos, inclusive antigos fallbacks indistinguíveis de nomes manuais. Recuperá-los exige seleção/autorização própria, não heurística ou migração oculta.
- R5: fixture nativa pode acessar host real se usar o construtor errado. Isolamento detached é pré-condição de lançamento; sem isso, registrar notProven, não marcar smoke como concluído.

## Migration Plan

Sem migração de dados: nomes existentes permanecem. Após aprovação, implementar em checkout isolado, observar RED/GREEN sintético, executar checks focados e smoke seguro, revisar a árvore final e integrar na main do fork. Arquivar pelo CLI, executar sync e cleanup dos recursos próprios. Não disparar tag/release nem substituir o app sem autorização específica.

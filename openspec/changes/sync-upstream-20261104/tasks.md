# Tasks

## 1. Preparação

- [x] 1.1 Conferir base/alvo/ancestralidade, contratos, ajustes e auditoria de todos os 107 commits; verificar SHAs no Git e entregar auditoria.
- [x] 1.2 Criar proposta/design/specs com D1–D9 iniciais e commit próprio de planejamento; validar OpenSpec.
- [x] 1.3 Executar cargo fmt --all antes do merge e registrar commit próprio se houver delta.

## 2. Integração e contratos

- [x] 2.1 Fazer merge real --no-ff --no-commit e resolver os 153 paths preservando customizações; verificar ausência de unmerged paths e pais do merge.
- [x] 2.2 D1/D2/D8: manter inicialmente zgx/plugins/templates/beta/edge adicional dormentes (D1a ampliado na seção 5) e publicação/engine do fork; verificar wiring e atualizar DOX donos no fechamento documental.
- [x] 2.3 D3/D4/D6/D7/D10: adaptar UI/Usage/harnesses/lineage/output e a coexistência das duas APIs de criação Files; verificar regressões focadas e registrar contratos nos DOX donos.
- [x] 2.4 D5: adaptar escopos IPC e registry único sem perder grants/clientes; verificar regressões MCP/Workers/engine.
- [x] 2.5 D9: portar APIs zui necessárias em fonte separada, registrar proveniência e verificar ambas regras de pontuação + compile downstream.

## 3. Verificação integrada

- [x] 3.1 cargo check --workspace --all-targets com target exclusivo e disco >=20 GB; registrar exit real.
- [x] 3.2 Regressões focadas das crates afetadas e edge typecheck/test; registrar falhas e comparar com base antes de atribuir preexistência.
- [x] 3.3 Comparar additions de cada TAKE/ADAPT ainda presentes em 35561ead com o resultado; documentar cada ausência e corrigir omissões.
- [x] 3.4 cargo fmt --all --check e openspec validate; registrar limitações Linux/macOS/iOS sem fingir prova nativa.

## 4. Entrega

- [x] 4.1 Commit só de docs após código: YAML de histórico, receita/tabelas upstream, DOX e tarefas; verificar git show --stat.
- [x] 4.2 Push sem force apenas origin sync/upstream-20261104 e PR draft para main; conferir head/base/draft na API.
- [x] 4.3 Relatório absoluto .tmp/reports/sync-upstream-20261104.md com todos os SHAs, decisões abertas, checks e URL; conferir existência e head final.

## Workflow follow-up

- Orquestrador executa revisão/aceite no Mac; D1a foi autorizado; dono decide ativação futura da interface D1 e dos serviços D2/D8.
- Não arquivar esta OpenSpec nem fazer merge da PR nesta execução.

## 5. CI e D1a (brief 2026-11-05)

- [x] 5.1 F1/F2: corrigir seletores Cargo preservando cobertura e provar comandos dos jobs.
- [x] 5.2 F3: RED próprio e alinhar descriptor OMP aos modos do driver.
- [x] 5.3 F4: RED próprio, reproduzir e corrigir admissão legacy sem retry/sleep.
- [x] 5.4 F5: terminar árvore Windows, cfg Unix e cross-check dos pacotes nativos.
- [x] 5.5 D1a: RED próprio e ativar plugins/templates mantendo grants e sem MCP injetado.
- [x] 5.6 Executar gates finais, DOX e relatório absoluto, sem publicação nem archive.

### Resultado histórico do follow-up (aceite substituído pela seção 6)

Itens 5.1–5.6 executados. F3/F4/D1a têm commits RED próprios e GREEN; grants e serviço entre engines passaram. Seletores F1/F2 corrigidos e preview Windows compila; cargo check --workspace --all-targets --locked passou. Os gates agregados ainda têm exceções anteriores comprovadas em 3f866aeb9 (Codex Voice, UI, ACP, Unpeel schema/Grok intermitente e compilação Windows do vendor). Ver verification.md e o relatório: esta marcação registra execução/aceite com as exceções permitidas pelo brief, não CI remoto nem uma suíte integral verde. Sem publicação/PR/merge/tag/dispatch e sem archive.

## 6. CI integral — rodada 2 (2026-11-05)

A exceção de baseline da seção 5 foi revogada pelo dono. Falhas herdadas entram no escopo; os gates abaixo devem passar sem novos skips/ignores/retries/sleeps, redução de cobertura ou aumento de orçamento/timeout. Limites de publicação e D1a/grants permanecem.

- [x] 6.1 R1: isolar e corrigir o contrato Codex Voice com RED/GREEN e comparação 35561ead.
- [x] 6.2 R2: corrigir chips URL, índice General e geometria de metadata da sidebar com RED/GREEN.
- [x] 6.3 R3: tornar ACP registry determinístico e provar atualização 2.0.0 → 2.1.0.
- [x] 6.4 R4: reduzir schema sessions dentro do orçamento atual.
- [x] 6.5 R5: corrigir ETXTBSY na raiz e provar ao menos 30 invocações sem falha.
- [x] 6.6 R6: portar os caminhos Windows usados, preservando implementações reais e provar os pacotes native-platform-check via GNU.
- [ ] 6.7 Executar todos os gates exigidos até exit 0; corrigir falhas adicionais; avaliar leaks conforme CI.
- [x] 6.8 Atualizar proveniência/vendor, DOX, conflicts/verification e relatório absoluto novo, sem publicação nem archive.

As falhas adicionais de pilha, Git broker, Cloud RPC, interrupt, Cursor JSON, Stop, discovery, hook roots, log writer, isolamento do browser, acceptor IPC, fixture Skill Usage e publicação da fila entram em 6.7; não há exceção de baseline. A execução nativa dos jobs Windows continua exigindo infraestrutura Windows: a única box pareada é Linux amd64. O cross-check GNU cobre o par harness/engine, sem substituir GUI, runtime, instalador ou testes Windows. Os jobs exclusivos macOS/iOS ficam “provado só no CI remoto”, conforme o brief, sem recibo remoto novo desta rodada. Não marcar 6.7 concluído enquanto faltar essa prova; não fazer dispatch ou publicação para obtê-la.

## 7. Revisão de quatro achados — snapshot 17dabf6a0 (2026-11-05)

- [x] 7.1 RED para espera de status ainda Pending; aguardar early/stop/stale-stop por eventos com deadline existente, sem depender de drain explícito.
- [x] 7.2 RED para marker 2.1.0 não detectado e shebang ausente; conferir ambas versões e executar o probe em artefatos reais como controle positivo.
- [x] 7.3 RED para instalação Codex Voice substituída por nova isolação; exigir CODEX_HOME selecionado e matriz ausente/Unsupported; gate Unix do controle externo.
- [x] 7.4 Gate Unix dos imports de I/O e timeout usados só por socket_command; proveniência e DOX do vendor no mesmo commit.
- [x] 7.5 Testes afetados, core, UI --lib, workspace all-targets check, fmt e par GNU harness/engine com exit 0.
- [x] 7.6 Atualizar verification e acrescentar recibos/commits ao relatório round2; sem push/merge/tag/PR/archive.

Esta revisão não transforma o check GNU em execução Windows nativa; a pendência de infraestrutura de 6.7 permanece distinta dos gates locais desta seção.

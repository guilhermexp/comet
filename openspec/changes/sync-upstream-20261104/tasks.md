# Tasks

## 1. Preparação

- [x] 1.1 Conferir base/alvo/ancestralidade, contratos, ajustes e auditoria de todos os 107 commits; verificar SHAs no Git e entregar auditoria.
- [x] 1.2 Criar proposta/design/specs com D1–D9 iniciais e commit próprio de planejamento; validar OpenSpec.
- [x] 1.3 Executar cargo fmt --all antes do merge e registrar commit próprio se houver delta.

## 2. Integração e contratos

- [x] 2.1 Fazer merge real --no-ff --no-commit e resolver os 153 paths preservando customizações; verificar ausência de unmerged paths e pais do merge.
- [x] 2.2 D1/D2/D8: manter zgx/plugins/templates/beta/edge adicional dormentes e publicação/engine do fork; verificar wiring e atualizar DOX donos no fechamento documental.
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
- [ ] 4.2 Push sem force apenas origin sync/upstream-20261104 e PR draft para main; conferir head/base/draft na API.
- [ ] 4.3 Relatório absoluto .tmp/reports/sync-upstream-20261104.md com todos os SHAs, decisões abertas, checks e URL; conferir existência e head final.

## Workflow follow-up

- Orquestrador executa revisão/aceite no Mac; dono decide ativação futura de D1/D2/D8.
- Não arquivar esta OpenSpec nem fazer merge da PR nesta execução.

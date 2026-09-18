## 1. Contexto, especificação e regressões

- [x] 1.1 Conferir código versus changes relacionados e fixar contrato JSON/tipos; verificar compatibilidade legada/futura por round-trip e atualizar design.
- [x] 1.2 Reproduzir por teste cadastro externo → remoção da pasta → novo snapshot; verificar falha atual de parentesco e preservação de sessões.
- [x] 1.3 Criar fixtures de principal ausente, grupo com mesmo path e legado sem Git; verificar expectativas com testes comportamentais antes de implementar.

## 2. Identidade e cadastro

- [x] 2.1 Implementar probe Git local e identidade separada de remote/branch/ownership; verificar matriz de Git temporário da Task 2 do plano.
- [x] 2.2 Persistir associação no cadastro/criação mantendo IDs e cwd; verificar launch em checkout externo e idempotência de add.
- [x] 2.3 Publicar campos opcionais no snapshot e controller; verificar testes adapter/vendor sem dependência invertida e leitura de estado antigo.

## 3. Reconciliação e recuperação

- [x] 3.1 Implementar dry-run, relatório, backup e journal; verificar que diagnóstico não altera estado e falha de escrita preserva arquivo anterior.
- [x] 3.2 Aplicar patches com revalidação sob lock; verificar idempotência, escritor concorrente, chaves desconhecidas e novo legado após primeiro passe.
- [x] 3.3 Preservar ausentes, conflitos e pendentes; verificar associação manual/desfazer e rollback sem apagar sessões posteriores.
- [x] 3.4 Implementar supressão de metadados esquecidos; verificar polling/restart e re-add explícito.

## 4. Catálogo, Workers e PR

- [x] 4.1 Criar catálogo compartilhado com contêiner não executável e checkouts/histórico; verificar uma raiz por repo e grupos separados.
- [x] 4.2 Integrar árvore, filtro, seleção e histórico de Workers; verificar ausência de roots espúrias após remoção e preservação de Worker ativo/selecionado.
- [x] 4.3 Manter PR por cwd/branch atual e estado indisponível explícito; verificar siblings, branch switch, detached e missing.

## 5. Projects

- [x] 5.1 Implementar lista por projeto e detalhe por checkout; verificar pesquisa por filho, ordenação e preservação de metadados.
- [x] 5.2 Vincular Config/Reveal/Auto Doc/launch ao checkout selecionado; verificar bloqueio para indisponível e nenhum fallback de cwd.
- [x] 5.3 Expor relatório e associação manual com prévia/desfazer; verificar persistência e história acessível.

## 6. Ciclo de vida

- [x] 6.1 Separar archive dos handlers destrutivos e migrar menus; verificar sessões/diretórios preservados e restore.
- [x] 6.2 Validar remoção gerenciada no backend; verificar rejeição de externo/principal/arbitrário/Worker ativo/alterações locais e preservação em falha.
- [x] 6.3 Alinhar forget, ownership e capacidades das duas telas; verificar polling sem ressurreição e ausência de exclusão implícita de sessão/branch.

## 7. Comprovação, revisão, validação, aceite, publicação e documentação

- [x] 7.1 Executar suites/fmt do plano e registrar comandos/resultados reais; verificar ausência de filtros com zero testes.
- [x] 7.2 Fazer revisão independente e resolver achados; verificar integridade da migração e ações em especial.
- [x] 7.3 Validar demo isolada nas duas telas com screenshots e interações; comprovar todos os cenários da matriz de aceite.
- [x] 7.4 Atualizar documentação próxima, proveniência e changes relacionados; verificar OpenSpec strict e relatório de evidências.
- [x] 7.5 Apresentar resultado para aceite e preparar publicação posterior no fork conforme autorização; verificar que nenhum gate pendente foi declarado concluído.

Evidência consolidada: `docs/verification/2026-09-18-workers-project-identity.md`.
Implementação entregue para aceite; publicação e migração do perfil real não
foram executadas nesta etapa.

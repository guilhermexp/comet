# Proposal

## Why

O Context expandido da v1.0.18 pode exibir `Unsupported` com todos os números vazios, mesmo quando a CLI já informa uso geral de contexto. Além disso, o rodapé repete origem, horários e diagnósticos técnicos que o usuário pediu para retirar. A investigação confirmou lacunas de integração e disponibilidade do produtor, não somente um problema de renderização.

## What Changes

- Exibir todos os fatos de contexto realmente disponíveis para o Chat selecionado: total/capacidade nativos quando existem, e composição/listas somente quando o runtime as fornece.
- Conectar o uso agregado já recebido da CLI à expansão; ausência de uma RPC de composição não deve apagar totais válidos ou transformar a seção inteira em `Unsupported`.
- Diferenciar total conhecido com detalhes parciais, ausência de medição, inspeção completa, dado antigo e falha real. Um catálogo de tools ou inventário de arquivos não prova que esses itens estão carregados no contexto.
- Remover os parágrafos persistentes `Native runtime`, `Observed…`, `Provider measurement time unknown` e o motivo bruto abaixo das listas. Usar estado curto e informação de atualização/recuperação sob interação quando útil.
- No OMP, tratar corretamente a ausência da RPC no binário oficial instalado: não prometer que um update oficial comum habilita a extensão do fork. Registrar e testar o contrato do produtor necessário para composição completa.
- Provar o caminho completo de dados por runtime e versão efetivamente suportados, sem prompts diagnósticos, reconstrução especulativa do prompt ou contagem paga.

## Capabilities

### New Capabilities

Nenhuma.

### Modified Capabilities

- `details-sidebar`: detalhes de contexto com total agregado utilizável, cobertura nativa explícita e apresentação sem rodapé técnico redundante.

## Impact

`crates/engine/src/context_breakdown.rs`, fonte de uso agregado no runtime/Chat, adapters e normalização em `crates/harness`, e `crates/ui/src/details_sidebar/{context_breakdown,view}.rs`. Reutilizar os tipos de contexto e os guards de host/modelo/seleção existentes.

A composição completa do OMP depende de um produtor compatível, separado do consumidor Comet; a release oficial OMP instalada não o inclui. O Codex 0.167.0 expõe métricas agregadas, mas seu schema não contém RPC de composição por categoria/arquivo/tool. Esta change deve entregar os dados reais disponíveis e deixar essa cobertura clara; não promete obter informação que a CLI não expõe. Extensão/distribuição de CLIs, se desejada para ampliar a cobertura, permanece dependência explícita e não é instalada silenciosamente por este plano.

## Execution authorization

Implementação autorizada em 2026-11-09 no worker Codex, branch `fix/blaze-omp-usage-context-20261109`, base `8c5898efc5438be60978d8dfd74468df95469e38`. Esta autorização supera a prosa histórica de proposta-apenas. Sem push, tag, release, deploy ou substituição de app/OMP. Aceite, marcação oficial das tasks e arquivamento pertencem ao orquestrador.

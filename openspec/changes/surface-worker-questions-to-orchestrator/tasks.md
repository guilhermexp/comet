## 1. Atenção no motor de estado

- [x] 1.1 Remover a supressão de `PermissionRequest` para prompt de pergunta ao usuário em `is_latch_only` (`third_party/unpeel/crates/unpeel-tui/src/activity.rs`), e atualizar o comentário que declara a premissa antiga. Verificar: teste de unidade novo em que `apply_hook_event` com a ferramenta de pergunta deixa `hook_owned_state` em `Attention`.
- [x] 1.2 Reescrever o caso existente que fixa o comportamento antigo (afirma latch-only e ausência de estado de hook para essa ferramenta) para descrever o comportamento novo. Verificar: o caso antigo não sobrevive como asserção invertida nem como caso duplicado.
- [x] 1.3 Fazer atenção originada de evento explícito de prompt sobreviver a crescimento de sinal de atividade em `note_output_and_sweep`, encerrando apenas por início de turno, marcador de input ou fim de turno. Verificar: teste em que atenção seguida de várias passadas com sinal crescente permanece `Attention`, e teste em que evento de início de turno a encerra.

## 2. Evento de atenção na família pi

- [x] 2.1 Adicionar à extensão de lifecycle da família pi (`third_party/unpeel/runtimes/_shared/pi-family/assets/lifecycle-extension.js`) o listener que emite atenção quando o agente abre prompt interativo bloqueante, carregando o nome da ferramenta. Verificar: teste executável da extensão sob o host real assertando que o transporte recebeu o evento de atenção e nenhum evento de fim de turno.
- [x] 2.2 Declarar atenção confiável no contrato do runtime `omp` (`third_party/unpeel/runtimes/omp/runtime.toml`) e nos demais runtimes da família que carregam a mesma extensão. Verificar: teste de capabilities por sessão a partir do catálogo pinado.
- [x] 2.3 Confirmar que `derive_activity` projeta o estado como bloqueado para a família pi sem depender de `menu_prompt_active`. Verificar: teste de `derive_activity` com estado de atenção e `menu_prompt_active` falso.

## 3. Notificação de espera por input

- [x] 3.1 Emitir `WorkerParentNotificationKind::WaitingForInput` quando uma sessão com tarefa registrada passa a bloqueada, uma vez por episódio de bloqueio. Verificar: teste do motor de notificações com transição para bloqueado, e teste de passadas sucessivas provando emissão única.
- [x] 3.2 Garantir que responder e voltar a bloquear emite nova notificação. Verificar: teste com sequência bloqueado → retomado → bloqueado assertando duas emissões.

## 4. Integridade da cauda de output

- [x] 4.1 Corrigir `safe_output_block` (`crates/workers-unpeel/src/parent_notifications.rs`) para descartar o retorno de carro final antes de escolher o segmento visível. Verificar: teste em que cauda terminada em retorno de carro produz bloco com o último texto pintado e a notificação não declara ausência de conteúdo.
- [x] 4.2 Trocar a coleta que alimenta a notificação em `crates/ui/src/workers/model.rs` pela projeção semântica já usada pelo controller MCP, mantendo a leitura crua como fallback. (Entregue na `main` por `worker_output_text`; o helper paralelo da branch saiu no merge de 2026-09-30.) Verificar: teste assertando que a montagem consome a via semântica quando ela responde.
- [x] 4.3 Preservar a sinalização de ausência real de conteúdo. Verificar: teste com cauda sem texto visível continuando a declarar ausência.

## 5. Não lido com fonte local

- [x] 5.1 Derivar o indicador de não lido do estado de notificação ao pai emitida e não reconhecida, substituindo a leitura do arquivo de estado do app nativo do upstream. Verificar: teste de projeção com notificação pendente reportando não lido.
- [x] 5.2 Limpar o não lido no reconhecimento da notificação. Verificar: teste de projeção após reconhecimento reportando lido.

## 6. Transcript da família pi

- [x] 6.1 Declarar a capability de transcript para `omp` e registrar o adaptador reusando o leitor de sessão JSONL da família, com raízes confiáveis restritas ao diretório de sessão gerenciado. Verificar: teste assertando que o adaptador está registrado na tabela gerada e que a resolução de provedor tem sucesso para um manifesto do runtime.
- [ ] 6.2 Confirmar a leitura fim a fim pelo controller MCP. Verificar: teste de integração do controller lendo o transcript de uma sessão gerenciada e recebendo as entradas da conversa.

## 7. Fechamento

- [x] 7.1 Confirmar que a política de hibernação continua protegendo Worker em atenção. Verificar: teste de `hibernation_candidates` com sessão em estado bloqueado não entrando na lista de candidatos.
- [x] 7.2 DOX pass: atualizar `crates/workers-unpeel/AGENTS.md` (contratos de atenção, não lido, notificação e transcript) e a Test Coverage Matrix da subárvore. Verificar: a cadeia raiz→alvo descreve o comportamento novo e não contém texto stale sobre atenção não confiável.
- [ ] 7.3 Rodar a suíte canônica do workspace uma vez ao final e registrar a saída literal. Verificar: `cargo test --workspace` sem falhas.

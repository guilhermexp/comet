## Context

Ver `proposal.md` — Why. O que restringe a abordagem:

- A máquina de estados de lifecycle é **compartilhada por inclusão**:
  `crates/workers-unpeel/src/activity_bridge.rs` puxa
  `third_party/unpeel/crates/unpeel-tui/src/activity.rs` via `#[path]`. Não há
  cópia local para forkar; a mudança é no fonte vendorizado.
- `third_party/unpeel` é vendorizado sem ancestral comum com o upstream atual.
  Mudança de forma se faz aqui; não há para onde mandar upstream.
- O estado alvo já existe ponta a ponta: `HookState::Attention` →
  `"blocked"` no wire → `WorkerParentNotificationKind::WaitingForInput`. O que
  falta é o evento chegar e não ser descartado.
- `attention_reliable = false` no `runtime.toml` da família pi é declaração de
  contrato, não configuração de runtime: ela descreve o que o runtime sabe
  reportar.
- A tabela `TRANSCRIPT_ADAPTERS` é **gerada em build** a partir das
  capabilities declaradas no catálogo; declarar a capability sem adaptador não
  compila para um provedor resolvível.

## Goals / Non-Goals

**Goals:**

- O Orquestrador distingue Worker bloqueado de Worker trabalhando sem ler o
  terminal.
- O Orquestrador é acordado quando o Worker pergunta, pelo mesmo canal que já
  o acorda quando o Worker termina.
- O conteúdo reportado atravessa a fronteira sem ser apagado por desenho de
  terminal.

**Non-Goals:**

- Não suprimir o prompt interativo nem substituí-lo por um protocolo de
  arquivo. O diálogo continua servindo o humano no pane do app headed; o que
  muda é que ele deixa de ser invisível para o Orquestrador.
- Não introduzir emulador de terminal novo. O caminho semântico já existe.
- Não mexer na política de hibernação.
- Não estender o reconhecimento de prompt por viewport para outros runtimes.

## Decisions

### D1 — A atenção vem do evento de lifecycle, não do desenho do prompt

Existem dois caminhos para `"blocked"`: o evento de hook
(`PermissionRequest` → `HookState::Attention`) e a heurística de viewport
(`menu_prompt_active`, que casa rodapés de navegação conhecidos em
`menu_prompt.rs`). Escolhemos o evento.

A heurística exigiria ensinar ao detector o desenho do prompt de cada runtime
e reagiria a qualquer tela que se pareça com um menu. O evento é emitido por
quem sabe a verdade — a extensão carregada dentro do agente — e a família pi
já tem o transporte montado (`lifecycle-extension.js` já posta `Start` e
`Stop` pelo mesmo caminho). O custo é um listener a mais.

### D2 — A supressão de atenção para prompt de pergunta é removida, não contornada

`is_latch_only` descarta `PermissionRequest` quando a ferramenta é o prompt de
pergunta ao usuário. O comentário que a acompanha declara a razão: *a pergunta
aparece no terminal; atenção seria sinal duplicado*. A premissa assume um
humano olhando o pane. Para um Worker lançado por Orquestrador não há
ninguém olhando, e o sinal não é duplicado — é o único.

Contornar emitindo outro nome de ferramenta esconderia a regra errada atrás de
um nome. A supressão sai.

Consequência de teste: existe um caso que **fixa** o comportamento antigo
(afirma que o evento fica latch-only e que não há estado de hook). Ele passa a
descrever comportamento errado e é reescrito, não re-fixado.

### D3 — Atenção de prompt explícito não é limpa por repintura de tela

O motor hoje sai de `Attention` para `Busy` quando o sinal de atividade cresce
(`note_output_and_sweep`). Um prompt interativo com cursor piscando, spinner
ou contador repinta a tela enquanto espera — o que limparia a atenção poucos
milissegundos depois de ela ser posta, reproduzindo o bug com um passo a mais.

Atenção originada de evento explícito de prompt SHALL sobreviver a crescimento
de sinal. Ela é encerrada por evento de início de turno, por marcador de input
ou por fim de turno — os três já existentes e todos significando que a espera
acabou. Atenção inferida por outros caminhos mantém o comportamento atual.

### D4 — A notificação passa a ler pela via semântica, e a via bruta para de apagar linha

Dois defeitos somados produzem `none`:

1. `safe_output_block` reduz cada linha ao segmento após o último retorno de
   carro. Uma linha **terminada** em retorno de carro devolve o segmento
   vazio, e a linha inteira é apagada. Nada no código anterior previa esse
   caso — o comentário antecipa repaint mais curto, não linha zerada.
2. A notificação lê a cauda **crua** do arquivo de output, enquanto o controller
   MCP já resolve a mesma pergunta pela projeção semântica da grade do
   terminal, que trata retorno de carro como reposicionamento e preserva o
   texto.

Corrigimos os dois: a redução de linha passa a descartar o retorno de carro
final antes de escolher o segmento, e a coleta que alimenta a notificação passa
a usar a via semântica com a via bruta como fallback. Só o primeiro conserto
seria frágil (depende do byte exato no fim do buffer); só o segundo deixaria a
função ainda capaz de apagar conteúdo para quem a chame direto.

Alternativa considerada: alimentar um emulador VT completo na montagem da
notificação. Rejeitada — é exatamente o que a projeção semântica já faz, e
duplicá-la aqui criaria a segunda implementação.

### D5 — Não lido é derivado do que o app já sabe

O campo depende hoje de um arquivo de estado que só o app nativo Swift do
upstream escrevia; no Comet nenhum componente o escreve, então o valor é
permanentemente falso. Duas saídas: passar a escrever o arquivo, ou derivar do
estado local.

Derivamos. O registro de notificação ao pai já modela registro, início,
confirmação, reconhecimento e cancelamento — "tem notificação emitida e não
reconhecida" é a definição natural de não lido e não ressuscita um arquivo com
escritor morto em outra linguagem.

### D6 — Transcript por capability declarada mais adaptador, reusando o leitor da família

O erro atual é recusa de resolução: o runtime não declara a capability e não há
adaptador. A família compartilha formato de sessão em JSONL sob diretório
gerenciado, e o caminho do transcript já é capturado pelo hook e persistido.
Declaramos a capability e registramos o adaptador reusando o leitor já existente
para a família, com as raízes confiáveis restritas ao diretório gerenciado.

## Risks / Trade-offs

- **O app headed passa a mostrar Workers como bloqueados onde antes mostrava
  trabalhando.** → É a correção, não efeito colateral: hoje o painel mente para
  o humano pela mesma razão que mente para o Orquestrador. A mudança torna o
  estado visível antes de o usuário abrir o pane.

- **Atenção mal encerrada deixaria o Worker preso em bloqueado.** → Por isso D3
  enumera os três encerramentos e todos já existem no motor; o caso de retomada
  tem cenário próprio na spec.

- **Interação com a política de hibernação.** → Hibernar exige
  `idle_confirmed_by_hook`, que exige estado ocioso com parada confirmada.
  Atenção não é ocioso, então um Worker bloqueado fica protegido — que é o
  desejado, já que hibernar quem espera resposta perderia a pergunta. A
  política não muda; a verificação precisa confirmar que continua valendo.

- **Ruído de notificação se o bloqueio for reentrante.** → Notificação por
  episódio de bloqueio, não por passada de reconciliação; há cenário cobrindo
  bloqueio contínuo.

- **Declarar atenção confiável muda a autoridade do lifecycle do runtime.** →
  O contrato passa a afirmar algo que o transporte cumpre a partir desta
  mudança; declarar sem o listener seria mentira de catálogo. As duas coisas
  entram juntas.

- **Transcript expõe conteúdo de conversa por uma via nova.** → As raízes
  confiáveis ficam restritas ao diretório de sessão gerenciado do Worker, o
  mesmo que o próprio app já lança e possui.

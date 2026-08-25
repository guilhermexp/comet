# comet-harness — adaptadores de coding agent

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

Esconde *qual* agente está rodando atrás de uma interface só. O trait `Harness` mais três implementações: **claude-code** (subprocesso falando stream-json), **codex** (app-server JSON-RPC) e **mock** (determinístico, usado pelo demo e pelos testes). Junto: mailbox de steering, `requestInput` e os catálogos de modelo/reasoning/opções.

## Ownership

Dona de tudo que é específico de vendor. A engine acima só conhece o trait — se um `if provider == "claude"` apareceu fora daqui, a abstração vazou.

## Local Contracts

- Catálogo de modelo/opção é dado do harness, não constante espalhada na UI.
- Steering é mailbox: comando chega enquanto o run está vivo e é entregue no ponto de corte; sem run vivo, vira o próximo turno.
- Resolução de ambiente de shell (`shell_env_resolution.rs`) existe porque o agente herda o ambiente errado quando invocado fora de um shell de login — mudanças aqui quebram o spawn em máquinas reais sem quebrar teste.
- Fixtures de transcript vivem em `tests/fixtures/` — é o que fixa o parse de stream-json contra mudança de formato do vendor.
- Write/Edit progressivos emitem `ToolCallPreview` transitório com o mesmo id: o parser consome cada chunk uma vez, retém tail de 8 KiB e coalesce refresh a cada 16 KiB; só o `ToolCall` autoritativo posterior carrega input completo.
- Anexo de imagem no OMP **nunca falha por tamanho**: o texto do prompt já lista cada arquivo como caminho local (`with_attachments`) e o OMP roda nesta máquina com ferramentas de arquivo próprias, então um conjunto acima do frame de 2 MiB degrada para esses caminhos e a rodada segue. Três screenshots de Retina estouram o orçamento em base64 — recusar o turno inutilizava um envio rotineiro. O preflight roda **antes** do spawn: é trabalho de filesystem que não depende do filho.
- Falha de transporte do OMP carrega as últimas 4 linhas de stderr do filho (`fatal_message`). "OMP RPC exited before ready" sozinho manda o usuário para um log de debug que ninguém abre.
- Todo launch do OMP passa `--config` com um overlay que desliga os quatro roots de skill de **nível user** (`~/.claude`, `~/.agents`, `~/.codex`, `~/.pi`); roots de nível **project** ficam ligados. Sem isso o chat herda o catálogo pessoal por cima do do repo — medido: 98 de 120 skills num repo alheio, incluindo skills de um workspace vazando nos outros pelos mirrors sob `~`. O overlay precisa ser YAML **aninhado**: chave pontilhada (`skills.enableClaudeUser`) é ignorada em silêncio, e parece ter funcionado. Skill vinda de plugin não passa por esses toggles e sobrevive de propósito — plugin é assunto de `--no-extensions`, não deste overlay. Falha de escrita do overlay degrada para "sem escopo", nunca mata o turno.
- Ordem de chave de JSON **não é contrato**: `serde_json/preserve_order` chega neste crate por unificação de feature do workspace, então `Value::Object` é ordenado sob `cargo test -p zeron-harness` e é ordem-de-inserção no app real. Teste que compara payload serializado compara `Value` parseado, nunca string; fixture que extrai campo faz parse ciente de profundidade, nunca regex guloso — um `.*"type":"..."` pegava o `type` aninhado do `message` e o dispatch caía sem resposta só na configuração que o usuário roda.

## Work Guidance

- Vendor mudou o formato de saída? A correção é uma fixture nova + o parse, nunca um `if` no consumidor.
- Adicionar harness novo = implementar o trait + catálogo + fixture de transcript. Nada mais deve precisar mudar.

## Verification

- Comandos: `cargo test -p comet-harness`

| Camada / path | Tier exigido | Como rodar |
|---|---|---|
| `src/**` (parse, mailbox, catálogos) | unit | `cargo test -p comet-harness` |
| `tests/{claude,codex}.rs` | integration — contra fixtures | `cargo test -p comet-harness` |
| `tests/shell_env_resolution.rs` | integration | `cargo test -p comet-harness` |

## Child DOX Index

Sem filhos.

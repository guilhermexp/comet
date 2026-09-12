## Purpose

Deixar visível, no próprio transcript, onde o modelo de um Chat mudou no meio da conversa. O marcador é durável e sincronizado, como qualquer entry do session doc.

## ADDED Requirements

### Requirement: Trocar o modelo num Chat com transcript escreve um marcador

Escolher outro modelo no picker, com um Chat selecionado que já tem ao menos uma entry, SHALL escrever no session doc uma entry de role `system` cuja part de texto nomeia o modelo anterior e o novo. A escrita SHALL usar os labels do catálogo (o que o picker mostra), não os ids opacos. Chat sem nenhuma entry SHALL NOT receber marcador. Escolher o modelo que já está efetivo SHALL NOT escrever nada.

#### Scenario: Troca no meio da conversa deixa rastro
Test: unit — `transcript.rs`, row de notice a partir da entry `system`.

- **WHEN** o Chat tem transcript e o usuário troca de `GPT-6 Astra` para `Claude Opus 5`
- **THEN** o transcript ganha uma linha `Model changed from GPT-6 Astra to Claude Opus 5.` abaixo do último turno

#### Scenario: Chat vazio não ganha marcador
Test: unit — `transcript.rs`, entries vazias não produzem row.

- **WHEN** o modelo é trocado antes de qualquer mensagem
- **THEN** nenhum marcador é escrito

### Requirement: O marcador é renderizado como divisor de turno

O transcript SHALL renderizar toda entry de role `system` como divisor centralizado — hairline dos dois lados, ícone e texto em tom muted —, nunca como bolha de usuário ou bloco de assistente. Marcador imediatamente seguido de outro marcador SHALL NOT ser renderizado, de modo que trocas consecutivas sem turno entre elas mostrem apenas a última.

#### Scenario: Divisor em vez de mensagem
Test: unit — `transcript.rs`, `RowKind::Notice` para role `system`.

- **WHEN** o transcript carrega uma entry `system`
- **THEN** a row produzida é um notice, não uma row de usuário nem de markdown

#### Scenario: Trocas consecutivas colapsam
Test: unit — `transcript.rs`, colapso de marcadores adjacentes.

- **WHEN** o usuário troca A→B e em seguida B→C sem enviar nada entre as duas
- **THEN** só o marcador `B → C` aparece no transcript

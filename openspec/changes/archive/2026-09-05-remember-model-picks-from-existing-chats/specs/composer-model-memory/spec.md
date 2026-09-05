## ADDED Requirements
### Requirement: Explicit model selection is remembered for subsequent Chats
The composer SHALL remember the harness and complete provider-qualified model id of the last explicit model pick, whether made in a new Chat draft or an existing Chat. No fixed provider or model SHALL override a remembered available selection.

#### Scenario: Existing Chat selection survives navigation and reload
- **WHEN** the user changes an existing Chat from an OpenRouter model to an openai-codex model and opens a new Chat
- **THEN** the new Chat uses that exact openai-codex model, including after reloading local composer defaults
- **Test:** unit

#### Scenario: Browsing another Chat does not replace the remembered model
- **WHEN** the user opens an older Chat with a different model without choosing a model
- **THEN** that Chat retains its model and the next new Chat retains the last explicit pick
- **Test:** unit

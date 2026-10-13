## 1. Detecção

- [x] 1.1 Reconhecer o footer de revisão de hooks em `viewport_has_menu_prompt` e no espelho iOS vendorizado, com testes de aviso, prosa e lista passiva.
- [x] 1.2 Detectar interrupção assentada do turno atual em `viewport_shows_settled_interrupt` e gravar `turn_settled_by_interrupt` no scan do host.

## 2. Atividade e espera

- [x] 2.1 Reportar `idle` sem confirmar `Stop` quando a interrupção do turno atual está visível, na máquina compartilhada, no bridge e no `derive_status` do TUI. Novo footer de trabalho rearma `working`.
- [x] 2.2 Fazer `wait_until_matching` retornar needs-input em `completed`/`exited` quando a atividade é `blocked`, preservando match de `blocked`, cancelamento, deadline e conclusão legítima.

## 3. Contrato e prova

- [x] 3.1 Atualizar DOX dono, changelog do fork e `vendored_tree`. O campo é o tree git de `third_party/unpeel`, não um snapshot upstream. `local_modifications_count` permanece 32: a ferramenta que o produziu não está no repositório. O ajuste Swift do footer não recalcula esse 32.
- [ ] 3.2 Rodar o gate do ticket, capturar o build do wrapper e o smoke isolado de CLI.

## ADDED Requirements

### Requirement: Retarget não entra num checkout onde um Worker está vivo

Um Retarget (Ref com worktree num Chat vivo) SHALL ser recusado quando o checkout de destino tem um
Worker vivo, porque dois agentes escreveriam na mesma árvore sem se verem. Workers são locais ao
device, e a troca de `cwd` é aplicada pelo engine do device que a pede: a checagem SHALL valer quando
esse device é o host do Chat. Quando o Chat é hospedado em outro device, o app SHALL NOT afirmar que
checou — a troca segue sem a checagem, e essa limitação fica registrada. O motivo de uma recusa SHALL
aparecer na faixa de erro do popover. Checkout in place (Ref sem worktree) não é afetado por esta
regra.

#### Scenario: Worker vivo no destino
Test: integration — `SetChatCwd` com um Worker vivo registrado no caminho de destino.

- **GIVEN** um Worker está vivo no worktree de um Ref
- **WHEN** o usuário escolhe esse Ref num Chat vivo
- **THEN** o `cwd` do Chat não muda
- **AND** o popover mostra que um Worker está trabalhando naquele checkout

#### Scenario: Worker parado não bloqueia
Test: integration — mesmo pedido com o Worker parado ou arquivado.

- **WHEN** o Worker daquele checkout não está vivo
- **THEN** o Retarget acontece normalmente

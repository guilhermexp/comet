---
status: accepted
---

# Um Chat vivo pode trocar de Chat Checkout

Até aqui o Chat Checkout era imutável depois da criação. A regra estava escrita no
código, não em doc: `Pickers::pick_ref` abria com
`if self.state.read(cx).selected_chat_row().is_some() { return; }` sob o comentário
"Refs are fixed at creation: an existing session can never move (wing's rule)", e a UI
reforçava isso em dois lugares independentes — o footer do composer trocava o chip
interativo por um label morto quando havia Chat selecionado, e `render_target_selectors`
recusava montar o popover de `PickerKind::Branch`. Três travas para uma decisão.

A regra cai. Um Chat vivo passa a poder trocar de Ref, e o controle sai do footer do
composer para o card Workspace do Details sidebar, que é onde o Chat Checkout já era
exibido — como texto decorativo, ao lado do Path. Um lugar só, com função.

A troca tem duas mecânicas, e qual delas roda é decidido pelo Ref escolhido, não pelo
usuário. Ref que já tem worktree é **Retarget**: `SetChatCwd` aponta o Chat para aquela
pasta e nenhum git checkout acontece. Ref sem worktree é checkout in place: `SwitchRef`
no `cwd` do Chat, e o HEAD-watcher reconcilia `chat.branch` sozinho. A alternativa de
usar só uma das duas foi rejeitada nas duas direções: só checkout in place faz git
recusar exatamente as rows marcadas "worktree", porque a branch já está checada em outro
lugar; só Retarget deixa todo Ref sem worktree inclicável. As duas primitivas já
existiam na engine antes desta mudança — `SetChatCwd` inclusive já documentava
"mid-session switch to an EXISTING worktree" —, então o que estava travado era só a UI.

Retarget custa a continuidade do harness: resume é escopo de cwd, então o próximo run
começa conversa nova. Esse custo é aceito e precisa estar visível na UI antes do clique,
não descoberto depois.

O que substitui a wing's rule como proteção é mais estreito e mais honesto. Árvore suja
não precisa de trava nossa: `Repos::switch_ref` deixa o git falhar com a mensagem dele, e
o popover já tem faixa de erro. O que precisa de trava é agente rodando — árvore limpa,
run vivo, e o checkout troca os arquivos debaixo de um agente que já leu o Ref antigo:
falha silenciosa, sem erro nenhum. Então a troca fica bloqueada enquanto o Chat está
`Working`, com o motivo visível no popover. Bloquear é deliberadamente menos poderoso que
interromper o run e trocar: interromper exige um caminho de aborto confiável que não
existe aqui, e o modo de falha de esperar o run acabar é uma espera, não um repo
corrompido.

Isto não abre a porta para trocar de Space num Chat vivo. O Ref muda dentro do repo que
o Chat já habita; o Space continua fixado na criação.

# Change: Hibernação ligada por default

## Why

A hibernação de Workers ociosos existe, está completa e nunca rodou. O campo
nasce `false` em `WorkersResourceSettings::default()`
(`crates/workers-unpeel/src/lib.rs`), e `hibernation_candidates` devolve lista
vazia na primeira linha enquanto a flag estiver desligada. Como o painel
Resources é a única forma de ligá-la, quem nunca abriu essa seção — o caso
comum — nunca teve hibernação nenhuma.

O efeito medido numa máquina de uso diário, com o app **fechado**: 38 Session
Hosts vivos reparentados ao `launchd`, os mais antigos de pé há mais de dois
dias, segurando 59 processos `zeron`, 48 `zsh`, 22 `claude` e 21 `omp`, com
load average em 13,7 e swap ativo. As 456 sessões em disco eram 417 `exited`
e 38 `running`. Nenhuma delas era terminal puro: 21 `claude`, 12 `omp`, 5
`pi` — todas com `lifecycle_hooks`, `resume` e `restart_agent` no catálogo,
ou seja, **todas elegíveis** à política que já existia. O que faltava era só
a flag.

Não há nada do lado do host que compense isso. O host detached não tem
relógio de ociosidade próprio: escreve heartbeat a cada 60s e espera no
`poll` do PTY, para sempre. `reap_dead_sessions` — documentado como "intended
to run once on app startup" — não tem nenhum chamador no repositório, e mesmo
chamado não desligaria host vivo, porque exige heartbeat com mais de 24h *e*
falha de ping, e o heartbeat nunca envelhece enquanto o processo vive. Ele só
coleta cadáver.

Ligar o default é a mudança inteira. O `WorkersModel` é construído uma vez no
start do app (`crates/ui/src/lib.rs:187`) e faz poll a cada 1s
independentemente do painel Workers estar visível, então
`hibernate_idle_workers` passa a rodar continuamente enquanto o app estiver
aberto, e um Worker ocioso além dos 15 minutos é parado e arquivado com a
conversa preservada.

## What Changes

- `hibernation_enabled` passa a nascer `true` em
  `WorkersResourceSettings::default()`.
- O atributo do campo passa de `#[serde(default)]` para
  `#[serde(default = "default_true")]`. Sem isso, todo bloco
  `comet_workers_resources` gravado antes do campo existir continuaria
  carregando `false` — o default do `bool` — e a mudança não alcançaria
  justamente quem já usa o app.
- Um `hibernation_enabled: false` explícito continua sendo respeitado: é
  decisão de usuário, não campo ausente.

## What Does NOT Change

Nenhuma regra da política. Prazo (15 min), teto de ociosos vivos (12), as
proteções de `working`/`blocked`/pinado/selecionado/terminal, a exigência de
evidência positiva por hook e a reconfirmação sob token do Host ficam
exatamente como estão.

## Fora de escopo, e por quê

- **Backstop de ociosidade dentro do Session Host.** Cobriria o cenário "app
  fechado" e os terminais puros, que nenhuma política do painel alcança. Fica
  de fora porque contradiz frontalmente o requisito "Hibernação exige
  evidência positiva, nunca ausência de sinal": tela parada é também o que um
  subprocesso longo e silencioso produz, e um backstop no host mataria esse
  subprocesso. Com hibernação ligada, sessões de agente são paradas dentro de
  15 minutos enquanto o app roda, então a janela de vazamento por fechamento
  do app fica limitada aos 15 minutos anteriores ao quit.
- **Terminal puro sem runtime de agente** segue vivo indefinidamente. É
  proteção deliberada da spec atual ("sessões de terminal sem runtime de
  agente"), e derrubar um shell perde estado vivo — cwd, env, jobs em
  background — que não é retomável como uma conversa é.
- **Ligar `reap_dead_sessions`.** No upstream ele apaga todo diretório de
  sessão `exited` que não esteja em `saved_sessions`. Este fork nunca popula
  `saved_sessions` — a chave existe no state e está vazia, e nenhum código do
  repo escreve nela. Ligá-lo hoje apagaria os 417 diretórios de sessão
  encerrada, que são o histórico de Workers do painel. É perda de dados
  disfarçada de limpeza; precisa de um conjunto de proteção próprio antes.

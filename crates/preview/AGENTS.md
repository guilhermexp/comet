# zeron-preview — servidores de desenvolvimento

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

Descobrir servidores HTTP pertencentes aos projetos locais e expor URLs estáveis no proxy loopback 7331, com conexão P2P autenticada para outro device do mesmo usuário/org.

## Ownership

Catálogo, descoberta de listeners/processos, proxy HTTP/WebSocket, multiplexação e sinalização. Não controla o sync Loro nem inicia servidores do usuário. Engine fornece raízes, identidade e tokens; UI consome `PreviewSnapshot`.

## Local Contracts

- Port adaptado de upstream #290/#295 (MIT). URLs persistidas não dependem da porta do processo. Revalidar PID, início do processo, cwd e listener antes de conectar evita reutilização indevida de porta.
- Proxy escuta apenas loopback. Só serviços anunciados e validados podem ser alvos; destinos externos arbitrários não são aceitos.
- O patch licenciado de `rtc-sctp` mantém SCTP + DTLS + UDP/IPv6 dentro de 1280 bytes para não travar em VPN; ver `third_party/rust/PATCHES.md`.
- RTC transporta bytes HTTP/HMR; PreviewRoom transporta apenas catálogo e sinalização autenticados. Não há fallback TURN.
- Descoberta roda fora da thread de UI; start é idempotente, stop cancela o serviço e shutdown aguarda tarefas. Novo runtime usa nova instância.

## Work Guidance

Preservar limites de frames, backpressure e teardown de peers. Não colocar credenciais, caminhos ou parâmetros de processo extras no signaling.

## Verification

| Camada / path | Tier | Comando |
|---|---|---|
| `src/**` | unit | `cargo test -p zeron-preview --lib` |
| `tests/**` | integration | `cargo test -p zeron-preview --tests` |
| browser/pairing real | none — BCU e dois devices; fixtures locais não provam rede publicada | ver change OpenSpec |

## Child DOX Index

Sem filhos.

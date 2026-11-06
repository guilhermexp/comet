# Proposal

## Why

Integrar por merge real o intervalo fixo `d5a01522..35561ead` (107 commits sem merge) à base do fork `0f5c6a34`, preservando suas customizações. A entrega autorizada é apenas a branch `sync/upstream-20261104` no origin e uma PR draft para main, sem merge, release ou deploy.

## What Changes

- Loro 1.17, diagnóstico de sync remoto, imagens locais/replies, anotações no Browser, busca nos Settings, atalhos macOS e scroll suave.
- Modos de acesso/plano, Fx/ACP registry, processos em background e leitura host-owned de output, preservando OMP/Kimi, leases, grants raiz e steering.
- Fontes zgx/beta/card-web/directory ficam dormentes; o follow-up D1a ativa o runtime de plugins/templates da engine sem alterar grants, injetar MCP ou ativar outra interface. Decisões D1–D10 no design.
- Versão 1.0.16, release/feed/packaging/workflows do fork, zui vendorizado com proveniência e shells gpui/Workers/Details/Usage preservados.

## Capabilities

### New Capabilities

Nenhuma capability paralela: a integração estende o contrato existente abaixo.

### Modified Capabilities

- `fork-upstream-refresh`: restrições de ativação de interfaces/plugins, grants/IPC, compatibilidade e adoção das melhorias compartilhadas do novo intervalo.

## Impact

Workspace Rust (proto/doc/sync/harness/engine/rpc/client/mobile/mcp/UI/preview/update/theme), sources iOS, edge, scripts e vendor zui. Auditoria commit a commit: `.tmp/reports/sync-upstream-20261104-auditoria.md` (9 TAKE, 85 ADAPT, 13 SKIP). Checks Linux diretos; suíte completa no CI da PR; macOS/iOS e aceite visual ficam para o orquestrador. A OpenSpec permanece aberta.

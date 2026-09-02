## 1. Receita de resume da família pi (vendorizado)

- [x] 1.1 Adicionar testes RED em `runtimes/_shared/pi-family/adapter/` cobrindo:
  resume com id (`--resume <id>`), resume sem id com `--session-dir` pinado
  (`--continue`), sem id e sem diretório (comando intacto), `fresh` removendo
  flags, preparação de novo lançamento pinando `pi-sessions/<id>`, e
  comando com flags explícitos preservado.
- [x] 1.2 Criar `runtimes/_shared/pi-family/adapter/resume.rs` derivado de
  `runtimes/pi/adapter/resume.rs` com os flags do `omp` e sem `--fork`.
- [x] 1.3 Registrar `.with_resume_adapter(resume::ADAPTER)` em
  `runtimes/_shared/pi-family/adapter/mod.rs`.
- [x] 1.4 Declarar `resume` e `restart_agent` em `runtimes/omp/runtime.toml` e
  `runtimes/prime-agent/runtime.toml`; rodar `cargo test -p unpeel-core` até
  GREEN, incluindo a validação do catálogo.

## 2. Frontier e painel

- [x] 2.1 Adicionar teste de integração em `crates/workers-unpeel/tests/session_actions.rs`
  provando que uma sessão `omp` expõe `restart` e `resume_agent` no bootstrap
  e que o relaunch de sessão nunca escrita é limpo.
- [ ] 2.2 (pendente de validação manual) Verificar no app real (`cargo run`) que Restart de um Worker `omp`
  com marker retoma a conversa e que o botão Restart aparece para `omp` e
  `prime-agent`.

## 3. Closeout

- [x] 3.1 Atualizar `third_party/unpeel-upstream.toml` com a nova metadata do
  snapshot vendorizado.
- [x] 3.2 DOX pass: `crates/workers-unpeel/AGENTS.md` (contrato de resume da
  família pi e diretório gerenciado) e pais afetados.
- [x] 3.3 `cargo fmt --all` e `cargo test -p unpeel-core -p zeron-workers-unpeel`.

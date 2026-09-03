## 1. Lifecycle por hook no runtime `pi`

- [x] 1.1 Extrair o append idempotente de `--extension` para
  `setup::with_lifecycle_extension` e fazer o adapter compartilhado da família
  delegar nele.
- [x] 1.2 Registrar no adapter do `pi` a instalação da extensão e o comando de
  startup com gate de alias próprio, preservando resume/context existentes;
  teste unitário do append único e idempotente.
- [x] 1.3 Declarar `lifecycle_hooks` + `notify_when_done` e
  `source = "hooks"` / `authority = "partial"` em `runtimes/pi/runtime.toml`;
  regenerar o catálogo do cliente (`bun scripts/generate-runtime-client-catalog.mjs`).
- [x] 1.4 Repontar os testes que usavam `pi` como runtime hookless para `agy` e
  incluir `pi` no teste de atividade da família pi.

## 2. Instalação de hooks resiliente a root apagado

- [x] 2.1 `write_file_atomic` cria o diretório pai; teste unitário com root
  ausente.
- [x] 2.2 `install_comet_managed_hooks` acumula falhas em vez de abortar no
  primeiro runtime.
- [x] 2.3 `config_has_stale_managed_hook` casa root temporário e nome de asset
  na mesma linha; teste de integração com hook de outra ferramenta sob
  `/private/tmp`.

## 3. Verificação e proveniência

- [x] 3.1 `bun run validate:runtimes`,
  `cargo test --manifest-path third_party/unpeel/crates/Cargo.toml -p unpeel-core`,
  `cargo test -p zeron-workers-unpeel`.
- [x] 3.2 Smoke real: `pi --extension <extensão renderizada> -p …` grava
  `Start` e `Stop` com id de conversa e transcript do provider.
- [x] 3.3 Atualizar `third_party/unpeel-upstream.toml` (`vendored_tree`,
  `local_modifications_count`) e o DOX de
  `crates/workers-unpeel/AGENTS.md` + `third_party/AGENTS.md`.

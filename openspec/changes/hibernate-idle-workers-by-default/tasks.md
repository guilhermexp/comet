# Tasks

## 1. Default

- [x] 1.1 `hibernation_enabled: true` em `WorkersResourceSettings::default()`,
      em `crates/workers-unpeel/src/lib.rs`.
- [x] 1.2 Trocar `#[serde(default)]` por `#[serde(default = "default_true")]`
      no campo, para que bloco persistido sem o campo carregue ligado em vez
      do default do `bool`.

## 2. Testes

- [x] 2.1 Reescrever `hibernation_is_off_by_default_and_never_runs_while_disabled`,
      que codificava a regra aposentada, em duas metades:
      `hibernation_is_on_by_default_so_idle_workers_do_not_pile_up` (o default
      produz candidato) e `hibernation_never_runs_while_explicitly_disabled`
      (com `hibernation_enabled: false` explícito).
      Prova negativa: com o `Default` revertido para `false`, o primeiro
      falha.
- [x] 2.2 `a_persisted_block_without_the_field_loads_hibernation_on` e
      `an_explicitly_disabled_block_stays_disabled` cobrem a carga do serde.
      Prova negativa: com `#[serde(default)]` de volta, o primeiro falha.

## 3. Verificação visual

- [ ] 3.1 `scripts/dev-demo.sh`: settings de Workers, seção Resources — o
      toggle de hibernação aparece ligado numa instalação sem
      `comet_workers_resources` gravado. Não fechável por agente: este repo
      não tem harness de render gpui.

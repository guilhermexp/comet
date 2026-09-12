## 1. Engine detection and slots

- [x] 1.1 Add `grok_home` to `AgentAccountsConfig` and detect/activate/list Grok
- [x] 1.2 Unit tests: detect, omit raw key, activate preserves `defaultModel`

## 2. Accounts and Usage UI

- [x] 2.1 Add Grok to `PROVIDERS`, empty copy, no Add
- [x] 2.2 Grok label/icon in Usage; visible-account test
- [x] 2.3 Update engine/UI AGENTS.md

## 3. Verify

- [x] 3.1 `cargo test -p zeron-engine grok --lib` and `cargo test -p zeron-ui --lib accounts usage`
- [x] 3.2 `openspec validate add-grok-accounts --strict --no-interactive`

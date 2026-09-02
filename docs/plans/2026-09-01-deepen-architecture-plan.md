# Deepen Architecture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Transformar os módulos rasos encontrados na revisão de arquitetura de 2026-09-01 em módulos profundos: um driver de processo por harness, um decoder de `ToolCall`, um método RPC tipado, um runner de git, um seam de Managed Provider Usage, um dono do recovery na engine, um host de surfaces no pane direito e uma única derivação da ordem da sidebar.

**Architecture:** Cada fase é independente, mergeável sozinha e ordenada por risco crescente. Nenhuma fase muda comportamento de capability: são refatorações que preservam wire, doc e UI, então não abrem change no OpenSpec. Cada fase termina com DOX pass no `AGENTS.md` dono. A regra de cada tarefa é a mesma: extrair a decisão para função pura com teste, deixar o chamador só despachando.

**Tech Stack:** Rust 2024, tokio, serde, gpui (fork `wingleeio/zed`), `cargo test -p <crate>`.

**Spec:** Este plano argumenta a partir da lista de candidatos da revisão de arquitetura (sessão de 2026-09-01) e dos contratos em `AGENTS.md`, `crates/AGENTS.md`, `crates/harness/AGENTS.md`, `crates/engine/AGENTS.md`, `crates/ui/AGENTS.md`. Vocabulário de domínio: `CONTEXT.md`.

## Global Constraints

- `cargo fmt --all` antes de todo commit. `cargo test --workspace` verde antes de fechar cada fase.
- Camadas não sobem: `proto` → `doc` → `sync` → `harness` → `engine` → `rpc` → `ui`. Tipo que atravessa duas crates vai para a camada de baixo.
- Nada bloqueante em contexto async; trabalho síncrono em `spawn_blocking`.
- Nada de buffer graúdo vivo através de `.await` (crash de stack em `EngineRpc::handle`, ver `crates/engine/AGENTS.md`).
- Privacidade de input de arquivo: só o preview limitado (`PARTIAL_PREVIEW_BODY_MAX_BYTES = 8 KiB`) entra em `ToolCallPreview`; o `ToolCall` autoritativo é a única cópia completa.
- Vendor mudou formato → fixture nova + parse, nunca `if` no consumidor (`crates/harness/AGENTS.md`).
- Nunca pushar para o upstream; push pelo gate `git push no-mistakes <branch>`.
- Closeout de cada fase: atualizar o `AGENTS.md` dono mais próximo e o Child DOX Index se um arquivo novo virar boundary.
- Um commit por tarefa. Mensagens no formato `refactor(<crate>): <o que>`.
- Nenhuma mudança visual esperada. Onde a ui é tocada, validar em `scripts/dev-demo.sh` olhando a tela, porque suite verde não prova render.

## Ordem das fases

| Fase | Escopo | Risco | Depende de |
|---|---|---|---|
| A | Limpezas baratas (harness, theme, workers client, sidebar morta) | baixo | nada |
| B | Harness: decoder de `ToolCall` + tagger de Subagent | médio | A |
| C | Engine: método RPC tipado, runner de git, seam de usage | médio | nada |
| D | UI: host de surfaces do pane direito, ordem única da sidebar | médio | A |
| E | Harness: driver de processo compartilhado | alto | B |
| F | Engine: dono do recovery e do execute | alto | C |

---

# Fase A — Limpezas baratas

### Task A1: Deletar `acp/subagent_opencode.rs`

**Files:**
- Delete: `crates/harness/src/acp/subagent_opencode.rs`

**Interfaces:**
- Consumes: nada.
- Produces: nada. O arquivo nunca foi declarado como módulo (`acp/mod.rs:30-31` declara só `normalize` e `subagent`).

- [ ] **Step 1: Provar que não é referenciado**

Run: `grep -rn "subagent_opencode\|OpencodeTracker" crates/`
Expected: nenhuma linha fora do próprio arquivo.

- [ ] **Step 2: Deletar e compilar**

```bash
git rm crates/harness/src/acp/subagent_opencode.rs
cargo build -p zeron-harness
```
Expected: build limpo.

- [ ] **Step 3: Commit**

```bash
git commit -m "refactor(harness): delete unreferenced acp/subagent_opencode.rs"
```

### Task A2: Módulo `workers_mcp` único, emitindo os dois formatos

**Files:**
- Create: `crates/harness/src/workers_mcp.rs`
- Modify: `crates/harness/src/lib.rs` (declarar `pub(crate) mod workers_mcp;`)
- Modify: `crates/harness/src/acp/mod.rs:50-95` (remover `workers_mcp_servers_for` e `workers_mcp_servers`, importar do módulo novo)
- Modify: `crates/harness/src/claude/mod.rs:127-181` (remover `claude_workers_mcp_config_for`/`claude_workers_mcp_config`)
- Modify: `crates/harness/src/codex/mod.rs:110-167` (remover `codex_workers_mcp_overrides_for`/`codex_workers_mcp_overrides`)
- Test: `mod tests` em `workers_mcp.rs`; os testes existentes em `claude/mod.rs`, `codex/mod.rs` e `tests/omp_rpc.rs` que comparam a config gerada continuam valendo.

**Interfaces:**
- Produces:
  ```rust
  pub(crate) struct WorkersMcpServer { pub name: &'static str, pub command: PathBuf, pub args: Vec<String>, pub env: Vec<(String, String)> }
  pub(crate) fn resolve(enabled: bool, parent_chat_id: Option<&str>) -> Option<WorkersMcpServer>;          // lê ZERON_DISABLE_WORKERS_MCP / ZERON_WORKERS_MCP_BIN / current_exe
  pub(crate) fn resolve_for(executable: &Path, enabled: bool, disabled_by_environment: bool, parent_chat_id: Option<&str>) -> Option<WorkersMcpServer>; // puro, para teste
  impl WorkersMcpServer {
      pub(crate) fn acp_value(&self) -> serde_json::Value;          // {"type":"stdio","name":..,"command":..,"args":..,"env":[{name,value}]}
      pub(crate) fn claude_config_json(&self) -> String;            // {"mcpServers":{name:{command,args,env:{k:v}}}}
      pub(crate) fn codex_overrides(&self) -> Vec<String>;          // mcp_servers.comet-workers.*
  }
  ```

- [ ] **Step 1: Escrever os testes**

```rust
// crates/harness/src/workers_mcp.rs (fim do arquivo)
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn server() -> WorkersMcpServer {
        resolve_for(Path::new("/opt/zeron"), true, false, Some("chat-1")).expect("enabled")
    }

    #[test]
    fn disabled_or_relative_executable_yields_none() {
        assert!(resolve_for(Path::new("/opt/zeron"), false, false, None).is_none());
        assert!(resolve_for(Path::new("/opt/zeron"), true, true, None).is_none());
        assert!(resolve_for(Path::new("zeron"), true, false, None).is_none());
    }

    #[test]
    fn acp_value_matches_previous_shape() {
        let v = server().acp_value();
        assert_eq!(v["type"], "stdio");
        assert_eq!(v["name"], "comet-workers");
        assert_eq!(v["args"][0], WORKERS_MCP_ARG);
        assert_eq!(v["env"][0]["name"], "COMET_WORKERS_CONTROLLER");
        assert_eq!(v["env"][1]["value"], "chat-1");
    }

    #[test]
    fn claude_config_nests_env_as_object() {
        let parsed: serde_json::Value =
            serde_json::from_str(&server().claude_config_json()).unwrap();
        assert_eq!(parsed["mcpServers"]["comet-workers"]["env"]["COMET_WORKERS_PARENT_CHAT_ID"], "chat-1");
    }

    #[test]
    fn codex_overrides_carry_deadline_and_env() {
        let overrides = server().codex_overrides();
        assert!(overrides.iter().any(|o| o == &format!(
            "mcp_servers.comet-workers.tool_timeout_sec={}", crate::WORKERS_CLIENT_DEADLINE_SECONDS)));
        assert!(overrides.iter().any(|o| o == "mcp_servers.comet-workers.env.COMET_WORKERS_PARENT_CHAT_ID=\"chat-1\""));
    }
}
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p zeron-harness workers_mcp`
Expected: erro de compilação, módulo não existe.

- [ ] **Step 3: Implementar**

```rust
// crates/harness/src/workers_mcp.rs
//! The Comet-owned Workers controller MCP server, resolved once and rendered
//! in each runtime's own config dialect. One resolver, three renderers.
use std::path::{Path, PathBuf};
use serde_json::{Value, json};

pub(crate) const WORKERS_MCP_ARG: &str = "__workers_mcp__";
const NAME: &str = "comet-workers";

pub(crate) struct WorkersMcpServer {
    pub name: &'static str,
    pub command: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

pub(crate) fn resolve(enabled: bool, parent_chat_id: Option<&str>) -> Option<WorkersMcpServer> {
    let disabled = std::env::var("ZERON_DISABLE_WORKERS_MCP").ok().is_some_and(|v| v == "1");
    let executable = std::env::var_os("ZERON_WORKERS_MCP_BIN")
        .map(PathBuf::from)
        .or_else(|| std::env::current_exe().ok())?;
    resolve_for(&executable, enabled, disabled, parent_chat_id)
}

pub(crate) fn resolve_for(
    executable: &Path,
    enabled: bool,
    disabled_by_environment: bool,
    parent_chat_id: Option<&str>,
) -> Option<WorkersMcpServer> {
    if !enabled || disabled_by_environment || !executable.is_absolute() {
        return None;
    }
    let mut env = vec![("COMET_WORKERS_CONTROLLER".to_owned(), "1".to_owned())];
    if let Some(id) = parent_chat_id.filter(|v| !v.trim().is_empty()) {
        env.push(("COMET_WORKERS_PARENT_CHAT_ID".to_owned(), id.to_owned()));
    }
    Some(WorkersMcpServer {
        name: NAME,
        command: executable.to_path_buf(),
        args: vec![WORKERS_MCP_ARG.to_owned()],
        env,
    })
}

impl WorkersMcpServer {
    pub(crate) fn acp_value(&self) -> Value {
        json!({
            "type": "stdio",
            "name": self.name,
            "command": self.command.to_string_lossy(),
            "args": self.args,
            "env": self.env.iter().map(|(k, v)| json!({"name": k, "value": v})).collect::<Vec<_>>(),
        })
    }

    pub(crate) fn claude_config_json(&self) -> String {
        let env: serde_json::Map<String, Value> =
            self.env.iter().map(|(k, v)| (k.clone(), Value::String(v.clone()))).collect();
        json!({ "mcpServers": { self.name: {
            "command": self.command.to_string_lossy(),
            "args": self.args,
            "env": env,
        }}}).to_string()
    }

    pub(crate) fn codex_overrides(&self) -> Vec<String> {
        let quote = |s: &str| serde_json::to_string(s).expect("string serialization cannot fail");
        let mut out = vec![
            format!("mcp_servers.{NAME}.command={}", quote(&self.command.to_string_lossy())),
            format!("mcp_servers.{NAME}.args={}", json!(self.args)),
            format!("mcp_servers.{NAME}.tool_timeout_sec={}", crate::WORKERS_CLIENT_DEADLINE_SECONDS),
        ];
        out.extend(self.env.iter().map(|(k, v)| format!("mcp_servers.{NAME}.env.{k}={}", quote(v))));
        out
    }
}
```

Em `acp/mod.rs`: `fn workers_mcp_servers(enabled, parent) -> Vec<Value>` vira `crate::workers_mcp::resolve(enabled, parent).map(|s| vec![s.acp_value()]).unwrap_or_default()`. Em `claude/mod.rs`: o chamador de `claude_workers_mcp_config(&request)` passa a usar `crate::workers_mcp::resolve(request.enable_workers_mcp, request.workers_parent_chat_id.as_deref()).map(|s| s.claude_config_json())`. Em `codex/mod.rs`: idem com `codex_overrides()`. Testes existentes que chamavam `*_for(executable, request, disabled)` passam a chamar `resolve_for(...)` + renderer.

- [ ] **Step 4: Rodar tudo do harness**

Run: `cargo test -p zeron-harness`
Expected: PASS, inclusive `tests/omp_rpc.rs` que pina o deadline.

- [ ] **Step 5: Commit**

```bash
git commit -am "refactor(harness): single workers_mcp resolver rendering acp/claude/codex dialects"
```

### Task A3: `find_on_paths` para claude e codex

**Files:**
- Modify: `crates/harness/src/claude/mod.rs:66-104` (`resolve_claude_executable`)
- Modify: `crates/harness/src/codex/mod.rs:73-108` (`resolve_codex_executable`)
- Modify: `crates/harness/src/acp/mod.rs:165` → mover `find_on_paths` para `crates/harness/src/lib.rs` ao lado de `node_version_manager_bins` (mesma visibilidade `pub(crate)`); acp, cursor, opencode, omp, `adapter_install.rs` passam a importar `crate::find_on_paths`.

**Interfaces:**
- Consumes: `pub(crate) fn find_on_paths(exe: &str, extra: Vec<PathBuf>) -> Option<PathBuf>` (já existe, só muda de lugar).

- [ ] **Step 1: Reescrever `resolve_claude_executable`**

```rust
fn resolve_claude_executable() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("CLAUDE_CODE_EXECUTABLE") && !p.is_empty() {
        return Some(PathBuf::from(p));
    }
    let exe = if cfg!(windows) { "claude.exe" } else { "claude" };
    let mut extra = Vec::new();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        extra.push(home.join(".claude").join("local").join("claude"));
        extra.push(home.join(".local").join("bin").join("claude"));
    }
    extra.push(PathBuf::from("/opt/homebrew/bin/claude"));
    extra.push(PathBuf::from("/usr/local/bin/claude"));
    crate::find_on_paths(exe, extra)
}
```

Atenção à ordem: `find_on_paths` já faz PATH → login shell → `extra` → node managers, que é a ordem que a versão manual seguia. Os `extra` aqui são caminhos completos (com o nome do binário), e `find_on_paths` faz `d.join(exe)` só para PATH e node managers; `extra` entra como está. Confirmar lendo `acp/mod.rs:165-188` antes de editar.

- [ ] **Step 2: Mesmo para `resolve_codex_executable`** com os dirs extras que ele lista hoje em `codex/mod.rs:73-108`.

- [ ] **Step 3: Testes**

Run: `cargo test -p zeron-harness`
Expected: PASS. `tests/shell_env_resolution.rs` cobre o login shell.

- [ ] **Step 4: Commit**

```bash
git commit -am "refactor(harness): claude and codex resolve executables through find_on_paths"
```

### Task A4: Tipos de tema da ui viram alias de `zeron-theme`

**Files:**
- Modify: `crates/theme/src/lib.rs:277` (`AccentPreset`: adicionar os aliases serde `violet`, `indigo`, `red`, `purple` em `Zeron` e `teal` em `Cyan`)
- Modify: `crates/ui/src/theme.rs:50-160, 213-240` (apagar `AccentColor`, os dois `From`, `Appearance` e `model_appearance`; substituir por `pub use zeron_theme::{AccentPreset as AccentColor, Appearance};` e manter `tokens()` como função livre `fn accent_tokens(accent: AccentColor, appearance: Appearance) -> AccentTokens`, e `Appearance::from_window` como função livre `fn appearance_from_window(gpui::WindowAppearance) -> Appearance`)
- Test: `crates/theme/src/lib.rs` `mod tests` (aliases), `cargo test -p zeron-ui theme`.

**Interfaces:**
- Produces: `zeron_ui::theme::AccentColor` continua existindo como nome (alias). Chamadores de `.into()` entre os dois tipos viram no-op e são removidos onde o compilador reclamar (`impl From<T> for T` já existe).

- [ ] **Step 1: Teste dos aliases no modelo**

```rust
// crates/theme/src/lib.rs, mod tests
#[test]
fn legacy_accent_names_deserialize_to_zeron() {
    for legacy in ["violet", "indigo", "red", "purple"] {
        let v: AccentPreset = serde_json::from_str(&format!("\"{legacy}\"")).unwrap();
        assert_eq!(v, AccentPreset::Zeron);
    }
    let teal: AccentPreset = serde_json::from_str("\"teal\"").unwrap();
    assert_eq!(teal, AccentPreset::Cyan);
}
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p zeron-theme legacy_accent`
Expected: FAIL, `unknown variant violet`.

- [ ] **Step 3: Mover aliases, apagar tipos da ui, compilar até ficar verde**

Run: `cargo build -p zeron-ui 2>&1 | grep -c "^error"` iterando; o compilador aponta cada `.into()` redundante e cada `Appearance::from_window`.

- [ ] **Step 4: Testes e olho**

Run: `cargo test -p zeron-theme && cargo test -p zeron-ui theme`
Run: `scripts/dev-demo.sh` e trocar accent e light/dark em Settings → Appearance. Um `settings.json` antigo com `"accent": "violet"` continua abrindo em Zeron.

- [ ] **Step 5: Commit**

```bash
git commit -am "refactor(ui,theme): ui reuses zeron-theme Appearance and AccentPreset"
```

### Task A5: Um `LocalWorkersClient` compartilhado na ui

**Files:**
- Create: `crates/ui/src/workers/client.rs`
- Modify: `crates/ui/src/workers/mod.rs` (declarar `pub(crate) mod client;`)
- Modify: `crates/ui/src/settings/projects.rs:418`, `crates/ui/src/workers/terminal.rs:745`, `crates/ui/src/workers/resource_monitor.rs:266`, `crates/ui/src/workers/workspace.rs:2025`, `crates/ui/src/workers/model.rs:394` — trocar `LocalWorkersClient::new()` por `crate::workers::client::shared()`.

**Interfaces:**
- Produces: `pub(crate) fn shared() -> LocalWorkersClient` (clone barato: a struct é quatro `Arc`).

- [ ] **Step 1: Implementar**

```rust
// crates/ui/src/workers/client.rs
//! The one `LocalWorkersClient` the app talks through. Five constructors used
//! to race the same process-wide request-id counter and replay cache
//! (`crates/workers-unpeel/AGENTS.md`); one instance keeps that invariant
//! at the call site instead of inside the counter.
use std::sync::OnceLock;
use zeron_workers_unpeel::LocalWorkersClient;

pub(crate) fn shared() -> LocalWorkersClient {
    static CLIENT: OnceLock<LocalWorkersClient> = OnceLock::new();
    CLIENT.get_or_init(LocalWorkersClient::new).clone()
}
```

- [ ] **Step 2: Substituir os cinco call sites e compilar**

Run: `grep -rn "LocalWorkersClient::new" crates/ui/src` → só `client.rs`. `cargo build -p zeron-ui`.

- [ ] **Step 3: Testes e olho**

Run: `cargo test -p zeron-ui workers`. `scripts/dev-demo.sh`: abrir a rota Workers, um terminal de worker e Settings → Projects na mesma sessão.

- [ ] **Step 4: Commit**

```bash
git commit -am "refactor(ui): one shared LocalWorkersClient"
```

### Task A6: Apagar `AppState::jump_target` (código morto testado)

**Files:**
- Modify: `crates/ui/src/state.rs:1346-1357` (função) e `:3270-3295` (teste `jump_target_*`).

A derivação viva é `Shell::sidebar_visible_order`; a Fase D a devolve para `AppState`. Apagar agora evita que a Fase D tenha que escolher entre duas.

- [ ] **Step 1:** `grep -rn "jump_target" crates/ui/src` → só definição e teste. Apagar ambos.
- [ ] **Step 2:** `cargo test -p zeron-ui` → PASS.
- [ ] **Step 3:** `git commit -am "refactor(ui): drop unused AppState::jump_target"`

### Task A7: DOX pass da Fase A

- [ ] `crates/harness/AGENTS.md`: em Local Contracts, substituir a menção a `workers_mcp_servers*` "injetado pelo acp" por "`workers_mcp.rs` resolve o controller uma vez e renderiza no dialeto de cada runtime". Adicionar a `find_on_paths` em `lib.rs` como o único resolvedor de binário.
- [ ] `crates/ui/AGENTS.md`: registrar `workers/client.rs::shared()` como o único construtor de `LocalWorkersClient` na ui.
- [ ] `crates/workers-unpeel/AGENTS.md`: no contrato de request id, apontar que a ui agora tem uma instância só, e que o contador compartilhado permanece como segunda defesa.
- [ ] `git commit -am "docs(dox): phase A closeout"`

---

# Fase B — Decoder de `ToolCall` e tagger de Subagent

### Task B1: `tool_decode.rs` com mapa de chaves por vendor

**Files:**
- Create: `crates/harness/src/tool_decode.rs`
- Modify: `crates/harness/src/lib.rs` (`pub(crate) mod tool_decode;`)
- Test: `mod tests` em `tool_decode.rs`

**Interfaces:**
- Produces:
  ```rust
  pub(crate) struct ToolKeys {
      pub exec: &'static [&'static str],       // nomes do vendor que viram Exec
      pub read: &'static [&'static str],
      pub write: &'static [&'static str],
      pub edit: &'static [&'static str],
      pub search: &'static [&'static str],
      pub glob: &'static [&'static str],
      pub web_fetch: &'static [&'static str],
      pub web_search: &'static [&'static str],
      pub todo: &'static [&'static str],
      pub spawn: &'static [&'static str],      // Agent/Task/spawn_subagent
      pub command_field: &'static str,         // "command"
      pub path_field: &'static str,            // "file_path" | "path"
      pub content_field: &'static str,
      pub old_field: &'static str,
      pub new_field: &'static str,
      pub pattern_field: &'static str,
      pub glob_field: &'static str,            // omp: "path"; claude: "pattern"
      pub glob_fallback_field: Option<&'static str>,
      pub url_field: &'static str,
      pub prompt_field: &'static str,
      pub query_field: &'static str,
      pub todos_field: &'static str,
      pub todo_text_field: &'static str,
      pub todo_status_field: &'static str,
      pub todo_done_value: &'static str,
      pub description_field: &'static str,
      pub mcp_prefix: Option<&'static str>,    // Some("mcp__") para claude
  }
  pub(crate) const CLAUDE_KEYS: ToolKeys;
  pub(crate) const OMP_KEYS: ToolKeys;
  pub(crate) const ACP_KEYS: ToolKeys;
  pub(crate) const CODEX_KEYS: ToolKeys;
  pub(crate) const CURSOR_KEYS: ToolKeys;
  pub(crate) const OPENCODE_KEYS: ToolKeys;
  pub(crate) fn decode(keys: &ToolKeys, name: &str, input: &serde_json::Value) -> ToolCall;
  pub(crate) fn spawn_label(description: &str) -> String;   // "Agent" | "Agent: {d}"
  ```

- [ ] **Step 1: Testes que fixam o comportamento atual de cada vendor**

Extrair de cada `normalize.rs` um caso por variante e colar aqui, sem mudar expectativa. Mínimo:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use zeron_proto::ToolCall;

    #[test]
    fn claude_write_keeps_full_content_in_authoritative_call() {
        let call = decode(&CLAUDE_KEYS, "Write", &json!({"file_path": "a.rs", "content": "x".repeat(20_000)}));
        match call {
            ToolCall::WriteFile { path, content } => {
                assert_eq!(path, "a.rs");
                assert_eq!(content.unwrap().len(), 20_000);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn omp_glob_prefers_path_and_falls_back_to_pattern() {
        assert!(matches!(decode(&OMP_KEYS, "glob", &json!({"path": "**/*.rs"})), ToolCall::Glob { pattern } if pattern == "**/*.rs"));
        assert!(matches!(decode(&OMP_KEYS, "glob", &json!({"pattern": "*.md"})), ToolCall::Glob { pattern } if pattern == "*.md"));
    }

    #[test]
    fn claude_mcp_prefix_splits_server_and_tool() {
        assert!(matches!(decode(&CLAUDE_KEYS, "mcp__github__list_prs", &json!({})), ToolCall::Mcp { server, tool, .. } if server == "github" && tool == "list_prs"));
    }

    #[test]
    fn spawn_names_the_task() {
        assert_eq!(spawn_label(""), "Agent");
        assert_eq!(spawn_label("fix tests"), "Agent: fix tests");
        assert!(matches!(decode(&CLAUDE_KEYS, "Task", &json!({"description": "fix tests"})), ToolCall::Unknown { name, .. } if name == "Agent: fix tests"));
    }

    #[test]
    fn unknown_tool_keeps_name_and_input() {
        assert!(matches!(decode(&CURSOR_KEYS, "weird", &json!({"a": 1})), ToolCall::Unknown { name, input: Some(_) } if name == "weird"));
    }
}
```

- [ ] **Step 2: Rodar e ver falhar** — `cargo test -p zeron-harness tool_decode` → não compila.

- [ ] **Step 3: Implementar `decode`**

```rust
pub(crate) fn decode(keys: &ToolKeys, name: &str, input: &Value) -> ToolCall {
    let s = |k: &str| input.get(k).and_then(Value::as_str).unwrap_or_default().to_owned();
    let opt = |k: &str| input.get(k).and_then(Value::as_str).map(str::to_owned);
    let raw = || (!input.is_null()).then(|| input.clone());
    let is = |set: &[&str]| set.contains(&name);
    if is(keys.exec) { return ToolCall::Exec { command: s(keys.command_field) }; }
    if is(keys.read) { return ToolCall::ReadFile { path: s(keys.path_field) }; }
    if is(keys.write) { return ToolCall::WriteFile { path: s(keys.path_field), content: opt(keys.content_field) }; }
    if is(keys.edit) { return ToolCall::EditFile { path: s(keys.path_field), old_string: opt(keys.old_field), new_string: opt(keys.new_field) }; }
    if is(keys.search) { return ToolCall::Search { pattern: s(keys.pattern_field), path: opt(keys.path_field) }; }
    if is(keys.glob) {
        let mut pattern = s(keys.glob_field);
        if pattern.is_empty() && let Some(fallback) = keys.glob_fallback_field { pattern = s(fallback); }
        return ToolCall::Glob { pattern };
    }
    if is(keys.web_fetch) { return ToolCall::WebFetch { url: s(keys.url_field), prompt: opt(keys.prompt_field) }; }
    if is(keys.web_search) { return ToolCall::WebSearch { query: s(keys.query_field) }; }
    if is(keys.todo) {
        let items = input.get(keys.todos_field).and_then(Value::as_array).map(|a| a.as_slice()).unwrap_or_default()
            .iter().map(|t| zeron_proto::TodoItem {
                text: t.get(keys.todo_text_field).and_then(Value::as_str).unwrap_or_default().to_owned(),
                done: t.get(keys.todo_status_field).and_then(Value::as_str) == Some(keys.todo_done_value),
            }).collect();
        return ToolCall::Todo { items };
    }
    if is(keys.spawn) { return ToolCall::Unknown { name: spawn_label(&s(keys.description_field)), input: raw() }; }
    if let Some(prefix) = keys.mcp_prefix
        && let Some((server, tool)) = name.strip_prefix(prefix).and_then(|r| r.split_once("__"))
    {
        return ToolCall::Mcp { server: server.into(), tool: tool.into(), input: raw() };
    }
    ToolCall::Unknown { name: name.into(), input: raw() }
}

pub(crate) fn spawn_label(description: &str) -> String {
    if description.is_empty() { "Agent".into() } else { format!("Agent: {description}") }
}
```

As constantes `*_KEYS` são preenchidas lendo cada dispatcher atual: `claude/normalize.rs:114`, `omp/normalize.rs:707`, `acp/normalize.rs:204`, `codex/normalize.rs:116`, `cursor/mod.rs:661`, `opencode/mod.rs:2353`. Vendor com campo que não existe na tabela (ex.: `ApplyPatch` do codex) mantém um `match` local **antes** de chamar `decode`, só para aquela variante.

- [ ] **Step 4: Rodar** — `cargo test -p zeron-harness tool_decode` → PASS.

- [ ] **Step 5: Commit** — `git commit -am "refactor(harness): shared ToolCall decoder keyed per vendor"`

### Task B2: Migrar os seis dispatchers para `tool_decode`

**Files:**
- Modify: `crates/harness/src/claude/normalize.rs:114-186` (`decode_tool_use` vira `tool_decode::decode(&CLAUDE_KEYS, name, input)`)
- Modify: `crates/harness/src/omp/normalize.rs:707`, `crates/harness/src/acp/normalize.rs:204`, `crates/harness/src/codex/normalize.rs:116`, `crates/harness/src/cursor/mod.rs:661`, `crates/harness/src/opencode/mod.rs:2353`.
- Test: `tests/{claude,codex,cursor,acp}.rs` e `tests/omp_rpc.rs` contra as fixtures existentes; testes unitários de cada `normalize.rs` continuam.

Um adapter por commit. Ordem: claude, omp (os dois com mais testes), acp, codex, cursor, opencode.

- [ ] **Step 1 (por adapter):** substituir o corpo do dispatcher por uma chamada a `decode`, mantendo a assinatura pública do adapter.
- [ ] **Step 2:** `cargo test -p zeron-harness` → PASS. Se uma fixture falhar, a diferença é chave de JSON errada na constante `*_KEYS`, nunca lógica nova.
- [ ] **Step 3:** `git commit -am "refactor(harness): <adapter> decodes tools through tool_decode"`

### Task B3: Preview bounded como invariante do decoder

**Files:**
- Modify: `crates/harness/src/partial_tool_input.rs:7` (`bounded_file_tool_preview` fica onde está)
- Modify: `crates/harness/src/cursor/mod.rs:682-691` e `crates/harness/src/opencode/mod.rs:2369-2376`: onde emitem `ToolCallPreview`, passar pelo `bounded_file_tool_preview` como claude/acp/omp já fazem. Se o adapter não emite preview (cursor manda `None`), não há mudança: o `ToolCall` autoritativo continua completo.
- Test: `crates/harness/src/opencode/tests.rs` — um caso com `content` de 20 KiB provando que o preview emitido tem ≤ 8 KiB e o `ToolCall` final tem 20 KiB.

- [ ] **Step 1: Teste**

```rust
#[test]
fn opencode_write_preview_is_bounded_and_final_call_is_complete() {
    let content = "x".repeat(20 * 1024);
    let events = normalize_write_sequence(&content); // helper já existente no tests.rs, ou o equivalente que alimenta chunks
    let preview = events.iter().find_map(|e| match e { AgentEvent::ToolCallPreview { call: ToolCall::WriteFile { content, .. }, .. } => content.clone(), _ => None }).unwrap();
    assert!(preview.len() <= crate::partial_tool_input::PARTIAL_PREVIEW_BODY_MAX_BYTES);
    let full = events.iter().find_map(|e| match e { AgentEvent::ToolCall { call: ToolCall::WriteFile { content, .. }, .. } => content.clone(), _ => None }).unwrap();
    assert_eq!(full.len(), 20 * 1024);
}
```

- [ ] **Step 2:** rodar, ver falhar (preview cru), aplicar `bounded_file_tool_preview`, rodar, PASS.
- [ ] **Step 3:** `git commit -am "refactor(harness): every adapter bounds file-tool previews"`

### Task B4: `subagent_tag.rs` — correlação de subagente uma vez

**Files:**
- Create: `crates/harness/src/subagent_tag.rs`
- Modify: `crates/harness/src/acp/subagent.rs:49-55`, `crates/harness/src/claude/normalize.rs:192`, `crates/harness/src/cursor/mod.rs`, `crates/harness/src/opencode/mod.rs:1478` — apagar o `fn tag` local e usar `crate::subagent_tag::tag`. Onde existir o padrão `PendingSpawn` + bind por descrição + FIFO (acp `subagent.rs`, opencode), substituir pelo `SpawnLedger`.
- Test: `mod tests` em `subagent_tag.rs`.

**Interfaces:**
- Produces:
  ```rust
  pub(crate) fn tag(parent_tool_use_id: &str, event: AgentEvent) -> AgentEvent;
  pub(crate) struct SpawnLedger { pending: VecDeque<PendingSpawn>, bound: HashMap<String /*subagent id*/, String /*tool_call_id*/> }
  impl SpawnLedger {
      pub(crate) fn note_spawn(&mut self, tool_call_id: &str, description: &str);
      /// Bind a subagent id to a spawn chip: exact description match first, then FIFO.
      pub(crate) fn bind(&mut self, subagent_id: &str, description: Option<&str>) -> Option<String>;
      pub(crate) fn parent_of(&self, subagent_id: &str) -> Option<&str>;
      pub(crate) fn finish(&mut self, subagent_id: &str) -> Option<String>;
  }
  ```

- [ ] **Step 1: Testes**

```rust
#[test]
fn bind_prefers_description_then_fifo() {
    let mut l = SpawnLedger::default();
    l.note_spawn("t1", "lint");
    l.note_spawn("t2", "tests");
    assert_eq!(l.bind("s-b", Some("tests")).as_deref(), Some("t2"));
    assert_eq!(l.bind("s-a", None).as_deref(), Some("t1"));
    assert_eq!(l.bind("s-c", None), None);
    assert_eq!(l.parent_of("s-a"), Some("t1"));
    assert_eq!(l.finish("s-a").as_deref(), Some("t1"));
    assert_eq!(l.parent_of("s-a"), None);
}

#[test]
fn tag_wraps_event() {
    let ev = tag("t1", AgentEvent::Done { status: zeron_proto::DoneStatus::Completed, ..Default::default() });
    assert!(matches!(ev, AgentEvent::Subagent { parent_tool_use_id, .. } if parent_tool_use_id == "t1"));
}
```

Se `AgentEvent::Done` não implementa `Default`, construir o `Done` com os campos que `proto/src/agent.rs` exige.

- [ ] **Step 2:** rodar, falhar, implementar (`VecDeque` + `HashMap`, sem mais nada), rodar, PASS.
- [ ] **Step 3:** migrar os quatro `tag` e os dois ledgers; `cargo test -p zeron-harness` → PASS.
- [ ] **Step 4:** `git commit -am "refactor(harness): one subagent tag and spawn ledger"`

### Task B5: DOX pass da Fase B

- [ ] `crates/harness/AGENTS.md` Local Contracts: "`tool_decode.rs` é o único mapeamento nome-de-tool → `ToolCall`; adapter novo adiciona uma `ToolKeys`, nunca um `match`. Preview de Write/Edit passa por `bounded_file_tool_preview` em todo adapter. Correlação de subagente é `subagent_tag::SpawnLedger`." Verification: `src/tool_decode.rs`, `src/subagent_tag.rs` → unit.
- [ ] `git commit -am "docs(dox): phase B closeout"`

---

# Fase C — Engine: RPC tipado, runner de git, seam de usage

### Task C1: `RpcMethod` tipado e a lista única em `zeron-rpc`

**Files:**
- Create: `crates/rpc/src/method.rs`
- Modify: `crates/rpc/src/lib.rs:35` (o `pub mod methods` passa a ser gerado pelo macro; consts mantêm nome e valor)
- Test: `mod tests` em `method.rs`

**Interfaces:**
- Produces:
  ```rust
  pub trait RpcMethod {
      const NAME: &'static str;
      const FORWARDABLE: bool;
      const STREAM: bool;
      const DEADLINE: std::time::Duration;
      type Params: serde::Serialize + serde::de::DeserializeOwned + Send;
      type Reply: serde::Serialize + serde::de::DeserializeOwned + Send;
  }
  pub struct MethodInfo { pub name: &'static str, pub forwardable: bool, pub stream: bool, pub deadline: Duration }
  pub fn info(name: &str) -> Option<MethodInfo>;   // gerado pelo macro; a ÚNICA lista
  pub mod methods { pub const LIST_REPOS: &str = "ListRepos"; /* ... todos os 74 */ }
  pub struct ListRepos; impl RpcMethod for ListRepos { ... }  // um marker type por método
  ```

- [ ] **Step 1: Teste**

```rust
#[test]
fn every_method_has_info_and_stream_implies_forwardable() {
    for name in ALL_METHOD_NAMES {
        let info = info(name).unwrap_or_else(|| panic!("{name} missing from registry"));
        if info.stream { assert!(info.forwardable, "{name}: stream methods are relay-proxied"); }
    }
    assert!(info("Nope").is_none());
    assert_eq!(info(methods::CLONE_REPO).unwrap().deadline, std::time::Duration::from_secs(15 * 60));
}
```

- [ ] **Step 2: Macro**

```rust
// crates/rpc/src/method.rs
macro_rules! rpc_methods {
    ($( $ty:ident = $name:literal { params: $p:ty, reply: $r:ty $(, forwardable: $fwd:literal)? $(, stream: $st:literal)? $(, deadline_secs: $dl:literal)? } ),* $(,)?) => {
        pub mod methods { $( pub const $ty: &str = $name; )* }
        pub const ALL_METHOD_NAMES: &[&str] = &[ $( $name ),* ];
        $(
            pub struct $ty;
            impl RpcMethod for $ty {
                const NAME: &'static str = $name;
                const FORWARDABLE: bool = false $(|| $fwd)?;
                const STREAM: bool = false $(|| $st)?;
                const DEADLINE: Duration = Duration::from_secs(30 $(* 0 + $dl)?);
                type Params = $p;
                type Reply = $r;
            }
        )*
        pub fn info(name: &str) -> Option<MethodInfo> {
            match name {
                $( $name => Some(MethodInfo { name: $name, forwardable: <$ty as RpcMethod>::FORWARDABLE, stream: <$ty as RpcMethod>::STREAM, deadline: <$ty as RpcMethod>::DEADLINE }), )*
                _ => None,
            }
        }
    };
}
```

O nome da const (`LIST_REPOS`) é o `$ty` em SCREAMING_CASE hoje; o macro usa o mesmo identificador para const e marker (`pub struct LIST_REPOS;` fica feio). Decisão: o macro recebe dois identificadores, `LIST_REPOS / ListRepos = "ListRepos"`. Ajustar o padrão para `$konst:ident / $ty:ident = $name:literal`.

- [ ] **Step 3: Migrar a lista** — copiar os 74 nomes de `rpc/src/lib.rs:35-…` para a invocação do macro. `params`/`reply` começam como `serde_json::Value` para TODOS (migração mecânica sem quebrar ninguém); `forwardable`, `stream` e `deadline_secs` vêm de `engine/src/rpc.rs:917-1000` (`forward_deadline`, `forwardable`, `is_stream_method`).

- [ ] **Step 4:** `cargo test -p zeron-rpc && cargo build --workspace` → PASS (nomes e valores das consts não mudaram).

- [ ] **Step 5:** `git commit -am "refactor(rpc): single method registry with typed markers"`

### Task C2: Engine lê `info()` em vez das quatro listas

**Files:**
- Modify: `crates/engine/src/rpc.rs:917-1000` (apagar `forward_deadline`, `forwardable`, `is_stream_method`; usar `zeron_rpc::info(method)`), `:1160-1172` (guard de forward usa `info(method).is_some_and(|i| i.forwardable)`), onde `is_stream_method` era lido no `forward`.
- `AuthRpc::handles` (`rpc.rs:1072`) fica: é roteamento por serviço, não atributo do método. Anotar no doc-comment.
- Test: `crates/engine/tests/device_routing.rs` e `relay_delivery.rs` já cobrem forward unary e stream.

- [ ] **Step 1:** substituir; `cargo test -p zeron-engine device_routing relay_delivery` → PASS.
- [ ] **Step 2:** `git commit -am "refactor(engine): rpc forwarding reads the shared method registry"`

### Task C3: Params tipados, um método por vez, começando pelos da ui

**Files:**
- Modify: `crates/rpc/src/method.rs` (trocar `serde_json::Value` pelos structs reais em `params`/`reply`)
- Create: `crates/rpc/src/params.rs` (os structs `*Params` hoje privados em `engine/src/rpc.rs:76-…`, movidos com `pub` e `#[serde(rename_all = "camelCase")]` preservado)
- Modify: `crates/engine/src/rpc.rs` (braços passam a `parse_params::<<X as RpcMethod>::Params>`)
- Modify: `crates/ui/src/state.rs` (`EngineHandle::call_as::<T>(name, json!)` ganha irmão `call_typed::<M: RpcMethod>(params: M::Params) -> Result<M::Reply>`; os 40 call sites migram para ele)
- Test: o teste `agent_account_params_accept_ui_shape` em `engine/src/rpc.rs:2037` é apagado quando o último `json!` da ui para aquele método sumir; é a prova de que o objetivo foi atingido.

**Interfaces:**
- Produces em `state.rs`:
  ```rust
  impl EngineHandle {
      pub async fn call_typed<M: zeron_rpc::RpcMethod>(&self, params: M::Params) -> Result<M::Reply, RpcError> {
          let value = serde_json::to_value(&params).map_err(|e| RpcError::BadParams(e.to_string()))?;
          self.call_as::<M::Reply>(M::NAME, value).await
      }
  }
  ```

- [ ] **Step 1:** implementar `call_typed`; commit.
- [ ] **Step 2 (repetir por método, um commit a cada 5-10):** mover o struct de params para `rpc/src/params.rs`, apontar o macro, trocar o `parse_params` do braço e o `json!` da ui. Ordem: os 40 de `state.rs`, depois `terminal/panel.rs`, `attachments.rs`, `pickers.rs`, `changes.rs`, `shell.rs`.
- [ ] **Step 3:** ao final, `grep -rn "serde_json::json!" crates/ui/src | grep -c methods::` → 0. Apagar `agent_account_params_accept_ui_shape`.
- [ ] **Step 4:** `cargo test --workspace` → PASS.

### Task C4: Dispatch de modos de diff sai do match

**Files:**
- Create: função `pub(crate) async fn capture_for_mode(repos: &Repos, diff_sync: &DiffSync, root: &Path, mode: DiffMode) -> Result<DiffSnapshot, EngineError>` em `crates/engine/src/diff_sync.rs`
- Modify: `crates/engine/src/rpc.rs:1457-1676` (os dois braços chamam `capture_for_mode`)
- Test: `mod tests` em `diff_sync.rs` (validação de modo é pura), `crates/engine/tests/m5_repos_diffs_terminals.rs` (já cobre os quatro modos pelo RPC).

**Interfaces:**
- Produces:
  ```rust
  pub(crate) enum DiffMode { WorkingTree, Branch { base_ref: String }, Commit { sha: String }, Turn { chat_id: String } }
  impl DiffMode {
      /// The wire triple → mode, with the same error strings the handler emits today.
      pub(crate) fn parse(mode: &str, base_ref: Option<&str>, commit_sha: Option<&str>, chat_id: Option<&str>) -> Result<Self, String>;
  }
  ```

- [ ] **Step 1: Teste puro**

```rust
#[test]
fn diff_mode_parse_requires_the_field_of_its_mode() {
    assert_eq!(DiffMode::parse("branch", None, None, None).unwrap_err(), "baseRef required");
    assert_eq!(DiffMode::parse("commit", None, None, None).unwrap_err(), "commitSha required");
    assert_eq!(DiffMode::parse("turn", None, None, None).unwrap_err(), "chatId required");
    assert!(matches!(DiffMode::parse("", None, None, None), Ok(DiffMode::WorkingTree)));
    assert!(matches!(DiffMode::parse("branch", Some("main"), None, None), Ok(DiffMode::Branch { base_ref }) if base_ref == "main"));
}
```

- [ ] **Step 2:** implementar `parse` e `capture_for_mode` (mover os quatro braços de `rpc.rs:1470-1512` para dentro, incluindo o `merge_base` e o filtro `turn_snapshot(...).filter(|s| s.root == root)`). Ambos os braços do RPC ficam com: parse params → `checkout_identity` → `DiffMode::parse` → `capture_for_mode` → montar reply.
- [ ] **Step 3:** `cargo test -p zeron-engine m5_repos_diffs_terminals diff_sync` → PASS. Rodar também `diff_sync::future_size_tests` (o teto de frame).
- [ ] **Step 4:** `git commit -am "refactor(engine): diff mode dispatch lives in diff_sync"`

### Task C5: `process.rs` — o único runner de git

**Files:**
- Create: `crates/engine/src/process.rs` (mover `ProcessRunner`, `ProcessRequest`, `ProcessOutput`, `ProcessRunError`, `SystemProcessRunner` de `source_control.rs:760-880` com visibilidade `pub(crate)`)
- Modify: `crates/engine/src/source_control.rs` (importa de `crate::process`)
- Modify: `crates/engine/src/repos.rs:169-195` (`Repos::git` usa `ProcessRunner` com `timeout: GIT_TIMEOUT`, `output_limit: 16 MiB`; converte `!success` em `EngineError::Other("git: {stderr}")` como hoje)
- Modify: `crates/engine/src/diff_sync.rs:777-820` (`capture_git` vira `runner.run(ProcessRequest{ program: "git", args, cwd, output_limit: max_bytes, .. })` e mapeia `stdout_truncated` → `Capture.truncated`)
- Test: `crates/engine/src/repos.rs` `mod tests` com um `FakeRunner` (mesmo padrão de `source_control.rs:882-898`).

**Interfaces:**
- Consumes (já existem):
  ```rust
  pub(crate) struct ProcessRequest { program: String, args: Vec<String>, cwd: PathBuf, env: Vec<(String,String)>, timeout: Duration, output_limit: usize }
  pub(crate) struct ProcessOutput { success: bool, stdout: Vec<u8>, stderr: Vec<u8>, stdout_truncated: bool }
  #[async_trait] pub(crate) trait ProcessRunner: Send + Sync { async fn run(&self, request: ProcessRequest) -> Result<ProcessOutput, ProcessRunError>; }
  ```
- `Repos` e o módulo `diff_sync` recebem `Arc<dyn ProcessRunner>` no construtor (`Repos::new(data_dir, device_id)` ganha `with_runner` para teste; produção usa `SystemProcessRunner`).

- [ ] **Step 1: Teste em `repos.rs`**

```rust
#[tokio::test]
async fn git_failure_surfaces_stderr() {
    struct Fail;
    #[async_trait::async_trait]
    impl ProcessRunner for Fail {
        async fn run(&self, _r: ProcessRequest) -> Result<ProcessOutput, ProcessRunError> {
            Ok(ProcessOutput { success: false, stdout: vec![], stderr: b"fatal: not a git repository\n".to_vec(), stdout_truncated: false })
        }
    }
    let repos = Repos::with_runner(tempdir_path(), "dev", Arc::new(Fail));
    let err = repos.git(&["status"], None).await.unwrap_err().to_string();
    assert!(err.contains("not a git repository"));
}
```

- [ ] **Step 2:** mover tipos, adaptar `Repos::git` e `capture_git`, manter os `read_to_end` bounded no `SystemProcessRunner` (é ele que já respeita `output_limit`; confirmar em `source_control.rs:800-870` que ele mata o child ao passar do limite; se não mata, portar o `start_kill()` de `capture_git` para lá).
- [ ] **Step 3:** `cargo test -p zeron-engine` inclusive `diff_sync::future_size_tests` e `diff_sync_churn` → PASS.
- [ ] **Step 4:** `git commit -am "refactor(engine): repos and diff_sync run git through ProcessRunner"`

### Task C6: `managed_usage.rs` — cache, fingerprint e invalidação uma vez

**Files:**
- Create: `crates/engine/src/managed_usage.rs`
- Modify: `crates/engine/src/kimi_usage.rs:97-135` e `crates/engine/src/antigravity_usage.rs:102-140` (apagar `CredentialFingerprint`, `Cached*Usage` e a lógica de TTL/invalidação; implementar o trait)
- Test: `mod tests` em `managed_usage.rs` com um provider fake; os testes existentes `cargo test -p zeron-engine antigravity` e `kimi` continuam.

**Interfaces:**
- Produces:
  ```rust
  #[derive(Clone, Copy, PartialEq, Eq)]
  pub(crate) struct CredentialFingerprint(pub [u8; 32]);
  pub(crate) struct UsageWindows { pub windows: Vec<AgentUsageWindow>, pub email: Option<String> }
  #[async_trait]
  pub(crate) trait ManagedUsageProvider: Send + Sync {
      /// Fingerprint of the credential store as it is on disk NOW; None = no credential.
      fn fingerprint(&self) -> Option<CredentialFingerprint>;
      /// Fetch fresh windows for the current credential (refresh included). Errors are provider-redacted.
      async fn fetch(&self) -> Result<UsageWindows, String>;
      fn on_credential_changed(&self) {}   // antigravity drops its in-memory access token here
  }
  pub(crate) struct ManagedUsage<P> { provider: P, ttl: Duration, cache: Mutex<Option<Cached>> }
  struct Cached { credential: CredentialFingerprint, value: UsageWindows, fetched_at: Instant }
  impl<P: ManagedUsageProvider> ManagedUsage<P> {
      pub(crate) fn new(provider: P, ttl: Duration) -> Self;
      pub(crate) async fn snapshot(&self, force: bool) -> ManagedUsageSnapshot;  // {present, windows, email, warning}
      pub(crate) fn invalidate(&self);
  }
  ```

- [ ] **Step 1: Testes com provider fake**

```rust
#[tokio::test]
async fn cache_hits_within_ttl_and_misses_on_fingerprint_change() {
    let p = Fake::new();                       // AtomicU8 fingerprint + AtomicUsize fetch_calls
    let usage = ManagedUsage::new(p.clone(), Duration::from_secs(60));
    usage.snapshot(false).await;
    usage.snapshot(false).await;
    assert_eq!(p.fetch_calls(), 1);
    p.set_fingerprint(2);
    usage.snapshot(false).await;
    assert_eq!(p.fetch_calls(), 2);
    assert_eq!(p.changed_calls(), 1);
}

#[tokio::test]
async fn force_refetches_and_missing_credential_is_absent() {
    let p = Fake::new();
    let usage = ManagedUsage::new(p.clone(), Duration::from_secs(60));
    usage.snapshot(false).await;
    usage.snapshot(true).await;
    assert_eq!(p.fetch_calls(), 2);
    p.clear_credential();
    let snap = usage.snapshot(false).await;
    assert!(!snap.present && snap.windows.is_empty());
}

#[tokio::test]
async fn fetch_error_keeps_present_and_carries_warning() {
    let p = Fake::failing("quota 403");
    let snap = ManagedUsage::new(p, Duration::from_secs(60)).snapshot(false).await;
    assert!(snap.present);
    assert_eq!(snap.warning.as_deref(), Some("quota 403"));
}
```

- [ ] **Step 2:** rodar, falhar, implementar (é a lógica que hoje está em `KimiUsage::snapshot` e `AntigravityUsage::snapshot`; ler as duas antes, elas divergem em detalhe e o teste acima fixa o comportamento comum: TTL, força, fingerprint, ausência, erro).
- [ ] **Step 3:** `KimiUsage` e `AntigravityUsage` implementam `ManagedUsageProvider`; `agent_accounts.rs` constrói `ManagedUsage::new(KimiUsage{..}, Duration::from_secs(60))`. As regras específicas (lock cross-process do Kimi, UA de licença do Antigravity, precedência Keychain) ficam dentro do provider.
- [ ] **Step 4:** `cargo test -p zeron-engine kimi antigravity managed_usage` → PASS.
- [ ] **Step 5:** `git commit -am "refactor(engine): managed provider usage cache shared by kimi and antigravity"`

### Task C7: DOX pass da Fase C

- [ ] `crates/rpc/AGENTS.md`: "`method.rs` é a lista única de métodos: nome, params, reply, forwardable, stream, deadline. Adicionar RPC = uma linha no macro + o handler." Verification: `src/method.rs` → unit.
- [ ] `crates/engine/AGENTS.md`: apagar as menções a `forwardable`/`is_stream_method` como listas a estender; registrar `process.rs` como único runner de subprocesso, `managed_usage.rs` como dono de cache/fingerprint, `diff_sync::capture_for_mode` como o dispatch de modos. Nas linhas de Kimi/Antigravity, trocar "60s em cache" por "TTL do `ManagedUsage`".
- [ ] `crates/ui/AGENTS.md`: "chamada à engine é `EngineHandle::call_typed::<M>`; `json!` literal contra `methods::` é proibido."
- [ ] `git commit -am "docs(dox): phase C closeout"`

---

# Fase D — UI: surfaces do pane direito e ordem da sidebar

### Task D1: `shell/right_pane.rs` — um host de surfaces

**Files:**
- Create: `crates/ui/src/shell/right_pane.rs`
- Modify: `crates/ui/src/shell.rs:560-600` (`RightSurface`, `register_worker_surface` migram), `:1550-1570` (os campos `diffs`, `diff_subs`, `diff_seq`, `preview_surfaces`, `preview_seq`, `subagent_tabs`, `subagent_seq`, `worker_terminal_tabs`, `worker_terminal_seq`, `right_tabs` viram um `right_pane: RightPane`), `:2670` (`resolved_right_active`), `:2731-2960` (`add_*_surface`), `:3057-3112` (`close_right_surface`), `:7596` (`render_right_pane`).
- Test: `mod tests` em `right_pane.rs`.

**Interfaces:**
- Produces:
  ```rust
  pub(super) enum SurfaceBody {
      Diff { view: Entity<Changes>, _sub: Subscription },
      Preview { context_key: String, relative_path: String },
      Subagent { doc_id: String, title: SharedString },
      Worker { title: SharedString, view: WorkersTerminalView },
      Terminal { tab: u64 },
  }
  pub(super) struct RightPane {
      seq: u64,
      bodies: HashMap<u64, SurfaceBody>,
      /// Per panel key: ordered tabs + active surface.
      tabs: HashMap<String, Vec<RightSurface>>,
      active: HashMap<String, RightSurface>,
  }
  impl RightPane {
      pub(super) fn open(&mut self, key: &str, body: SurfaceBody) -> RightSurface;            // aloca id, insere tab, ativa
      pub(super) fn find(&self, pred: impl Fn(&SurfaceBody) -> bool) -> Option<RightSurface>; // dedupe (worker por session_id, preview por path)
      pub(super) fn close(&mut self, key: &str, surface: RightSurface) -> (Option<SurfaceBody>, RightSurface /* next active */);
      pub(super) fn activate(&mut self, key: &str, surface: RightSurface);
      pub(super) fn active(&self, key: &str) -> RightSurface;   // Picker se vazio
      pub(super) fn tabs(&self, key: &str) -> &[RightSurface];
      pub(super) fn body(&self, surface: RightSurface) -> Option<&SurfaceBody>;
      pub(super) fn is_open(&self, key: &str) -> bool;          // right_pane_open = tem tab
  }
  ```
  `RightSurface` mantém as variantes; `open` escolhe a variante pelo `SurfaceBody`.

- [ ] **Step 1: Testes puros** (o `SurfaceBody` de teste usa `Terminal { tab }` e `Preview`, que não precisam de gpui)

```rust
#[test]
fn close_active_falls_back_to_neighbor_then_picker() {
    let mut p = RightPane::default();
    let a = p.open("k", SurfaceBody::Terminal { tab: 1 });
    let b = p.open("k", SurfaceBody::Terminal { tab: 2 });
    assert_eq!(p.active("k"), b);
    let (body, next) = p.close("k", b);
    assert!(matches!(body, Some(SurfaceBody::Terminal { tab: 2 })));
    assert_eq!(next, a);
    p.close("k", a);
    assert_eq!(p.active("k"), RightSurface::Picker);
    assert!(!p.is_open("k"));
}

#[test]
fn ids_are_unique_across_kinds_and_keys() {
    let mut p = RightPane::default();
    let a = p.open("k1", SurfaceBody::Terminal { tab: 1 });
    let b = p.open("k2", SurfaceBody::Preview { context_key: "c".into(), relative_path: "x".into() });
    assert_ne!(a, b);
    assert_eq!(p.tabs("k1").len(), 1);
    assert_eq!(p.tabs("k2").len(), 1);
}

#[test]
fn find_dedupes_by_predicate() {
    let mut p = RightPane::default();
    let a = p.open("k", SurfaceBody::Preview { context_key: "c".into(), relative_path: "x".into() });
    assert_eq!(p.find(|b| matches!(b, SurfaceBody::Preview { relative_path, .. } if relative_path == "x")), Some(a));
}
```

- [ ] **Step 2:** implementar `RightPane`; a regra de "próxima ativa" copia `remove_right_surface` atual (vizinho à esquerda, senão direita, senão Picker). Ler `shell.rs` para confirmar a regra antes.
- [ ] **Step 3:** migrar `Shell`: `close_right_surface` vira `let (body, next) = self.right_pane.close(&key, surface); match body { Some(SurfaceBody::Diff{..}) => {}, Some(SurfaceBody::Terminal{tab}) => panel.close..., Some(SurfaceBody::Preview{..}) => file_preview.close_path(..), Some(SurfaceBody::Subagent{doc_id,..}) => unwatch, Some(SurfaceBody::Worker{view,..}) => view.detach(), None => {} }` e o `if was_active { set_right_active(next) }` de sempre. Os `add_*_surface` viram `self.right_pane.open(key, body)`. O ramo `sidebar_mode == Workers` de `Terminal` permanece, só que agora num lugar só.
- [ ] **Step 4:** `cargo test -p zeron-ui right_pane shell` → PASS. `scripts/dev-demo.sh`: abrir diff, terminal, preview e subagente; fechar a ativa e ver a vizinha assumir; fechar todas e ver a coluna sumir.
- [ ] **Step 5:** `git commit -am "refactor(ui): right pane surfaces live in one host"`

### Task D2: Ordem da sidebar derivada uma vez em `AppState`

**Files:**
- Modify: `crates/ui/src/state.rs:1329` (`sidebar_chats` ganha `sort: SidebarSort` e devolve já ordenado com `compare_sidebar_chats`, que migra de `shell/spaces.rs:25` para `state.rs` como `pub(crate)`)
- Modify: `crates/ui/src/shell/spaces.rs:1066` (`sidebar_visible_order` para de re-ordenar) e `:1092` (`render_active_rows` idem); `:1335` idem.
- Test: `state.rs` `mod tests`.

**Interfaces:**
- Produces: `pub fn sidebar_chats(&self, now: DateTime<Utc>, space_filter: Option<&str>, sort: SidebarSort) -> Vec<(ChatIndicator, &Chat)>`. `SidebarSort` já mora em `settings.rs`; se `state.rs` não puder depender de `settings` sem ciclo, mover o enum para `zeron_proto::view` (regra derivada compartilhada).

- [ ] **Step 1: Teste**

```rust
#[test]
fn sidebar_chats_are_sorted_by_the_requested_sort() {
    let state = state_with_chats(&[("a", created(1), last(9)), ("b", created(2), last(5))]);
    let by_created: Vec<_> = state.sidebar_chats(now(), None, SidebarSort::Created).into_iter().map(|(_, c)| c.id.as_str()).collect();
    assert_eq!(by_created, ["b", "a"]);
    let by_updated: Vec<_> = state.sidebar_chats(now(), None, SidebarSort::LastUpdated).into_iter().map(|(_, c)| c.id.as_str()).collect();
    assert_eq!(by_updated, ["a", "b"]);
}
```

`state_with_chats` é o helper que os testes de `state.rs` já usam para popular `chats`; reusar.

- [ ] **Step 2:** implementar; apagar os três `chats.sort_by(compare_sidebar_chats(...))` de `spaces.rs`.
- [ ] **Step 3:** `cargo test -p zeron-ui` → PASS. `scripts/dev-demo.sh`: trocar o sort no menu da sidebar e usar Ctrl+1..9; a linha pulada é a linha desenhada.
- [ ] **Step 4:** `git commit -am "refactor(ui): sidebar order derived once in AppState"`

### Task D3: DOX pass da Fase D

- [ ] `crates/ui/AGENTS.md`: na regra "O pane direito é um único host de tabs", apontar `shell/right_pane.rs::RightPane` como o host e `SurfaceBody` como a única lista de tipos de surface. Na regra "Ordem de atalho é ordem pintada", dizer que a ordem vem de `AppState::sidebar_chats(.., sort)` e que render não reordena.
- [ ] `git commit -am "docs(dox): phase D closeout"`

---

# Fase E — Driver de processo compartilhado no Harness

Depende da Fase B (os adapters já só têm wire + normalize por dentro).

### Task E1: `driver.rs` com `LineWire`

**Files:**
- Create: `crates/harness/src/driver.rs`
- Modify: `crates/harness/src/lib.rs` (`pub(crate) mod driver;`, mover `StderrTail` para lá se ainda não estiver)
- Test: `mod tests` em `driver.rs` com um wire fake in-process (sem subprocesso: o driver recebe `stdin: impl AsyncWrite`, `stdout: impl AsyncBufRead`, e um `ChildControl` trait para pid/kill; o teste usa `tokio::io::duplex`).

**Interfaces:**
- Produces:
  ```rust
  /// What a line-oriented adapter must know. Everything else is the driver's.
  pub(crate) trait LineWire: Send + 'static {
      type Frame: Send;
      fn tag(&self) -> &'static str;                                   // "claude" | "cursor"...: só para logs
      fn first_lines(&mut self, request: &RunRequest) -> Vec<String>;  // prompt inicial já codificado
      fn parse(&mut self, line: &str) -> Option<Self::Frame>;          // None = pula (log em debug)
      /// Frames that the driver must answer on stdin instead of emitting (control requests).
      fn intercept(&mut self, frame: &Self::Frame, ctx: &mut WireCtx) -> bool;
      fn normalize(&mut self, frame: Self::Frame, interrupted: bool) -> Vec<AgentEvent>;
      fn steer_lines(&mut self, msg: &SteerMessage) -> Vec<String>;
      fn interrupt_lines(&mut self) -> Vec<String>;
      fn rotate_for_steer(&mut self) -> (String, String);
      fn close_stdin_on_mailbox_close(&self) -> bool { true }
  }
  pub(crate) struct WireCtx<'a> { pub stdin: &'a mpsc::UnboundedSender<StdinMsg>, pub request_input: &'a Arc<RequestInputFn> }
  pub(crate) enum StdinMsg { Line(String), Close }
  pub(crate) struct DriverConfig { pub interrupt_grace: Duration, pub kill_grace: Duration }
  #[async_trait]
  pub(crate) trait ChildControl: Send + 'static { fn pid(&self) -> Option<u32>; fn start_kill(&mut self); async fn wait(&mut self) -> std::io::Result<std::process::ExitStatus>; }
  impl ChildControl for tokio::process::Child { ... }
  /// Spawn + run: the whole of today's `run()` preamble and `run_session` loop.
  pub(crate) async fn run_line_process<W: LineWire>(
      cmd: tokio::process::Command,
      exe_for_error: &Path,
      wire: W,
      request: RunRequest,
      controls: RunControls,
      config: DriverConfig,
  ) -> Result<BoxStream<'static, Result<AgentEvent, HarnessError>>, HarnessError>;
  /// The loop alone, for tests and for adapters that spawn differently (omp).
  pub(crate) async fn drive<W: LineWire, C: ChildControl>(
      child: C, stdin: impl AsyncWrite + Unpin + Send + 'static, stdout: impl AsyncBufRead + Unpin + Send + 'static,
      wire: W, controls: RunControls, config: DriverConfig, event_tx: mpsc::Sender<Result<AgentEvent, HarnessError>>,
  );
  ```

- [ ] **Step 1: Testes do loop com `tokio::io::duplex`**

```rust
struct EchoWire;   // parse: cada linha vira Frame(String); normalize: "done" → Done, senão Text
struct FakeChild { killed: Arc<AtomicBool>, exit: oneshot::Receiver<()> }

#[tokio::test]
async fn done_closes_the_stream_and_steer_rotates_id() {
    let (stdout_w, stdout_r) = tokio::io::duplex(1024);
    let (stdin_w, mut stdin_r) = tokio::io::duplex(1024);
    let (controls, steer_tx, _interrupt) = test_controls();
    let (tx, mut rx) = mpsc::channel(16);
    tokio::spawn(drive(FakeChild::alive(), stdin_w, BufReader::new(stdout_r), EchoWire, controls, DriverConfig::test(), tx));
    steer_tx.send(SteerMessage { prompt: "more".into(), message_id: None }).await.unwrap();
    assert!(matches!(rx.recv().await.unwrap().unwrap(), AgentEvent::Steered { .. }));
    assert_eq!(read_line(&mut stdin_r).await, "more");
    stdout_w.write_all(b"done\n").await.unwrap();
    assert!(matches!(rx.recv().await.unwrap().unwrap(), AgentEvent::Done { .. }));
}

#[tokio::test]
async fn interrupt_sends_wire_line_then_escalates() {
    // grace de 10ms; FakeChild registra kill; após interrupt sem Done o driver emite Done{Interrupted}
    let (stdout_w, stdout_r) = tokio::io::duplex(1024);
    let (stdin_w, mut stdin_r) = tokio::io::duplex(1024);
    let (controls, _steer, interrupt) = test_controls();
    let (tx, mut rx) = mpsc::channel(16);
    let child = FakeChild::alive();
    let killed = child.killed.clone();
    tokio::spawn(drive(child, stdin_w, BufReader::new(stdout_r), EchoWire, controls, DriverConfig { interrupt_grace: Duration::from_millis(10), kill_grace: Duration::from_millis(10) }, tx));
    interrupt.cancel();
    assert_eq!(read_line(&mut stdin_r).await, "interrupt");
    drop(stdout_w); // EOF: CLI saiu
    assert!(matches!(rx.recv().await.unwrap().unwrap(), AgentEvent::Done { status: DoneStatus::Interrupted, .. }));
    let _ = killed; // escalada só dispara se o child não sair no grace; aqui saiu
}

#[tokio::test]
async fn consumer_gone_stops_the_loop() {
    let (stdout_w, stdout_r) = tokio::io::duplex(1024);
    let (stdin_w, _stdin_r) = tokio::io::duplex(1024);
    let (controls, _s, _i) = test_controls();
    let (tx, rx) = mpsc::channel(1);
    let h = tokio::spawn(drive(FakeChild::alive(), stdin_w, BufReader::new(stdout_r), EchoWire, controls, DriverConfig::test(), tx));
    drop(rx);
    stdout_w.write_all(b"hello\n").await.unwrap();
    tokio::time::timeout(Duration::from_secs(1), h).await.expect("loop exits when consumer hangs up").unwrap();
}
```

`test_controls()` constrói `RunControls` com `request_input` que responde vazio, o `steering` receiver e o `CancellationToken` de interrupt, na forma exata que `RunControls` em `lib.rs:52` exige. Ler a struct antes.

- [ ] **Step 2:** rodar, falhar, implementar `drive` copiando o loop de `claude/mod.rs:700-800` (é o mais completo: stdout, steer, interrupt+escalada, `event_tx.closed()`, bookkeeping final com `Done` sintético, abort da escalada, `shutdown_child`). `run_line_process` copia o preâmbulo de `claude/mod.rs:489-558`.
- [ ] **Step 3:** `cargo test -p zeron-harness driver` → PASS.
- [ ] **Step 4:** `git commit -am "refactor(harness): shared line-process driver"`

### Task E2: Migrar claude e cursor para o driver

**Files:**
- Modify: `crates/harness/src/claude/mod.rs` (`impl LineWire for ClaudeWire` usando `wire::parse_frame`, `Normalizer`, `handle_control_request`, `wire::user_message_line`, `wire::interrupt_request_line("int_1")`; `run()` vira `build_command` + `run_line_process`; apagar `Session`, `run_session`, `stdin_writer`)
- Modify: `crates/harness/src/cursor/mod.rs` idem (apagar `stdin_writer` duplicado em `:413`).
- Test: `tests/claude.rs`, `tests/cursor.rs` contra `fake-claude.sh` / `fake-cursor-shim.sh` (agora são testes do wire, não do loop).

- [ ] **Step 1:** claude. `cargo test -p zeron-harness claude` → PASS. Commit.
- [ ] **Step 2:** cursor. `cargo test -p zeron-harness cursor` → PASS. Commit.

### Task E3: Migrar codex, acp e opencode

Os três falam JSON-RPC bidirecional (`jsonrpc.rs`) em vez de linhas simples, então `LineWire::intercept` é onde entram as respostas a requests do servidor. Se o encaixe exigir mais de 2 métodos novos no trait, **parar e registrar** em `docs/plans/` um design curto antes de seguir: dois drivers (`LineWire` e `RpcWire`) é aceitável, três não.

- [ ] codex → commit → acp → commit → opencode → commit, cada um com `cargo test -p zeron-harness` verde.
- [ ] Ao final: `grep -rn "steering_open\|interrupt_sent" crates/harness/src` → só `driver.rs`. `grep -rn "unfold(event_rx" crates/harness/src` → só `driver.rs` e `omp/`.

### Task E4: omp opcional

O omp já expõe `normalize/process/protocol` e tem 1491 linhas de teste in-process. Migrar só se `drive` couber sem mudança no trait; senão deixar e anotar no DOX que o omp mantém driver próprio por ter protocolo v2 com chunks.

### Task E5: DOX pass da Fase E

- [ ] `crates/harness/AGENTS.md` Purpose e Work Guidance: "Adicionar harness = `LineWire` (ou `RpcWire`) + `ToolKeys` + catálogo + fixture. O driver é dono de spawn, mailbox, interrupt e escalada." Verification: `src/driver.rs` → unit (fake wire in-process); `tests/{claude,codex,cursor,acp}.rs` continuam integration contra fixtures, agora cobrindo só wire.
- [ ] `git commit -am "docs(dox): phase E closeout"`

---

# Fase F — Engine: dono do recovery e do execute

Depende da Fase C. Contrato testado: `crates/engine/tests/restart_resume.rs`, `transcript_salvage.rs`, `self_continued_quiesce.rs`, `born_chat2_race.rs`.

### Task F1: `recovery.rs` — os três passes de boot num lugar

**Files:**
- Create: `crates/engine/src/recovery.rs`
- Modify: `crates/engine/src/sessions.rs:1121` (`recover_stale`) e `:1268` (`sweep_abandoned_streams`) — movem para `recovery.rs` como funções que recebem `&SessionsEngine` e `&DocHost`.
- Modify: `crates/engine/src/doc_host.rs:1583-1625` (`spawn_transcript_salvage` e `salvage_chat_transcript` movem; `mark_abandoned_streams` em `:491` fica no `ChatDocHandle`, é operação de doc).
- Modify: `crates/engine/src/lib.rs:237-243` (as três chamadas viram `recovery::run_boot_recovery(&sessions, &doc_host, journal_root)`).
- Test: `crates/engine/tests/restart_resume.rs`, `transcript_salvage.rs` sem alteração de expectativa; novo `mod tests` em `recovery.rs` para a ordem dos passes.

**Interfaces:**
- Produces:
  ```rust
  pub(crate) struct BootRecoveryReport { pub resumed: usize, pub streams_settled: usize }
  /// Order is the contract: stale journals first (may resume a run), then every journaled chat
  /// with no live run gets its streaming entries settled, then the transcript salvage sweep is
  /// spawned after the boot-settle delay.
  pub(crate) fn run_boot_recovery(sessions: &SessionsEngine, doc_host: &DocHost, journals_dir: PathBuf) -> BootRecoveryReport;
  ```

- [ ] **Step 1: Teste da ordem** (usa o harness mock e o `TestEngine` que `restart_resume.rs` já monta; extrair o setup para `tests/common/mod.rs` se ainda não existir)

```rust
#[test]
fn stale_journal_is_resumed_before_streams_are_settled() {
    // journal de chat A fechado sem Done (stale) e chat B com entry streaming e sem journal stale
    let (sessions, doc_host, journals) = engine_with_journals(&[stale("A"), streaming_only("B")]);
    let report = recovery::run_boot_recovery(&sessions, &doc_host, journals);
    assert_eq!(report.resumed, 1);
    assert_eq!(report.streams_settled, 1);
    assert!(sessions.has_live_run("A"));
    assert_eq!(doc_host.open("B").unwrap().doc().read_entries().unwrap()[0].status, Some(MessageStatus::Aborted));
}
```

- [ ] **Step 2:** mover as funções (corpo idêntico), escrever `run_boot_recovery`, ligar no `lib.rs`.
- [ ] **Step 3:** `cargo test -p zeron-engine` → PASS, com `restart_resume`, `transcript_salvage`, `self_continued_quiesce` verdes.
- [ ] **Step 4:** `git commit -am "refactor(engine): boot recovery has one owner"`

### Task F2: `execute` sai do `DocHost`

**Files:**
- Modify: `crates/engine/src/doc_host.rs:3182-3405` (`execute` e `resolve_request_attachments` movem para `sessions.rs` como `SessionsEngine::execute_command(&self, handle: &Arc<ChatDocHandle>, entry: &SessionCommandEntry) -> Result<(SessionCommandStatus, Option<String>), EngineError>`)
- Modify: `crates/engine/src/doc_host.rs:2932` (`drain_commands` chama `sessions.execute_command(handle, &entry)`)
- Test: `crates/engine/tests/{e2e,run_controls_chat_id,queued_attachments,relay_delivery}.rs` sem alteração.

`execute` já recebe `sessions: &SessionsEngine` como primeiro parâmetro e chama `sessions.steer/interrupt/has_live_run/last_request` — o movimento é mecânico. `DocHost` continua dono de fila e ledger (marcar processado antes de executar), que é o contrato do `crates/engine/AGENTS.md`.

- [ ] **Step 1:** mover; onde `execute` usava `self.` do DocHost (ex.: `materialize_worktree`, `resolve_request_attachments`), passar `doc_host: &DocHost` como parâmetro ou chamar por `self.doc_host()` do `SessionsEngine`.
- [ ] **Step 2:** `cargo test -p zeron-engine` → PASS.
- [ ] **Step 3:** `git commit -am "refactor(engine): command execution belongs to SessionsEngine"`

### Task F3: Cortar a back-edge `DocHost → SessionsEngine` (gate de design)

Depois de F1 e F2, o que resta em `DocHostInner.sessions` é `drain_commands` precisando de `execute_command`. Opções: (a) `drain_commands` recebe `&SessionsEngine` do chamador; (b) `SessionsEngine` assina `watch_commands()` do handle e drena sozinho. Nenhuma das duas cabe neste plano sem ler quem chama `drain_commands` (grep antes: `grep -rn "drain_commands" crates/engine/src`).

- [ ] Escrever `docs/plans/<data>-run-owner-design.md` com o grep, a opção escolhida e o teste que prova que `set_sessions` sumiu. Só então abrir tarefa.

### Task F4: DOX pass da Fase F

- [ ] `crates/engine/AGENTS.md`: nas linhas sobre recovery ("Journal fechado com Done não prova…", "Carimbar a entry aborted…"), apontar `recovery::run_boot_recovery` como o dono e a ordem dos passes como contrato. Na linha "Executor é gated por ownership", registrar que `DocHost` gate/marca e `SessionsEngine::execute_command` executa.
- [ ] `git commit -am "docs(dox): phase F closeout"`

---

## Self-review

**Cobertura dos candidatos da revisão:** 1 → Fase E. 2 → B1-B4. 3 → A1-A3, B4 (triviais triplicadas somem com o driver em E). 4 → C1-C4. 5 → F1-F3 (F3 explicitamente gated). 6 → C5. 7 → C6. 8 → D1. 9 → A6 + D2. 10 → A4 + A5. Nenhum candidato ficou sem tarefa.

**Placeholders:** E3 e F3 têm gates de "parar e escrever design" em vez de código, de propósito: são os dois pontos onde o plano não tem evidência suficiente para prometer o diff.

**Consistência de nomes:** `tool_decode::decode` (B1, B2), `subagent_tag::{tag, SpawnLedger}` (B4), `workers_mcp::{resolve, resolve_for, WorkersMcpServer}` (A2), `zeron_rpc::{RpcMethod, info, methods}` (C1-C3), `EngineHandle::call_typed` (C3), `diff_sync::{DiffMode, capture_for_mode}` (C4), `process::{ProcessRunner, ProcessRequest, ProcessOutput}` (C5), `managed_usage::{ManagedUsage, ManagedUsageProvider, CredentialFingerprint}` (C6), `right_pane::{RightPane, SurfaceBody}` (D1), `AppState::sidebar_chats(now, filter, sort)` (D2), `driver::{LineWire, drive, run_line_process, DriverConfig, StdinMsg}` (E1-E3), `recovery::run_boot_recovery` (F1), `SessionsEngine::execute_command` (F2).

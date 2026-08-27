//! Comandos de setup de worktree: o arquivo no checkout e o executor que o
//! roda.
//!
//! Os dois andam juntos de proposito. O comet criava worktree
//! (`unpeel_core::worktrees::create`) e nao rodava nada depois — nao havia
//! leitor de `setup-worktree` em lugar nenhum do repo. Um formulario que
//! escreve um arquivo que ninguem le nao e metade da feature, e zero dela.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Caminho proprio do comet, relativo a raiz do projeto.
pub const COMET_CONFIG_PATH: &str = ".comet/worktree.json";
/// Caminho do Cursor. So e oferecido quando o arquivo ja existe: o comet nao
/// cria `.cursor/` em projeto que nao usa Cursor.
pub const CURSOR_CONFIG_PATH: &str = ".cursor/worktrees.json";

/// Teto por comando. Setup que trava seguraria a criacao do worktree para
/// sempre; 5 min e o mesmo teto do reference.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(300);

/// De onde a config foi lida — a linha "Config file" mostra isso.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConfigTarget {
    /// O caminho proprio do comet — o alvo de um projeto que ainda nao escolheu.
    #[default]
    Comet,
    Cursor,
}

impl ConfigTarget {
    pub fn relative_path(self) -> &'static str {
        match self {
            ConfigTarget::Comet => COMET_CONFIG_PATH,
            ConfigTarget::Cursor => CURSOR_CONFIG_PATH,
        }
    }
}

/// O arquivo. Cada campo aceita string OU lista no disco — o reference grava
/// das duas formas e um projeto compartilhado pode ter vindo de la.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeConfig {
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        rename = "setup-worktree"
    )]
    pub shared: Vec<String>,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        rename = "setup-worktree-unix"
    )]
    pub unix: Vec<String>,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        rename = "setup-worktree-windows"
    )]
    pub windows: Vec<String>,
}

impl WorktreeConfig {
    pub fn is_empty(&self) -> bool {
        self.shared.is_empty() && self.unix.is_empty() && self.windows.is_empty()
    }

    /// A lista que roda nesta plataforma. A compartilhada vence: e o que o
    /// rotulo "Falls back to commands above" promete na tela.
    pub fn commands_for_this_platform(&self) -> &[String] {
        if !self.shared.is_empty() {
            return &self.shared;
        }
        if cfg!(windows) {
            &self.windows
        } else {
            &self.unix
        }
    }
}

/// Uma config lida do disco, com o caminho de onde veio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedConfig {
    pub config: WorktreeConfig,
    pub target: ConfigTarget,
    pub path: PathBuf,
}

/// Linha comecando com `#` e comentario, nao comando. O reference filtra na
/// leitura e nos preservamos isso para nao executar o comentario de ninguem.
fn is_comment(command: &str) -> bool {
    command.trim_start().starts_with('#')
}

/// Aceita string solta ou lista, filtra comentario e vazio.
fn commands_from(value: Option<&serde_json::Value>) -> Vec<String> {
    let Some(value) = value else {
        return Vec::new();
    };
    let raw: Vec<String> = match value {
        serde_json::Value::String(single) => vec![single.clone()],
        serde_json::Value::Array(items) => items
            .iter()
            .filter_map(|item| item.as_str().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    };
    raw.into_iter()
        .filter(|command| !command.trim().is_empty() && !is_comment(command))
        .collect()
}

fn read(path: &Path) -> Option<WorktreeConfig> {
    let raw = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&raw).ok()?;
    Some(WorktreeConfig {
        shared: commands_from(value.get("setup-worktree")),
        unix: commands_from(value.get("setup-worktree-unix")),
        windows: commands_from(value.get("setup-worktree-windows")),
    })
}

/// Procura a config do projeto. Ordem: o caminho do comet, depois o do Cursor.
///
/// O comet vem primeiro (o reference poe o Cursor na frente, mas la o arquivo
/// proprio e legado); um projeto que tenha os dois e um projeto que escolheu o
/// do comet na tela.
pub fn detect(project_path: &Path) -> Option<DetectedConfig> {
    for target in [ConfigTarget::Comet, ConfigTarget::Cursor] {
        let path = project_path.join(target.relative_path());
        if let Some(config) = read(&path) {
            return Some(DetectedConfig {
                config,
                target,
                path,
            });
        }
    }
    None
}

/// Quais caminhos existem hoje. A tela so oferece o do Cursor quando ele ja
/// existe.
pub fn available_targets(project_path: &Path) -> BTreeMap<&'static str, bool> {
    [ConfigTarget::Comet, ConfigTarget::Cursor]
        .into_iter()
        .map(|target| {
            (
                target.relative_path(),
                project_path.join(target.relative_path()).is_file(),
            )
        })
        .collect()
}

/// Grava a config, criando o diretorio pai. Config vazia REMOVE o arquivo em
/// vez de deixar um `{}` que a deteccao ainda encontraria.
pub fn save(
    project_path: &Path,
    config: &WorktreeConfig,
    target: ConfigTarget,
) -> Result<PathBuf, String> {
    let path = project_path.join(target.relative_path());
    if config.is_empty() {
        if path.is_file() {
            std::fs::remove_file(&path).map_err(|error| error.to_string())?;
        }
        return Ok(path);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let body = serde_json::to_string_pretty(config).map_err(|error| error.to_string())?;
    std::fs::write(&path, body).map_err(|error| error.to_string())?;
    Ok(path)
}

/// O que o setup fez, para a tela poder dizer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SetupOutcome {
    pub commands_run: usize,
    /// `Some` no primeiro comando que falhou — a execucao para ali.
    pub failed: Option<String>,
    pub output: Vec<String>,
}

impl SetupOutcome {
    pub fn succeeded(&self) -> bool {
        self.failed.is_none()
    }
}

/// Roda um comando com teto de tempo, matando o processo se ele estourar.
///
/// `Command::output()` bloqueia para sempre, e um comando de setup travado
/// seguraria a criacao do worktree indefinidamente — sem sinal na tela e sem
/// como sair. `try_wait` em intervalo curto e o suficiente aqui: o teto e de
/// minutos, entao a granularidade do poll nao aparece.
fn run_one(
    worktree_path: &Path,
    main_path: &Path,
    command: &str,
    timeout: Duration,
) -> Result<(), String> {
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(worktree_path)
        .env("ROOT_WORKTREE_PATH", main_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;

    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => {
                let mut stderr = String::new();
                if let Some(mut pipe) = child.stderr.take() {
                    use std::io::Read as _;
                    let _ = pipe.read_to_string(&mut stderr);
                }
                let stderr = stderr.trim();
                return Err(if stderr.is_empty() {
                    format!("exit {status}")
                } else {
                    stderr.to_owned()
                });
            }
            Ok(None) if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("timed out after {:?}", timeout));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(error) => return Err(error.to_string()),
        }
    }
}

/// Roda o setup no worktree recem-criado.
///
/// `ROOT_WORKTREE_PATH` aponta para o checkout principal — e o que os comandos
/// usam para copiar `.env` e afins, e o token que a tela deixa copiar.
/// Para no primeiro comando que falha: seguir depois de um `bun install` que
/// quebrou so produz erro pior mais adiante.
pub fn run_setup(worktree_path: &Path, main_path: &Path, config: &WorktreeConfig) -> SetupOutcome {
    run_setup_with_timeout(worktree_path, main_path, config, COMMAND_TIMEOUT)
}

/// `run_setup` com teto explicito — existe para o teste do timeout poder usar
/// milissegundos em vez de esperar os cinco minutos de producao.
fn run_setup_with_timeout(
    worktree_path: &Path,
    main_path: &Path,
    config: &WorktreeConfig,
    timeout: Duration,
) -> SetupOutcome {
    let mut outcome = SetupOutcome::default();
    let commands = config.commands_for_this_platform();
    if commands.is_empty() {
        return outcome;
    }
    for command in commands {
        if command.trim().is_empty() {
            continue;
        }
        outcome.output.push(format!("$ {command}"));
        match run_one(worktree_path, main_path, command, timeout) {
            Ok(()) => outcome.commands_run += 1,
            Err(reason) => {
                outcome.output.push(reason);
                outcome.failed = Some(command.clone());
                return outcome;
            }
        }
    }
    outcome
}

/// `run_setup` depois de detectar a config do projeto. Projeto sem config roda
/// nada e isso NAO e erro.
pub fn run_setup_for_project(worktree_path: &Path, main_path: &Path) -> SetupOutcome {
    match detect(main_path) {
        Some(detected) => run_setup(worktree_path, main_path, &detected.config),
        None => SetupOutcome::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Dir(PathBuf);

    impl Dir {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!("comet-wt-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn write(&self, relative: &str, body: &str) {
            let path = self.0.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, body).unwrap();
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_project_without_config_detects_nothing() {
        let dir = Dir::new();
        assert_eq!(detect(dir.path()), None);
        assert!(!available_targets(dir.path()).values().any(|hit| *hit));
    }

    #[test]
    fn the_comet_path_wins_over_cursor() {
        let dir = Dir::new();
        dir.write(CURSOR_CONFIG_PATH, r#"{"setup-worktree":["cursor"]}"#);
        dir.write(COMET_CONFIG_PATH, r#"{"setup-worktree":["comet"]}"#);
        let found = detect(dir.path()).unwrap();
        assert_eq!(found.target, ConfigTarget::Comet);
        assert_eq!(found.config.shared, vec!["comet".to_owned()]);
    }

    #[test]
    fn cursor_is_detected_when_it_is_the_only_one() {
        let dir = Dir::new();
        dir.write(CURSOR_CONFIG_PATH, r#"{"setup-worktree":"bun install"}"#);
        let found = detect(dir.path()).unwrap();
        assert_eq!(found.target, ConfigTarget::Cursor);
        assert_eq!(found.config.shared, vec!["bun install".to_owned()]);
        assert!(available_targets(dir.path())[CURSOR_CONFIG_PATH]);
        assert!(!available_targets(dir.path())[COMET_CONFIG_PATH]);
    }

    #[test]
    fn comments_and_blanks_never_become_commands() {
        let dir = Dir::new();
        dir.write(
            COMET_CONFIG_PATH,
            r##"{"setup-worktree":["# so um comentario","","bun install","   "]}"##,
        );
        let found = detect(dir.path()).unwrap();
        assert_eq!(found.config.shared, vec!["bun install".to_owned()]);
    }

    #[test]
    fn saving_round_trips_and_an_empty_config_removes_the_file() {
        let dir = Dir::new();
        let config = WorktreeConfig {
            shared: vec!["bun install".to_owned()],
            unix: vec!["brew bundle".to_owned()],
            windows: Vec::new(),
        };
        let written = save(dir.path(), &config, ConfigTarget::Comet).unwrap();
        assert!(written.is_file());
        assert_eq!(detect(dir.path()).unwrap().config, config);

        save(dir.path(), &WorktreeConfig::default(), ConfigTarget::Comet).unwrap();
        assert!(
            !written.is_file(),
            "config vazia nao deixa arquivo para tras"
        );
        assert_eq!(detect(dir.path()), None);
    }

    #[test]
    fn the_shared_list_wins_over_the_platform_list() {
        let config = WorktreeConfig {
            shared: vec!["shared".to_owned()],
            unix: vec!["unix".to_owned()],
            windows: vec!["win".to_owned()],
        };
        assert_eq!(config.commands_for_this_platform(), ["shared".to_owned()]);
    }

    #[test]
    fn the_platform_list_runs_when_there_is_no_shared_one() {
        let config = WorktreeConfig {
            shared: Vec::new(),
            unix: vec!["unix".to_owned()],
            windows: vec!["win".to_owned()],
        };
        let expected = if cfg!(windows) { "win" } else { "unix" };
        assert_eq!(config.commands_for_this_platform(), [expected.to_owned()]);
    }

    #[test]
    fn setup_runs_every_command_and_exposes_the_main_checkout() {
        let main = Dir::new();
        let worktree = Dir::new();
        let config = WorktreeConfig {
            shared: vec![
                "echo primeiro > um.txt".to_owned(),
                "printf '%s' \"$ROOT_WORKTREE_PATH\" > raiz.txt".to_owned(),
            ],
            ..Default::default()
        };
        let outcome = run_setup(worktree.path(), main.path(), &config);
        assert!(outcome.succeeded(), "{outcome:?}");
        assert_eq!(outcome.commands_run, 2);
        assert!(worktree.path().join("um.txt").is_file());
        assert_eq!(
            std::fs::read_to_string(worktree.path().join("raiz.txt")).unwrap(),
            main.path().display().to_string(),
            "ROOT_WORKTREE_PATH tem que chegar no comando"
        );
    }

    #[test]
    fn a_failing_command_stops_the_run_and_is_named() {
        let main = Dir::new();
        let worktree = Dir::new();
        let config = WorktreeConfig {
            shared: vec![
                "exit 3".to_owned(),
                "echo nao devia rodar > tarde.txt".to_owned(),
            ],
            ..Default::default()
        };
        let outcome = run_setup(worktree.path(), main.path(), &config);
        assert!(!outcome.succeeded());
        assert_eq!(outcome.failed.as_deref(), Some("exit 3"));
        assert_eq!(outcome.commands_run, 0);
        assert!(
            !worktree.path().join("tarde.txt").exists(),
            "parou no primeiro que falhou"
        );
    }

    /// Sem isto um comando travado seguraria a criacao do worktree para
    /// sempre. Fica vermelho no instante em que o teto sair do caminho.
    #[test]
    fn a_hanging_command_is_killed_at_the_deadline() {
        let main = Dir::new();
        let worktree = Dir::new();
        let config = WorktreeConfig {
            shared: vec!["sleep 30".to_owned()],
            ..Default::default()
        };
        let started = std::time::Instant::now();
        let outcome = run_setup_with_timeout(
            worktree.path(),
            main.path(),
            &config,
            Duration::from_millis(150),
        );
        assert!(!outcome.succeeded());
        assert_eq!(outcome.failed.as_deref(), Some("sleep 30"));
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "matou no prazo, nao esperou o sleep: {:?}",
            started.elapsed()
        );
        assert!(
            outcome.output.iter().any(|line| line.contains("timed out")),
            "o motivo tem que aparecer: {:?}",
            outcome.output
        );
    }

    #[test]
    fn a_project_with_no_config_runs_nothing_and_that_is_not_a_failure() {
        let main = Dir::new();
        let worktree = Dir::new();
        let outcome = run_setup_for_project(worktree.path(), main.path());
        assert!(outcome.succeeded());
        assert_eq!(outcome.commands_run, 0);
    }
}

//! Estado de git de uma pasta de projeto, lido na hora.
//!
//! Nada disto e guardado: o reference (orchestrator.dev) calcula `gitStatus` e
//! `commitContext` a cada consulta em vez de persistir, e pela mesma razao —
//! um remote cacheado desatualiza e uma data ancorada em commit so existe se
//! for perguntada ao repositorio.
//!
//! `WorkersProject::git_branch` NAO serve de fonte: o campo e desserializado
//! de `gitBranch`, mas o host que o comet usa (`controller_host.rs`) nunca o
//! emite — so o host TUI emite — entao pela rota `comet-local` ele e sempre
//! `None`.
//!
//! Toda leitura aqui e total: pasta inexistente, pasta comum e repositorio
//! vazio devolvem a variante ausente, nunca `Err` e nunca panic. Uma linha da
//! tela pode ficar sem valor; o card inteiro nao pode sumir por causa disso.

use std::path::Path;
use std::process::{Command, Stdio};

/// O que a linha "Repository" precisa saber sobre uma pasta.
///
/// `owner`/`repo` NAO moram aqui: derivar isso e trabalho de
/// `zeron_engine::parse_git_remote`, e puxar a crate `engine` (loro, tokio,
/// rusqlite, reqwest) para dentro desta so por causa de um parser inflaria
/// ate o `cargo test` desta crate. Quem consome — a UI — ja depende das duas
/// e faz a derivacao a partir de `remote_url`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectGitStatus {
    pub is_repo: bool,
    pub has_remote: bool,
    pub remote_url: Option<String>,
    pub branch: Option<String>,
}

impl ProjectGitStatus {
    /// Oferecer `git init` so faz sentido numa pasta que existe e nao e repo.
    pub fn can_init(&self, path: &Path) -> bool {
        !self.is_repo && path.is_dir()
    }
}

/// O commit que era HEAD numa data — a ancora que a linha "Added" e a linha
/// "Last opened" mostram embaixo do valor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnchorCommit {
    pub hash: String,
    pub short_hash: String,
    pub subject: String,
    pub date: String,
}

/// Roda um comando git na pasta e devolve o stdout aparado, ou `None` para
/// qualquer falha — binario ausente, pasta inexistente, exit code diferente
/// de zero, saida vazia.
fn git(path: &Path, args: &[&str]) -> Option<String> {
    if !path.is_dir() {
        return None;
    }
    let output = Command::new("git")
        .args(args)
        .current_dir(path)
        .stdin(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!text.is_empty()).then_some(text)
}

/// Le o estado de git da pasta. Ver a nota do modulo: total por construcao.
pub fn status(path: &Path) -> ProjectGitStatus {
    if git(path, &["rev-parse", "--git-dir"]).is_none() {
        return ProjectGitStatus::default();
    }
    let remote_url = git(path, &["remote", "get-url", "origin"]);
    ProjectGitStatus {
        is_repo: true,
        has_remote: remote_url.is_some(),
        remote_url,
        // Vazio em detached HEAD e em repo sem commit — `git` ja devolve
        // `None` para saida vazia, entao os dois caem no mesmo lugar.
        branch: git(path, &["branch", "--show-current"]),
    }
}

/// O ultimo commit que existia numa data (`git log -1 --until`).
///
/// `None` para nao-repo, repo vazio e datas anteriores ao primeiro commit — o
/// `git log` devolve saida vazia com exit 0 nos dois ultimos casos, e
/// [`git`] ja converte isso em ausencia.
pub fn commit_at(path: &Path, unix_ms: u64) -> Option<AnchorCommit> {
    // O offset NAO e decorativo: medido nesta maquina, `--until=@86400`
    // e `--until=86400 +0000` sao aceitos e o filtro e IGNORADO em silencio
    // (o log devolve HEAD como se nao houvesse data). So `@<segundos> <tz>` e
    // ISO 8601 filtram de verdade — e esta forma evita depender de chrono so
    // para formatar uma data. `a_date_before_the_first_commit_has_no_anchor`
    // e a rede: ela pegou exatamente esse silencio.
    let until = format!("@{} +0000", unix_ms / 1_000);
    let line = git(
        path,
        &[
            "log",
            "-1",
            &format!("--until={until}"),
            "--format=%H%x1f%h%x1f%s%x1f%cI",
        ],
    )?;
    let mut fields = line.split('\u{1f}');
    let hash = fields.next()?.to_owned();
    let short_hash = fields.next()?.to_owned();
    if hash.is_empty() || short_hash.is_empty() {
        return None;
    }
    Some(AnchorCommit {
        subject: fields.next().unwrap_or_default().to_owned(),
        date: fields.next().unwrap_or_default().to_owned(),
        hash,
        short_hash,
    })
}

/// Publico ou privado — o dialogo pergunta as duas como acoes distintas, nunca
/// com um default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Private,
}

impl Visibility {
    fn flag(self) -> &'static str {
        match self {
            Visibility::Public => "--public",
            Visibility::Private => "--private",
        }
    }
}

/// `git init` na pasta. Recusa pasta que ja e repo em vez de rodar por cima.
pub fn init(path: &Path) -> Result<(), String> {
    if !path.is_dir() {
        return Err(format!("{} nao existe", path.display()));
    }
    if status(path).is_repo {
        return Err("ja e um repositorio git".to_owned());
    }
    let done = Command::new("git")
        .arg("init")
        .current_dir(path)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| error.to_string())?;
    if done.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&done.stderr).trim().to_owned())
    }
}

/// Os argumentos do `gh repo create`. Separado da execucao para poder ser
/// testado sem criar repositorio no GitHub de ninguem.
///
/// `--push` so entra quando ja existe commit: `gh` falha ao empurrar um repo
/// sem HEAD, e o caso "pasta inicializada, nada commitado" e comum.
pub fn publish_args(path: &Path, visibility: Visibility, has_commits: bool) -> Vec<String> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("project");
    let mut args = vec![
        "repo".to_owned(),
        "create".to_owned(),
        name.to_owned(),
        visibility.flag().to_owned(),
        "--source".to_owned(),
        path.display().to_string(),
        "--remote".to_owned(),
        "origin".to_owned(),
    ];
    if has_commits {
        args.push("--push".to_owned());
    }
    args
}

/// Cria o repositorio no GitHub e aponta `origin` para ele.
///
/// Recusa antes de chamar `gh` uma pasta que nao e repo ou que ja tem remote —
/// as duas fariam o `gh` falhar com mensagem pior que a nossa.
pub fn publish_to_github(path: &Path, visibility: Visibility) -> Result<(), String> {
    let read = status(path);
    if !read.is_repo {
        return Err("nao e um repositorio git".to_owned());
    }
    if read.has_remote {
        return Err("o repositorio ja tem um remote".to_owned());
    }
    let has_commits = git(path, &["rev-parse", "HEAD"]).is_some();
    let done = Command::new("gh")
        .args(publish_args(path, visibility, has_commits))
        .current_dir(path)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                "GitHub CLI (gh) nao encontrado. Instale em https://cli.github.com".to_owned()
            } else {
                error.to_string()
            }
        })?;
    if done.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&done.stderr).trim().to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Repo(std::path::PathBuf);

    impl Repo {
        fn empty() -> Self {
            let dir = std::env::temp_dir().join(format!("comet-git-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&dir).unwrap();
            let repo = Self(dir);
            repo.run(&["init", "-q"]);
            repo.run(&["config", "user.email", "t@example.com"]);
            repo.run(&["config", "user.name", "T"]);
            repo
        }

        fn with_commit() -> Self {
            let repo = Self::empty();
            std::fs::write(repo.0.join("a.txt"), "a").unwrap();
            repo.run(&["add", "a.txt"]);
            repo.run(&["commit", "-q", "-m", "primeiro commit"]);
            repo
        }

        fn run(&self, args: &[&str]) {
            let status = Command::new("git")
                .args(args)
                .current_dir(&self.0)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .unwrap();
            assert!(status.success(), "git {args:?}");
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Repo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }

    #[test]
    fn publish_args_only_push_when_there_is_something_to_push() {
        let path = Path::new("/tmp/meu-projeto");
        let with = publish_args(path, Visibility::Private, true);
        assert_eq!(with[2], "meu-projeto", "o nome vem do basename");
        assert_eq!(with[3], "--private");
        assert_eq!(with.last().unwrap(), "--push");

        let without = publish_args(path, Visibility::Public, false);
        assert_eq!(without[3], "--public");
        assert!(
            !without.contains(&"--push".to_owned()),
            "repo sem commit nao pode receber --push: {without:?}"
        );
    }

    #[test]
    fn init_refuses_a_folder_that_is_already_a_repo() {
        let repo = Repo::with_commit();
        assert!(init(repo.path()).is_err());
    }

    #[test]
    fn init_creates_a_repo_in_a_plain_folder() {
        let dir = std::env::temp_dir().join(format!("comet-init-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(!status(&dir).is_repo);
        init(&dir).unwrap();
        assert!(status(&dir).is_repo);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Nao chama a rede: para antes do `gh` justamente porque `has_remote`.
    #[test]
    fn publishing_a_repo_that_already_has_a_remote_is_refused_locally() {
        let repo = Repo::with_commit();
        repo.run(&["remote", "add", "origin", "https://example.com/x.git"]);
        let error = publish_to_github(repo.path(), Visibility::Private).unwrap_err();
        assert!(error.contains("ja tem um remote"), "{error}");
    }

    #[test]
    fn a_missing_folder_is_absent_not_an_error() {
        let missing = std::env::temp_dir().join("comet-git-does-not-exist-ever");
        assert_eq!(status(&missing), ProjectGitStatus::default());
        assert_eq!(commit_at(&missing, now_ms()), None);
        assert!(!status(&missing).can_init(&missing), "nem pra init serve");
    }

    #[test]
    fn a_plain_folder_is_not_a_repo_but_can_be_initialised() {
        let dir = std::env::temp_dir().join(format!("comet-plain-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let read = status(&dir);
        assert!(!read.is_repo);
        assert!(read.can_init(&dir));
        assert_eq!(commit_at(&dir, now_ms()), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_repo_without_a_remote_reports_no_remote() {
        let repo = Repo::with_commit();
        let read = status(repo.path());
        assert!(read.is_repo);
        assert!(!read.has_remote);
        assert_eq!(read.remote_url, None);
        assert!(!read.can_init(repo.path()));
    }

    #[test]
    fn a_repo_with_a_remote_reports_its_url() {
        let repo = Repo::with_commit();
        repo.run(&[
            "remote",
            "add",
            "origin",
            "https://github.com/guilhermexp/comet.git",
        ]);
        let read = status(repo.path());
        assert!(read.has_remote);
        assert_eq!(
            read.remote_url.as_deref(),
            Some("https://github.com/guilhermexp/comet.git")
        );
    }

    #[test]
    fn an_empty_repo_has_no_anchor_commit() {
        let repo = Repo::empty();
        assert!(status(repo.path()).is_repo);
        assert_eq!(commit_at(repo.path(), now_ms()), None);
    }

    #[test]
    fn a_date_before_the_first_commit_has_no_anchor() {
        let repo = Repo::with_commit();
        // 1970-01-02: anterior a qualquer commit que este teste possa criar.
        assert_eq!(commit_at(repo.path(), 86_400_000), None);
    }

    #[test]
    fn the_anchor_at_now_is_the_head_commit() {
        let repo = Repo::with_commit();
        let anchor = commit_at(repo.path(), now_ms()).expect("HEAD existe");
        assert_eq!(anchor.subject, "primeiro commit");
        assert_eq!(anchor.hash.len(), 40);
        assert!(anchor.hash.starts_with(&anchor.short_hash));
        assert!(anchor.date.starts_with("20"), "ISO: {}", anchor.date);
    }
}

//! O ledger de projetos: tudo que o app ja viu, por path canonico.
//!
//! O working set (`projects[]` em `app-state.json`) e o que a sidebar mostra,
//! e [`LocalWorkersClient::remove_project`](super::LocalWorkersClient::remove_project)
//! o poda — apagando o registro E todas as sessoes debaixo dele. Sem um
//! segundo lugar, a data de entrada, o icone e a propria existencia do
//! projeto morrem nessa chamada.
//!
//! O ledger mora na chave irma `comet_projects`, no MESMO arquivo: mesmo
//! flock, mesmo rename atomico, mesma recusa de dropar chave nao modelada
//! (`unpeel_core::app_state`). A sobrevivencia e estrutural, nao uma regra
//! que alguem precisa lembrar — `remove_project_record` enumera as tres
//! chaves que limpa (`projects`, cores de pasta, modos de ordenacao) e esta
//! nao esta entre elas.
//!
//! A chave e o PATH, nunca o id: `add_project` cunha um `comet-<uuid>` novo a
//! cada entrada, entao remover e readicionar a mesma pasta orfanaria o
//! historico dela.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Chave de topo do ledger em `app-state.json`.
pub const LEDGER_KEY: &str = "comet_projects";

/// Uma entrada do ledger — so o que NAO da pra recalcular. Estado de git,
/// commits ancora e contagem de sessoes sao lidos frescos a cada abertura,
/// nunca guardados aqui.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerProject {
    pub path: String,
    pub name: String,
    pub added_at_unix_ms: u64,
    /// Ultimo sinal de atividade real que vimos enquanto o projeto estava no
    /// working set. E o que a linha "Last opened" mostra depois que ele sai.
    pub last_seen_at_unix_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_path: Option<String>,
}

/// Um projeto vivo, reduzido ao que a reconciliacao precisa. O chamador mapeia
/// de `WorkersProject` para ca para que [`reconcile`] continue puro sobre dado
/// simples — e testavel com literais de quatro campos em vez de onze.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveProject {
    pub id: String,
    pub path: String,
    pub name: String,
    /// `max(session.updated_at_unix_ms)` entre as sessoes deste projeto.
    pub last_activity_unix_ms: Option<u64>,
}

/// Uma linha da tela de Settings > Projects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRow {
    /// `None` quando o projeto so existe no ledger: sem id nao ha o que
    /// lancar, renomear ou revelar pelo registro vivo.
    pub project_id: Option<String>,
    pub path: String,
    pub name: String,
    pub added_at_unix_ms: u64,
    pub last_opened_at_unix_ms: u64,
    pub icon_path: Option<String>,
}

impl ProjectRow {
    pub fn is_live(&self) -> bool {
        self.project_id.is_some()
    }
}

/// O resultado de uma passada de reconciliacao.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reconciled {
    /// As linhas a renderizar, da atividade mais recente para a mais antiga.
    pub rows: Vec<ProjectRow>,
    /// O ledger como precisa ficar em disco depois desta passada.
    pub ledger: Vec<LedgerProject>,
    /// `false` quando `ledger` e identico ao que entrou — deixa o chamador
    /// pular a escrita. Sem isso, abrir a tela escreveria num arquivo
    /// compartilhado e travado a cada render.
    pub dirty: bool,
}

/// Normaliza um path para servir de chave. Textual de proposito: `add_project`
/// ja grava o path canonico do sistema de arquivos, entao aqui basta remover
/// separador final e unificar a barra — e isso mantem [`reconcile`] puro.
fn key(path: &str) -> String {
    let trimmed = path.replace('\\', "/");
    let trimmed = trimmed.trim_end_matches('/');
    if trimmed.is_empty() {
        "/".to_owned()
    } else {
        trimmed.to_owned()
    }
}

/// Junta ledger e working set. Puro: nao le nem escreve nada.
///
/// Tres ramos, na ordem em que aparecem no design da change:
/// - vivo em ambos: nome e path vem do working set, `added_at` do ledger;
/// - so no working set: primeira vista, o ledger ganha a linha com `added_at = now`;
/// - so no ledger: valores congelados, sem id.
///
/// `last_seen_at` so avanca com sinal de atividade REAL, nunca com `now`. Um
/// projeto vivo e parado nao suja o ledger, e o valor congelado que a linha
/// mostra depois vira "a ultima vez que houve trabalho ali" em vez de "a
/// ultima vez que a tela abriu".
pub fn reconcile(ledger: &[LedgerProject], live: &[LiveProject], now: u64) -> Reconciled {
    let mut remaining: HashMap<String, LedgerProject> = ledger
        .iter()
        .map(|entry| (key(&entry.path), entry.clone()))
        .collect();

    let mut rows = Vec::with_capacity(live.len() + remaining.len());
    let mut next: Vec<LedgerProject> = Vec::with_capacity(remaining.len() + live.len());
    let mut seen_live_paths = HashSet::with_capacity(live.len());

    for project in live {
        let entry_key = key(&project.path);
        if !seen_live_paths.insert(entry_key.clone()) {
            continue;
        }
        let activity = project.last_activity_unix_ms;
        let entry = match remaining.remove(&entry_key) {
            Some(previous) => LedgerProject {
                path: project.path.clone(),
                name: project.name.clone(),
                added_at_unix_ms: previous.added_at_unix_ms,
                last_seen_at_unix_ms: previous
                    .last_seen_at_unix_ms
                    .max(activity.unwrap_or_default()),
                icon_path: previous.icon_path,
            },
            None => LedgerProject {
                path: project.path.clone(),
                name: project.name.clone(),
                added_at_unix_ms: now,
                last_seen_at_unix_ms: activity.unwrap_or(now),
                icon_path: None,
            },
        };
        rows.push(ProjectRow {
            project_id: Some(project.id.clone()),
            path: entry.path.clone(),
            name: entry.name.clone(),
            added_at_unix_ms: entry.added_at_unix_ms,
            last_opened_at_unix_ms: activity.unwrap_or(entry.last_seen_at_unix_ms),
            icon_path: entry.icon_path.clone(),
        });
        next.push(entry);
    }

    let mut orphans: Vec<LedgerProject> = remaining.into_values().collect();
    orphans.sort_by(|left, right| left.path.cmp(&right.path));
    for entry in orphans {
        rows.push(ProjectRow {
            project_id: None,
            path: entry.path.clone(),
            name: entry.name.clone(),
            added_at_unix_ms: entry.added_at_unix_ms,
            last_opened_at_unix_ms: entry.last_seen_at_unix_ms,
            icon_path: entry.icon_path.clone(),
        });
        next.push(entry);
    }

    // Ordem estavel em disco para que `dirty` compare conteudo, nao arranjo.
    next.sort_by_key(|entry| key(&entry.path));
    let mut before = ledger.to_vec();
    before.sort_by_key(|entry| key(&entry.path));
    let dirty = before != next;

    rows.sort_by(|left, right| {
        right
            .last_opened_at_unix_ms
            .cmp(&left.last_opened_at_unix_ms)
            .then_with(|| left.name.cmp(&right.name))
    });

    Reconciled {
        rows,
        ledger: next,
        dirty,
    }
}

fn parse(state: &Value) -> Vec<LedgerProject> {
    state
        .get(LEDGER_KEY)
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| serde_json::from_value(entry.clone()).ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Le o ledger do `app-state.json` real.
pub fn read() -> Result<Vec<LedgerProject>, String> {
    Ok(parse(&unpeel_core::app_state::load()?))
}

/// Le o ledger de um arquivo de estado explicito — a variante que os testes
/// usam para nunca encostar no registro real desta maquina.
pub fn read_at(path: &Path) -> Result<Vec<LedgerProject>, String> {
    Ok(parse(&unpeel_core::app_state::load_for_edit_at(path)?))
}

fn encode(entries: &[LedgerProject]) -> Result<Value, String> {
    serde_json::to_value(entries).map_err(|error| error.to_string())
}

/// Substitui o ledger inteiro. Passa por `app_state::edit`, entao herda o
/// flock e a recusa de dropar qualquer outra chave de topo.
pub fn write(entries: &[LedgerProject]) -> Result<(), String> {
    let encoded = encode(entries)?;
    unpeel_core::app_state::edit(|state| {
        state.insert(LEDGER_KEY.to_owned(), encoded);
        Ok(())
    })
}

/// `write` contra um arquivo de estado explicito — ver [`read_at`].
pub fn write_at(path: &Path, entries: &[LedgerProject]) -> Result<(), String> {
    let encoded = encode(entries)?;
    unpeel_core::app_state::edit_at(path, |state| {
        state.insert(LEDGER_KEY.to_owned(), encoded);
        Ok(())
    })
}

/// [`forget_at`] contra o `app-state.json` real.
pub fn forget(project_path: &str) -> Result<bool, String> {
    forget_at(&state_path(), project_path)
}

/// [`set_icon_at`] contra o `app-state.json` real.
pub fn set_icon(project_path: &str, icon: Option<&str>) -> Result<(), String> {
    set_icon_at(&state_path(), project_path, icon)
}

/// O arquivo de estado real. `unpeel_core::app_state` resolve isso internamente
/// mas nao expoe o caminho, e as variantes `_at` precisam dele.
fn state_path() -> std::path::PathBuf {
    unpeel_core::app_paths::app_state_path()
}

/// Esquece um projeto: tira a linha do ledger. Nao toca em `projects[]` nem em
/// sessao nenhuma — e o oposto de `remove_project`, que faz exatamente o
/// contrario.
pub fn forget_at(path: &Path, project_path: &str) -> Result<bool, String> {
    let target = key(project_path);
    let mut entries = read_at(path)?;
    let before = entries.len();
    entries.retain(|entry| key(&entry.path) != target);
    if entries.len() == before {
        return Ok(false);
    }
    write_at(path, &entries)?;
    Ok(true)
}

/// Grava o caminho do icone de um projeto, criando a linha se ela nao existir.
pub fn set_icon_at(path: &Path, project_path: &str, icon: Option<&str>) -> Result<(), String> {
    let target = key(project_path);
    let mut entries = read_at(path)?;
    let Some(entry) = entries.iter_mut().find(|entry| key(&entry.path) == target) else {
        return Err(format!("unknown project path: {project_path}"));
    };
    entry.icon_path = icon.map(str::to_owned);
    write_at(path, &entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_state<T>(body: impl FnOnce(&Path) -> T) -> T {
        let dir = std::env::temp_dir().join(format!("comet-ledger-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let outcome = body(&dir.join("app-state.json"));
        let _ = std::fs::remove_dir_all(&dir);
        outcome
    }

    fn entry(path: &str, added: u64, seen: u64) -> LedgerProject {
        LedgerProject {
            path: path.to_owned(),
            name: path.rsplit('/').next().unwrap_or(path).to_owned(),
            added_at_unix_ms: added,
            last_seen_at_unix_ms: seen,
            icon_path: None,
        }
    }

    fn live(id: &str, path: &str, activity: Option<u64>) -> LiveProject {
        LiveProject {
            id: id.to_owned(),
            path: path.to_owned(),
            name: path.rsplit('/').next().unwrap_or(path).to_owned(),
            last_activity_unix_ms: activity,
        }
    }

    /// A razao de existir da change: a poda que `remove_project` faz sobre
    /// `projects` nao pode levar o ledger junto. Fica vermelho no instante em
    /// que o ledger virar filho de `projects`.
    #[test]
    fn ledger_survives_the_pruning_that_removes_a_project() {
        with_state(|path| {
            unpeel_core::app_state::edit_at(path, |state| {
                state.insert(
                    "projects".to_owned(),
                    serde_json::json!([{ "id": "comet-1", "path": "/tmp/one" }]),
                );
                Ok(())
            })
            .unwrap();
            write_at(path, &[entry("/tmp/one", 10, 20)]).unwrap();

            // Exatamente o que `remove_project_record` faz: esvazia `projects`
            // e as duas chaves de organizacao, e mais nada.
            unpeel_core::app_state::edit_at(path, |state| {
                let projects = state
                    .get_mut("projects")
                    .and_then(Value::as_array_mut)
                    .unwrap();
                projects
                    .retain(|project| project.get("id").and_then(Value::as_str) != Some("comet-1"));
                state.remove("project_folder_colors");
                state.remove("session_sort_modes");
                Ok(())
            })
            .unwrap();

            let survivors = read_at(path).unwrap();
            assert_eq!(survivors, vec![entry("/tmp/one", 10, 20)]);
        });
    }

    /// O arquivo e contrato entre frontends: escrever o ledger nao pode custar
    /// uma chave que esta crate nem modela.
    #[test]
    fn writing_the_ledger_keeps_keys_this_crate_does_not_model() {
        with_state(|path| {
            unpeel_core::app_state::edit_at(path, |state| {
                state.insert("theme".to_owned(), Value::String("nord".to_owned()));
                Ok(())
            })
            .unwrap();
            write_at(path, &[entry("/tmp/one", 1, 2)]).unwrap();

            let state = unpeel_core::app_state::load_for_edit_at(path).unwrap();
            assert_eq!(state.get("theme").and_then(Value::as_str), Some("nord"));
            assert_eq!(read_at(path).unwrap().len(), 1);
        });
    }

    #[test]
    fn a_project_seen_for_the_first_time_is_recorded() {
        let outcome = reconcile(&[], &[live("comet-1", "/tmp/one", None)], 500);
        assert!(outcome.dirty);
        assert_eq!(outcome.ledger.len(), 1);
        assert_eq!(outcome.ledger[0].added_at_unix_ms, 500);
        assert_eq!(outcome.rows[0].project_id.as_deref(), Some("comet-1"));
    }

    #[test]
    fn a_project_removed_from_the_working_set_becomes_a_ledger_only_row() {
        let outcome = reconcile(&[entry("/tmp/one", 10, 40)], &[], 900);
        assert!(!outcome.dirty, "nada mudou, nao deve reescrever o arquivo");
        assert_eq!(outcome.rows.len(), 1);
        assert_eq!(outcome.rows[0].project_id, None);
        assert_eq!(outcome.rows[0].last_opened_at_unix_ms, 40);
    }

    /// D-03: a chave e o path, entao a mesma pasta readicionada sob um id novo
    /// reencontra a propria historia.
    #[test]
    fn re_adding_a_folder_keeps_the_original_added_date() {
        let outcome = reconcile(
            &[entry("/tmp/one", 10, 40)],
            &[live("comet-brand-new", "/tmp/one", Some(80))],
            900,
        );
        assert_eq!(outcome.ledger[0].added_at_unix_ms, 10, "nao e 900");
        assert_eq!(outcome.ledger[0].last_seen_at_unix_ms, 80);
        assert_eq!(
            outcome.rows[0].project_id.as_deref(),
            Some("comet-brand-new")
        );
    }

    /// Sem isto, abrir a tela escreveria no arquivo compartilhado a cada render.
    #[test]
    fn an_idle_live_project_does_not_dirty_the_ledger() {
        let outcome = reconcile(
            &[entry("/tmp/one", 10, 40)],
            &[live("comet-1", "/tmp/one", Some(40))],
            9_999,
        );
        assert!(!outcome.dirty);
        assert_eq!(outcome.ledger[0].last_seen_at_unix_ms, 40);
    }

    #[test]
    fn rows_are_ordered_by_last_activity_desc() {
        let outcome = reconcile(
            &[entry("/tmp/cold", 1, 5)],
            &[
                live("comet-1", "/tmp/warm", Some(100)),
                live("comet-2", "/tmp/hot", Some(300)),
            ],
            900,
        );
        let paths: Vec<&str> = outcome.rows.iter().map(|row| row.path.as_str()).collect();
        assert_eq!(paths, vec!["/tmp/hot", "/tmp/warm", "/tmp/cold"]);
    }

    #[test]
    fn a_trailing_separator_is_the_same_project() {
        let outcome = reconcile(
            &[entry("/tmp/one/", 10, 40)],
            &[live("comet-1", "/tmp/one", Some(50))],
            900,
        );
        assert_eq!(outcome.rows.len(), 1, "seria 2 se a chave fosse literal");
        assert_eq!(outcome.rows[0].added_at_unix_ms, 10);
    }

    /// Duas entidades do working set podem apontar para a mesma pasta (um
    /// projeto e um grupo organizacional). A chave do ledger e o path, entao
    /// a segunda nunca pode cunhar outra row/entrada com a mesma identidade.
    #[test]
    fn duplicate_live_paths_produce_one_row_and_one_ledger_entry() {
        let outcome = reconcile(
            &[],
            &[
                live("comet-project", "/tmp/repo", Some(10)),
                live("comet-group", "/tmp/repo", Some(20)),
            ],
            30,
        );

        assert_eq!(outcome.rows.len(), 1);
        assert_eq!(outcome.ledger.len(), 1);
        assert_eq!(outcome.rows[0].project_id.as_deref(), Some("comet-project"));
    }

    #[test]
    fn forgetting_removes_only_the_named_project() {
        with_state(|path| {
            write_at(path, &[entry("/tmp/one", 1, 2), entry("/tmp/two", 3, 4)]).unwrap();
            assert!(forget_at(path, "/tmp/one/").unwrap());
            let left = read_at(path).unwrap();
            assert_eq!(left.len(), 1);
            assert_eq!(left[0].path, "/tmp/two");
            assert!(!forget_at(path, "/tmp/one").unwrap(), "segunda vez e no-op");
        });
    }

    #[test]
    fn an_icon_round_trips_and_survives_a_reconcile() {
        with_state(|path| {
            write_at(path, &[entry("/tmp/one", 1, 2)]).unwrap();
            set_icon_at(path, "/tmp/one", Some("/icons/one.png")).unwrap();

            let outcome = reconcile(
                &read_at(path).unwrap(),
                &[live("comet-1", "/tmp/one", Some(9))],
                900,
            );
            assert_eq!(outcome.rows[0].icon_path.as_deref(), Some("/icons/one.png"));
            assert_eq!(
                outcome.ledger[0].icon_path.as_deref(),
                Some("/icons/one.png")
            );
        });
    }
}

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
//! A chave historica continua sendo o PATH para compatibilidade, mas a
//! associacao logica de repositorio/checkouts vive em
//! `comet_project_identity`. Assim a linha pode sobreviver a um id de execucao
//! removido sem fazer a UI tratar cada worktree como um projeto novo.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use super::project_identity::{CheckoutAvailability, CheckoutKind, CheckoutOwnership};

/// Chave de topo do ledger em `app-state.json`.
pub const LEDGER_KEY: &str = "comet_projects";
/// Display name used when a repository has linked checkouts but no registered
/// primary checkout to serve as its logical project row.
pub const UNREGISTERED_PRIMARY_NAME: &str = "Principal not registered";

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

/// Estado da associacao duravel com um projeto logico.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AssociationState {
    #[default]
    Known,
    Pending,
    Conflict,
}

/// Uma linha de checkout da tela de Settings > Projects.
///
/// Os seis campos adicionados ao contrato anterior sao aditivos e possuem
/// defaults no construtor de compatibilidade. A reconciliacao legada continua
/// produzindo uma linha por path ate o adapter conseguir publicar a identidade
/// do repositorio. Isso deixa a migracao incremental e evita agrupar clones
/// distintos por nome ou remote.
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
    /// Identidade local do repositorio. `None` significa legado ainda nao
    /// reconciliado, nao que duas entradas podem ser fundidas.
    pub repository_id: Option<String>,
    /// ID estavel do checkout quando ele continua apenas no historico.
    pub checkout_id: Option<String>,
    pub checkout_kind: Option<CheckoutKind>,
    pub checkout_ownership: Option<CheckoutOwnership>,
    pub checkout_availability: Option<CheckoutAvailability>,
    pub current_branch: Option<String>,
    pub last_known_branch: Option<String>,
    pub association: AssociationState,
    pub archived: bool,
}

impl ProjectRow {
    pub fn is_live(&self) -> bool {
        self.project_id.is_some()
    }

    /// Um registro legado sem observacao continua elegivel para as acoes
    /// antigas; depois da reconciliacao, `Missing` e `ProbeFailed` bloqueiam
    /// filesystem ate que o checkout volte a ser verificavel.
    pub fn is_available(&self) -> bool {
        !matches!(
            self.checkout_availability,
            Some(CheckoutAvailability::Missing)
        ) && self.path_is_present_or_legacy()
    }

    fn path_is_present_or_legacy(&self) -> bool {
        !matches!(
            self.checkout_availability,
            Some(CheckoutAvailability::Missing | CheckoutAvailability::ProbeFailed)
        )
    }

    pub fn is_pending(&self) -> bool {
        matches!(
            self.association,
            AssociationState::Pending | AssociationState::Conflict
        )
    }

    pub fn display_branch(&self) -> Option<&str> {
        self.current_branch
            .as_deref()
            .or(self.last_known_branch.as_deref())
    }
}

/// Um projeto logico e o agrupador usado por Settings. `checkouts` contem
/// registros disponiveis e historicos; a tela decide como separar visualmente
/// por `checkout_availability` sem remover o row do resultado da busca.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectGroup {
    pub id: String,
    pub name: String,
    pub icon_path: Option<String>,
    pub added_at_unix_ms: u64,
    pub last_opened_at_unix_ms: u64,
    pub checkouts: Vec<ProjectRow>,
}

impl ProjectGroup {
    pub fn selected_checkout(&self) -> Option<&ProjectRow> {
        self.checkouts
            .iter()
            .find(|row| row.checkout_kind == Some(CheckoutKind::Primary) && row.is_available())
            .or_else(|| self.checkouts.iter().find(|row| row.is_available()))
            .or_else(|| {
                self.checkouts
                    .iter()
                    .find(|row| row.checkout_kind == Some(CheckoutKind::Primary))
            })
            .or_else(|| self.checkouts.first())
    }

    pub fn has_pending(&self) -> bool {
        self.checkouts.iter().any(ProjectRow::is_pending)
    }

    pub fn available_checkouts(&self) -> impl Iterator<Item = &ProjectRow> {
        self.checkouts.iter().filter(|row| row.is_available())
    }

    pub fn historical_checkouts(&self) -> impl Iterator<Item = &ProjectRow> {
        self.checkouts.iter().filter(|row| {
            matches!(
                row.checkout_availability,
                Some(CheckoutAvailability::Missing)
            )
        })
    }
}

/// Agrupa somente registros com a mesma identidade persistida. Linhas legadas
/// sem `repository_id` permanecem separadas, pois path/basename/remote nao sao
/// evidencia suficiente para juntar clones ou branches antigas.
pub fn group_rows(rows: &[ProjectRow]) -> Vec<ProjectGroup> {
    let mut groups: Vec<ProjectGroup> = Vec::new();
    let mut indexes: HashMap<String, usize> = HashMap::new();

    for row in rows {
        let group_id = row
            .repository_id
            .clone()
            .unwrap_or_else(|| format!("legacy:{}", key(&row.path)));
        let index = if let Some(index) = indexes.get(&group_id).copied() {
            index
        } else {
            let index = groups.len();
            indexes.insert(group_id.clone(), index);
            groups.push(ProjectGroup {
                id: group_id,
                name: row.name.clone(),
                icon_path: row.icon_path.clone(),
                added_at_unix_ms: row.added_at_unix_ms,
                last_opened_at_unix_ms: row.last_opened_at_unix_ms,
                checkouts: Vec::new(),
            });
            index
        };

        let group = &mut groups[index];
        group.added_at_unix_ms = group.added_at_unix_ms.min(row.added_at_unix_ms);
        if row.last_opened_at_unix_ms > group.last_opened_at_unix_ms {
            group.last_opened_at_unix_ms = row.last_opened_at_unix_ms;
        }
        if group.icon_path.is_none() {
            group.icon_path = row.icon_path.clone();
        }
        if row.checkout_kind == Some(CheckoutKind::Primary) {
            group.name = row.name.clone();
            group.icon_path = row.icon_path.clone().or(group.icon_path.clone());
        }
        group.checkouts.push(row.clone());
    }

    for group in &mut groups {
        group.checkouts.sort_by(|left, right| {
            let left_primary = left.checkout_kind == Some(CheckoutKind::Primary);
            let right_primary = right.checkout_kind == Some(CheckoutKind::Primary);
            right_primary
                .cmp(&left_primary)
                .then_with(|| {
                    right
                        .last_opened_at_unix_ms
                        .cmp(&left.last_opened_at_unix_ms)
                })
                .then_with(|| left.name.cmp(&right.name))
        });
        if let Some(primary) = group
            .checkouts
            .iter()
            .find(|row| row.checkout_kind == Some(CheckoutKind::Primary))
        {
            group.name = primary.name.clone();
            if primary.icon_path.is_some() {
                group.icon_path = primary.icon_path.clone();
            }
        } else if group
            .checkouts
            .iter()
            .any(|row| row.repository_id.is_some())
        {
            group.name = UNREGISTERED_PRIMARY_NAME.to_owned();
        }
    }

    groups.sort_by(|left, right| {
        right
            .last_opened_at_unix_ms
            .cmp(&left.last_opened_at_unix_ms)
            .then_with(|| left.name.cmp(&right.name))
    });
    groups
}

/// Busca pelo nome do projeto ou por qualquer nome, branch ou path de seus
/// checkouts. Devolve o checkout que deve receber foco quando a query encontra
/// um filho, preservando a navegabilidade de historicos ausentes.
pub fn group_matches_query<'a>(group: &'a ProjectGroup, query: &str) -> Option<&'a ProjectRow> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return group.selected_checkout();
    }
    if group.name.to_lowercase().contains(&query) {
        return group.selected_checkout();
    }
    group.checkouts.iter().find(|row| {
        row.name.to_lowercase().contains(&query)
            || row.path.to_lowercase().contains(&query)
            || row
                .display_branch()
                .is_some_and(|branch| branch.to_lowercase().contains(&query))
    })
}

/// Mescla a identidade duravel na projecao do ledger sem promover um checkout
/// historico a alvo executavel. O match primario e o `project_id`; o fallback
/// por path so e usado quando ha exatamente uma entrada da registry para esse
/// path, o que permite recuperar rows antigas sem adivinhar por basename.
pub fn decorate_with_identity(
    mut rows: Vec<ProjectRow>,
    registry: &super::project_identity::IdentityRegistry,
) -> Vec<ProjectRow> {
    for row in &mut rows {
        let by_id = row
            .project_id
            .as_deref()
            .and_then(|project_id| registry.checkout(project_id));
        let by_path = if by_id.is_none() {
            let matches = registry
                .checkouts
                .iter()
                .filter(|checkout| key(&checkout.path) == key(&row.path))
                .collect::<Vec<_>>();
            (matches.len() == 1).then(|| matches[0])
        } else {
            None
        };
        let Some(checkout) = by_id.or(by_path) else {
            continue;
        };
        row.repository_id = checkout.repository_id.clone();
        row.checkout_id = checkout
            .checkout_id
            .clone()
            .or_else(|| Some(checkout.project_id.clone()));
        row.checkout_kind = Some(checkout.kind);
        row.checkout_ownership = Some(checkout.ownership);
        row.checkout_availability = Some(checkout.availability);
        row.current_branch = checkout.branch.clone();
        row.last_known_branch = checkout.last_known_branch.clone();
        row.association = if checkout.conflict.is_some() {
            AssociationState::Conflict
        } else if checkout.repository_id.is_some() {
            AssociationState::Known
        } else {
            AssociationState::Pending
        };
        row.archived = checkout.archived;
    }
    rows
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
            repository_id: None,
            checkout_id: Some(project.id.clone()),
            checkout_kind: None,
            checkout_ownership: None,
            checkout_availability: None,
            current_branch: None,
            last_known_branch: None,
            association: AssociationState::Pending,
            archived: false,
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
            repository_id: None,
            checkout_id: None,
            checkout_kind: None,
            checkout_ownership: None,
            checkout_availability: None,
            current_branch: None,
            last_known_branch: None,
            association: AssociationState::Pending,
            archived: false,
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
    unpeel_core::app_state::edit_at(path, |state| {
        let mut entries = state
            .get(LEDGER_KEY)
            .and_then(Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|entry| serde_json::from_value(entry.clone()).ok())
                    .collect::<Vec<LedgerProject>>()
            })
            .unwrap_or_default();
        let before = entries.len();
        entries.retain(|entry| key(&entry.path) != target);
        let mut changed = entries.len() != before;
        if changed {
            state.insert(LEDGER_KEY.to_owned(), encode(&entries)?);
        }

        // Forget is presentation metadata only, but the identity registry also
        // needs a durable suppression marker so a later reconcile cannot
        // recreate the row from a stale checkout registration. Unknown or
        // future identity schemas remain untouched and the ledger deletion is
        // still allowed to complete.
        if let Some(identity) = state.get(super::project_identity::IDENTITY_KEY).cloned()
            && let Ok(mut registry) =
                serde_json::from_value::<super::project_identity::IdentityRegistry>(identity)
            && registry.version <= super::project_identity::IDENTITY_SCHEMA_VERSION
        {
            let matching_checkout_ids = registry
                .checkouts
                .iter()
                .filter(|checkout| key(&checkout.path) == target)
                .map(|checkout| checkout.project_id.clone())
                .collect::<Vec<_>>();
            for checkout_id in matching_checkout_ids {
                if !registry
                    .suppressed_project_ids
                    .iter()
                    .any(|id| id == &checkout_id)
                {
                    registry.suppressed_project_ids.push(checkout_id.clone());
                    changed = true;
                }
                if let Some(checkout) = registry
                    .checkouts
                    .iter_mut()
                    .find(|checkout| checkout.project_id == checkout_id)
                {
                    if !checkout.archived {
                        checkout.archived = true;
                        changed = true;
                    }
                }
            }
            if changed {
                state.insert(
                    crate::project_identity::IDENTITY_KEY.to_owned(),
                    serde_json::to_value(registry).map_err(|error| error.to_string())?,
                );
            }
        }
        Ok(changed)
    })
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
    fn forgetting_a_checkout_suppresses_identity_without_touching_sessions() {
        with_state(|path| {
            unpeel_core::app_state::edit_at(path, |state| {
                state.insert(
                    LEDGER_KEY.to_owned(),
                    serde_json::json!([{
                        "path": "/tmp/removed",
                        "name": "removed",
                        "added_at_unix_ms": 1,
                        "last_seen_at_unix_ms": 2
                    }]),
                );
                state.insert(
                    crate::project_identity::IDENTITY_KEY.to_owned(),
                    serde_json::json!({
                        "version": 1,
                        "repositories": [],
                        "checkouts": [{
                            "projectID": "checkout-1",
                            "path": "/tmp/removed",
                            "kind": "linked",
                            "ownership": "external",
                            "availability": "missing"
                        }],
                        "suppressedProjectIds": []
                    }),
                );
                state.insert("sessions".to_owned(), serde_json::json!([{"id": "kept"}]));
                Ok(())
            })
            .unwrap();

            assert!(forget_at(path, "/tmp/removed").unwrap());
            let state = unpeel_core::app_state::load_for_edit_at(path).unwrap();
            assert!(state[LEDGER_KEY].as_array().is_some_and(Vec::is_empty));
            assert_eq!(state["sessions"][0]["id"], "kept");
            assert_eq!(
                state[crate::project_identity::IDENTITY_KEY]["suppressedProjectIds"][0],
                "checkout-1"
            );
            assert_eq!(
                state[crate::project_identity::IDENTITY_KEY]["checkouts"][0]["archived"],
                true
            );
        });
    }

    #[test]
    fn forgetting_does_not_rewrite_a_future_identity_schema() {
        with_state(|path| {
            unpeel_core::app_state::edit_at(path, |state| {
                state.insert(
                    LEDGER_KEY.to_owned(),
                    serde_json::json!([{
                        "path": "/tmp/future",
                        "name": "future",
                        "added_at_unix_ms": 1,
                        "last_seen_at_unix_ms": 2
                    }]),
                );
                state.insert(
                    crate::project_identity::IDENTITY_KEY.to_owned(),
                    serde_json::json!({
                        "version": 999,
                        "futureField": {"keep": true},
                        "repositories": [],
                        "checkouts": [{
                            "projectID": "future-1",
                            "path": "/tmp/future",
                            "kind": "linked",
                            "ownership": "external",
                            "availability": "missing"
                        }],
                        "suppressedProjectIds": []
                    }),
                );
                Ok(())
            })
            .unwrap();

            let before = unpeel_core::app_state::load_for_edit_at(path).unwrap()
                [crate::project_identity::IDENTITY_KEY]
                .clone();
            assert!(forget_at(path, "/tmp/future").unwrap());
            let after = unpeel_core::app_state::load_for_edit_at(path).unwrap();
            assert_eq!(after[crate::project_identity::IDENTITY_KEY], before);
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

    fn checkout(
        repository_id: Option<&str>,
        path: &str,
        name: &str,
        kind: CheckoutKind,
        availability: CheckoutAvailability,
        branch: Option<&str>,
    ) -> ProjectRow {
        ProjectRow {
            project_id: Some(format!("project-{name}")),
            path: path.to_owned(),
            name: name.to_owned(),
            added_at_unix_ms: 10,
            last_opened_at_unix_ms: 20,
            icon_path: None,
            repository_id: repository_id.map(str::to_owned),
            checkout_id: Some(format!("checkout-{name}")),
            checkout_kind: Some(kind),
            checkout_ownership: Some(CheckoutOwnership::External),
            checkout_availability: Some(availability),
            current_branch: branch.map(str::to_owned),
            last_known_branch: None,
            association: AssociationState::Known,
            archived: false,
        }
    }

    #[test]
    fn grouping_uses_persisted_repository_identity_and_keeps_children() {
        let groups = group_rows(&[
            checkout(
                Some("repo-1"),
                "/tmp/comet",
                "comet",
                CheckoutKind::Primary,
                CheckoutAvailability::Available,
                Some("main"),
            ),
            checkout(
                Some("repo-1"),
                "/tmp/comet-worktree",
                "feature/login",
                CheckoutKind::Linked,
                CheckoutAvailability::Available,
                Some("feature/login"),
            ),
            checkout(
                Some("repo-2"),
                "/tmp/other",
                "comet",
                CheckoutKind::Primary,
                CheckoutAvailability::Available,
                Some("main"),
            ),
        ]);

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].checkouts.len(), 2);
        assert_eq!(groups[0].name, "comet");
        assert_eq!(
            group_matches_query(&groups[0], "LOGIN").map(|row| row.path.as_str()),
            Some("/tmp/comet-worktree")
        );
    }

    #[test]
    fn legacy_rows_are_not_merged_by_name_or_path_shape() {
        let groups = group_rows(&[
            checkout(
                None,
                "/tmp/a/comet",
                "comet",
                CheckoutKind::Unresolved,
                CheckoutAvailability::ProbeFailed,
                None,
            ),
            checkout(
                None,
                "/tmp/b/comet",
                "comet",
                CheckoutKind::Unresolved,
                CheckoutAvailability::ProbeFailed,
                None,
            ),
        ]);
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn missing_checkout_stays_in_history_and_does_not_count_as_available() {
        let row = checkout(
            Some("repo-1"),
            "/tmp/removed",
            "feature/old",
            CheckoutKind::Linked,
            CheckoutAvailability::Missing,
            Some("feature/old"),
        );
        let groups = group_rows(&[row]);
        assert_eq!(groups[0].available_checkouts().count(), 0);
        assert_eq!(groups[0].historical_checkouts().count(), 1);
        assert_eq!(groups[0].selected_checkout().unwrap().name, "feature/old");
    }

    #[test]
    fn group_selection_prefers_an_available_child_when_primary_is_missing() {
        let groups = group_rows(&[
            checkout(
                Some("repo-1"),
                "/tmp/removed-main",
                "comet",
                CheckoutKind::Primary,
                CheckoutAvailability::Missing,
                Some("main"),
            ),
            checkout(
                Some("repo-1"),
                "/tmp/feature",
                "feature/sidebar",
                CheckoutKind::Linked,
                CheckoutAvailability::Available,
                Some("feature/sidebar"),
            ),
        ]);
        assert_eq!(
            groups[0].selected_checkout().map(|row| row.path.as_str()),
            Some("/tmp/feature")
        );
    }

    #[test]
    fn a_repository_without_a_primary_uses_the_explicit_container_name() {
        let groups = group_rows(&[checkout(
            Some("repo-1"),
            "/tmp/feature",
            "feature/sidebar",
            CheckoutKind::Linked,
            CheckoutAvailability::Available,
            Some("feature/sidebar"),
        )]);
        assert_eq!(groups[0].name, UNREGISTERED_PRIMARY_NAME);
    }

    #[test]
    fn decorating_rows_preserves_legacy_fields_and_adds_identity_metadata() {
        let row = checkout(
            None,
            "/tmp/feature",
            "feature",
            CheckoutKind::Linked,
            CheckoutAvailability::Available,
            None,
        );
        let registry = super::super::project_identity::IdentityRegistry {
            repositories: vec![super::super::project_identity::RepositoryIdentity {
                id: "repo-1".to_owned(),
                common_dir: Some("/tmp/repo/.git".to_owned()),
                common_dir_fingerprint: None,
                common_dir_stable_fingerprint: None,
                name: Some("repo".to_owned()),
                primary_path: None,
                primary_project_id: None,
                project_ids: vec!["project-feature".to_owned()],
                last_seen_unix_ms: Some(30),
                extra: Default::default(),
            }],
            checkouts: vec![super::super::project_identity::CheckoutIdentity {
                project_id: "project-feature".to_owned(),
                checkout_id: Some("checkout-feature".to_owned()),
                repository_id: Some("repo-1".to_owned()),
                path: "/tmp/feature".to_owned(),
                canonical_path: None,
                main_repo: Some("/tmp/repo".to_owned()),
                kind: CheckoutKind::Linked,
                ownership: CheckoutOwnership::External,
                availability: CheckoutAvailability::Available,
                branch: Some("feature/login".to_owned()),
                last_known_branch: Some("feature/login".to_owned()),
                detached_oid: None,
                detached: false,
                remotes: Vec::new(),
                observed_common_dir: Some("/tmp/repo/.git".to_owned()),
                conflict: None,
                last_observed_unix_ms: Some(30),
                archived: false,
                extra: Default::default(),
            }],
            ..Default::default()
        };
        let rows = decorate_with_identity(vec![row], &registry);
        assert_eq!(rows[0].repository_id.as_deref(), Some("repo-1"));
        assert_eq!(rows[0].checkout_id.as_deref(), Some("checkout-feature"));
        assert_eq!(rows[0].display_branch(), Some("feature/login"));
        assert_eq!(rows[0].association, AssociationState::Known);
    }
}

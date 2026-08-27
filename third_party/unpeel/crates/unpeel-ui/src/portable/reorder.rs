//! Shared, renderer-independent helpers for reordering portable collections.
//!
//! Native clients send a complete ordered list of stable item IDs. Terminal
//! Apps can use [`ReorderState`] to provide the equivalent pick-up, move, and
//! drop interaction without coupling App state to a particular input crate.

use std::collections::{HashMap, HashSet};
use std::fmt;

use serde::{Deserialize, Serialize};

/// Stable identity for one entry in an ordered collection.
///
/// Item IDs are scoped by the collection node and action that emit them. They
/// therefore need to be unique within that collection, not across a whole
/// view tree.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ItemId(pub String);

impl ItemId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }

    /// Whether this ID is usable in reorder metadata and actions.
    pub fn is_valid(&self) -> bool {
        !self.0.trim().is_empty()
    }
}

impl fmt::Display for ItemId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl From<&str> for ItemId {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl From<String> for ItemId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&String> for ItemId {
    fn from(value: &String) -> Self {
        Self(value.clone())
    }
}

impl From<&ItemId> for ItemId {
    fn from(value: &ItemId) -> Self {
        value.clone()
    }
}

impl From<ItemId> for String {
    fn from(value: ItemId) -> Self {
        value.0
    }
}

impl AsRef<str> for ItemId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

/// Renderer-independent commands for a keyboard-accessible reorder gesture.
///
/// A caller maps vertical or horizontal keys to `Previous` and `Next`. This
/// keeps the helper equally useful for lists, tabs, table rows, and columns.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ReorderCommand {
    Previous,
    Next,
    /// Move selection, or the picked-up item, directly to a hit-tested index.
    /// This supports terminal mouse dragging and native/web drop targets.
    MoveTo(usize),
    ToggleGrab,
    Cancel,
}

/// Observable result of applying one [`ReorderCommand`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReorderUpdate {
    Unchanged,
    SelectionChanged {
        selected: Option<usize>,
    },
    Grabbed {
        index: usize,
    },
    Moved {
        from: usize,
        to: usize,
    },
    /// The complete logical order to send to the App's reorder action.
    Committed {
        order: Vec<ItemId>,
    },
    Cancelled {
        selected: Option<usize>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Grabbed {
    id: ItemId,
    original_order: Vec<ItemId>,
}

/// Terminal selection and pick-up/drop state for an ordered collection.
///
/// The helper mutates the supplied `Vec` only to provide an immediate terminal
/// preview. `Cancel` restores the exact order from before the item was picked
/// up, while `ToggleGrab` commits and returns the complete ID order.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReorderState {
    selected: Option<usize>,
    grabbed: Option<Grabbed>,
}

impl ReorderState {
    pub const fn new() -> Self {
        Self {
            selected: None,
            grabbed: None,
        }
    }

    #[must_use]
    pub fn with_selected<T>(mut self, selected: T) -> Self
    where
        T: Into<Option<usize>>,
    {
        self.selected = selected.into();
        self
    }

    pub const fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub const fn is_grabbed(&self) -> bool {
        self.grabbed.is_some()
    }

    /// Current index of the picked-up item, if it remains in the collection.
    pub fn grabbed_index<T, F>(&self, items: &[T], mut item_id: F) -> Option<usize>
    where
        F: for<'a> FnMut(&'a T) -> Option<&'a str>,
    {
        let grabbed = self.grabbed.as_ref()?;
        items
            .iter()
            .position(|item| item_id(item) == Some(grabbed.id.as_str()))
    }

    /// Clear selection and any in-progress reorder gesture.
    pub fn reset(&mut self) {
        self.selected = None;
        self.grabbed = None;
    }

    /// Apply one logical reorder command to a collection.
    ///
    /// `item_id` must return every item's stable ID. Missing, blank, or
    /// duplicate IDs are rejected before the collection is changed.
    pub fn handle<T, F>(
        &mut self,
        command: ReorderCommand,
        items: &mut Vec<T>,
        mut item_id: F,
    ) -> Result<ReorderUpdate, ReorderError>
    where
        F: for<'a> FnMut(&'a T) -> Option<&'a str>,
    {
        let ids = collect_ids(items, &mut item_id)?;
        if items.is_empty() {
            let changed = self.selected.is_some() || self.grabbed.is_some();
            self.reset();
            return Ok(if changed {
                ReorderUpdate::SelectionChanged { selected: None }
            } else {
                ReorderUpdate::Unchanged
            });
        }

        self.selected = self.selected.map(|selected| selected.min(items.len() - 1));
        if let Some(grabbed) = &self.grabbed {
            if let Some(index) = ids.iter().position(|id| id == &grabbed.id) {
                self.selected = Some(index);
            } else {
                // The App replaced the collection while a gesture was active.
                // Drop the stale gesture rather than moving a different item.
                self.grabbed = None;
            }
        }

        match command {
            ReorderCommand::Previous => self.move_selection(items, false),
            ReorderCommand::Next => self.move_selection(items, true),
            ReorderCommand::MoveTo(index) => self.move_to(items, index),
            ReorderCommand::ToggleGrab => {
                let selected = self.selected.unwrap_or(0);
                self.selected = Some(selected);
                if self.grabbed.is_some() {
                    let order = collect_ids(items, &mut item_id)?;
                    self.grabbed = None;
                    Ok(ReorderUpdate::Committed { order })
                } else {
                    self.grabbed = Some(Grabbed {
                        id: ids[selected].clone(),
                        original_order: ids,
                    });
                    Ok(ReorderUpdate::Grabbed { index: selected })
                }
            }
            ReorderCommand::Cancel => {
                let Some(grabbed) = self.grabbed.take() else {
                    return Ok(ReorderUpdate::Unchanged);
                };
                let applied = apply_order(items, &grabbed.original_order, &mut item_id)?;
                self.selected = applied.remap_index(self.selected);
                Ok(ReorderUpdate::Cancelled {
                    selected: self.selected,
                })
            }
        }
    }

    fn move_selection<T>(
        &mut self,
        items: &mut [T],
        forward: bool,
    ) -> Result<ReorderUpdate, ReorderError> {
        let Some(selected) = self.selected else {
            let selected = if forward { 0 } else { items.len() - 1 };
            self.selected = Some(selected);
            return Ok(ReorderUpdate::SelectionChanged {
                selected: self.selected,
            });
        };
        let target = if forward {
            selected.saturating_add(1).min(items.len() - 1)
        } else {
            selected.saturating_sub(1)
        };
        self.move_to(items, target)
    }

    fn move_to<T>(
        &mut self,
        items: &mut [T],
        target: usize,
    ) -> Result<ReorderUpdate, ReorderError> {
        let target = target.min(items.len() - 1);
        let Some(selected) = self.selected else {
            self.selected = Some(target);
            return Ok(ReorderUpdate::SelectionChanged {
                selected: self.selected,
            });
        };
        if target == selected {
            return Ok(ReorderUpdate::Unchanged);
        }

        self.selected = Some(target);
        if self.grabbed.is_some() {
            if selected < target {
                items[selected..=target].rotate_left(1);
            } else {
                items[target..=selected].rotate_right(1);
            }
            Ok(ReorderUpdate::Moved {
                from: selected,
                to: target,
            })
        } else {
            Ok(ReorderUpdate::SelectionChanged {
                selected: self.selected,
            })
        }
    }
}

/// Index mapping produced by an accepted exact-permutation reorder.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppliedOrder {
    old_to_new: Vec<usize>,
    changed: bool,
}

impl AppliedOrder {
    pub fn len(&self) -> usize {
        self.old_to_new.len()
    }

    pub fn is_empty(&self) -> bool {
        self.old_to_new.is_empty()
    }

    pub const fn changed(&self) -> bool {
        self.changed
    }

    /// Translate a positional selection from the old order to the new one.
    pub fn remap_index(&self, selected: Option<usize>) -> Option<usize> {
        selected.and_then(|index| self.old_to_new.get(index).copied())
    }
}

/// Apply a complete requested order after proving it is an exact permutation.
///
/// Validation happens before mutation, so an error leaves `items` untouched.
/// The returned mapping makes positional List/Tab/Table selection preserve the
/// same stable item identity after the move.
pub fn apply_order<T, F>(
    items: &mut Vec<T>,
    requested: &[ItemId],
    mut item_id: F,
) -> Result<AppliedOrder, ReorderError>
where
    F: for<'a> FnMut(&'a T) -> Option<&'a str>,
{
    let current = collect_ids(items, &mut item_id)?;
    validate_requested(&current, requested)?;

    let old_by_id: HashMap<&ItemId, usize> = current
        .iter()
        .enumerate()
        .map(|(index, id)| (id, index))
        .collect();
    let mut old_to_new = vec![0; current.len()];
    for (new_index, id) in requested.iter().enumerate() {
        old_to_new[old_by_id[id]] = new_index;
    }
    let changed = old_to_new.iter().enumerate().any(|(old, new)| old != *new);

    let mut slots: Vec<Option<T>> = items.drain(..).map(Some).collect();
    let reordered = requested
        .iter()
        .map(|id| {
            slots[old_by_id[id]]
                .take()
                .expect("validated permutation uses each item exactly once")
        })
        .collect();
    *items = reordered;

    Ok(AppliedOrder {
        old_to_new,
        changed,
    })
}

fn collect_ids<T, F>(items: &[T], item_id: &mut F) -> Result<Vec<ItemId>, ReorderError>
where
    F: for<'a> FnMut(&'a T) -> Option<&'a str>,
{
    let mut ids = Vec::with_capacity(items.len());
    let mut seen = HashSet::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let id = item_id(item).ok_or(ReorderError::MissingItemId { index })?;
        if id.trim().is_empty() {
            return Err(ReorderError::EmptyItemId { index });
        }
        let id = ItemId::from(id);
        if !seen.insert(id.clone()) {
            return Err(ReorderError::DuplicateItemId { id });
        }
        ids.push(id);
    }
    Ok(ids)
}

fn validate_requested(current: &[ItemId], requested: &[ItemId]) -> Result<(), ReorderError> {
    if current.len() != requested.len() {
        return Err(ReorderError::LengthMismatch {
            expected: current.len(),
            received: requested.len(),
        });
    }

    let available: HashSet<&ItemId> = current.iter().collect();
    let mut seen = HashSet::with_capacity(requested.len());
    for (index, id) in requested.iter().enumerate() {
        if !id.is_valid() {
            return Err(ReorderError::EmptyRequestedId { index });
        }
        if !seen.insert(id) {
            return Err(ReorderError::DuplicateRequestedId { id: id.clone() });
        }
        if !available.contains(id) {
            return Err(ReorderError::UnknownItemId { id: id.clone() });
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReorderError {
    MissingItemId { index: usize },
    EmptyItemId { index: usize },
    DuplicateItemId { id: ItemId },
    LengthMismatch { expected: usize, received: usize },
    EmptyRequestedId { index: usize },
    DuplicateRequestedId { id: ItemId },
    UnknownItemId { id: ItemId },
}

impl fmt::Display for ReorderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingItemId { index } => write!(formatter, "item {index} has no stable id"),
            Self::EmptyItemId { index } => write!(formatter, "item {index} has an empty id"),
            Self::DuplicateItemId { id } => write!(formatter, "duplicate item id `{id}`"),
            Self::LengthMismatch { expected, received } => write!(
                formatter,
                "reorder contains {received} ids for {expected} items"
            ),
            Self::EmptyRequestedId { index } => {
                write!(formatter, "requested reorder id {index} is empty")
            }
            Self::DuplicateRequestedId { id } => {
                write!(formatter, "requested reorder repeats id `{id}`")
            }
            Self::UnknownItemId { id } => {
                write!(formatter, "requested reorder contains unknown id `{id}`")
            }
        }
    }
}

impl std::error::Error for ReorderError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::model::{Node, NodeId};
    use crate::portable::widgets::Paragraph;

    #[derive(Clone, Debug, Eq, PartialEq)]
    struct Item {
        id: Option<ItemId>,
        label: String,
    }

    fn item(id: &str) -> Item {
        Item {
            id: Some(id.into()),
            label: id.to_owned(),
        }
    }

    fn id(item: &Item) -> Option<&str> {
        item.id.as_ref().map(ItemId::as_str)
    }

    fn labels(items: &[Item]) -> Vec<&str> {
        items.iter().map(|item| item.label.as_str()).collect()
    }

    #[test]
    fn item_id_is_an_owned_transparent_wire_value() {
        let id = ItemId::new("task-a");
        assert_eq!(id.as_str(), "task-a");
        assert_eq!(id.to_string(), "task-a");
        assert_eq!(serde_json::to_string(&id).unwrap(), r#""task-a""#);
        assert_eq!(serde_json::from_str::<ItemId>(r#""task-a""#).unwrap(), id);
        assert!(id.is_valid());
        assert!(!ItemId::new(" \t").is_valid());
    }

    #[test]
    fn apply_order_accepts_an_exact_permutation_and_remaps_selection() {
        let mut items = vec![item("a"), item("b"), item("c")];
        let applied = apply_order(&mut items, &["c".into(), "a".into(), "b".into()], id).unwrap();

        assert_eq!(labels(&items), ["c", "a", "b"]);
        assert!(applied.changed());
        assert_eq!(applied.remap_index(Some(0)), Some(1));
        assert_eq!(applied.remap_index(Some(1)), Some(2));
        assert_eq!(applied.remap_index(Some(2)), Some(0));
        assert_eq!(applied.remap_index(Some(9)), None);
        assert_eq!(applied.remap_index(None), None);
    }

    #[test]
    fn apply_order_reports_an_unchanged_order() {
        let mut items = vec![item("a"), item("b")];
        let applied = apply_order(&mut items, &["a".into(), "b".into()], id).unwrap();
        assert!(!applied.changed());
        assert_eq!(applied.len(), 2);
    }

    #[test]
    fn apply_order_accepts_layout_node_ids_without_conversion_storage() {
        let mut children = vec![
            Node::from(Paragraph::new("Alpha")).id("alpha"),
            Node::from(Paragraph::new("Beta")).id("beta"),
        ];
        apply_order(&mut children, &["beta".into(), "alpha".into()], |node| {
            node.id.as_ref().map(NodeId::as_str)
        })
        .unwrap();

        assert_eq!(children[0].id.as_ref().map(NodeId::as_str), Some("beta"));
        assert_eq!(children[1].id.as_ref().map(NodeId::as_str), Some("alpha"));
    }

    #[test]
    fn invalid_permutations_do_not_mutate_items() {
        let cases = [
            vec!["a".into()],
            vec!["a".into(), "a".into(), "c".into()],
            vec!["a".into(), "b".into(), "x".into()],
            vec!["a".into(), "b".into(), " ".into()],
        ];
        for requested in cases {
            let mut items = vec![item("a"), item("b"), item("c")];
            assert!(apply_order(&mut items, &requested, id).is_err());
            assert_eq!(labels(&items), ["a", "b", "c"]);
        }
    }

    #[test]
    fn invalid_source_ids_are_rejected_without_mutation() {
        let mut missing = vec![
            item("a"),
            Item {
                id: None,
                label: "b".to_owned(),
            },
        ];
        assert_eq!(
            apply_order(&mut missing, &["b".into(), "a".into()], id),
            Err(ReorderError::MissingItemId { index: 1 })
        );
        assert_eq!(labels(&missing), ["a", "b"]);

        let mut duplicate = vec![item("a"), item("a")];
        assert!(matches!(
            apply_order(&mut duplicate, &["a".into(), "b".into()], id),
            Err(ReorderError::DuplicateItemId { .. })
        ));
        assert_eq!(labels(&duplicate), ["a", "a"]);
    }

    #[test]
    fn terminal_state_selects_grabs_moves_and_commits() {
        let mut items = vec![item("a"), item("b"), item("c")];
        let mut state = ReorderState::new();

        assert_eq!(
            state.handle(ReorderCommand::Next, &mut items, id).unwrap(),
            ReorderUpdate::SelectionChanged { selected: Some(0) }
        );
        assert_eq!(
            state.handle(ReorderCommand::Next, &mut items, id).unwrap(),
            ReorderUpdate::SelectionChanged { selected: Some(1) }
        );
        assert_eq!(
            state
                .handle(ReorderCommand::ToggleGrab, &mut items, id)
                .unwrap(),
            ReorderUpdate::Grabbed { index: 1 }
        );
        assert_eq!(
            state.handle(ReorderCommand::Next, &mut items, id).unwrap(),
            ReorderUpdate::Moved { from: 1, to: 2 }
        );
        assert_eq!(labels(&items), ["a", "c", "b"]);
        assert_eq!(state.selected(), Some(2));
        assert_eq!(state.grabbed_index(&items, id), Some(2));
        assert_eq!(
            state
                .handle(ReorderCommand::ToggleGrab, &mut items, id)
                .unwrap(),
            ReorderUpdate::Committed {
                order: vec!["a".into(), "c".into(), "b".into()]
            }
        );
        assert!(!state.is_grabbed());
    }

    #[test]
    fn cancel_restores_the_original_order_and_selected_identity() {
        let mut items = vec![item("a"), item("b"), item("c")];
        let mut state = ReorderState::new().with_selected(1);
        state
            .handle(ReorderCommand::ToggleGrab, &mut items, id)
            .unwrap();
        state
            .handle(ReorderCommand::Previous, &mut items, id)
            .unwrap();
        assert_eq!(labels(&items), ["b", "a", "c"]);

        assert_eq!(
            state
                .handle(ReorderCommand::Cancel, &mut items, id)
                .unwrap(),
            ReorderUpdate::Cancelled { selected: Some(1) }
        );
        assert_eq!(labels(&items), ["a", "b", "c"]);
        assert_eq!(state.selected(), Some(1));
    }

    #[test]
    fn movement_is_bounded_and_empty_collections_are_safe() {
        let mut items = vec![item("a")];
        let mut state = ReorderState::new();
        assert_eq!(
            state
                .handle(ReorderCommand::Previous, &mut items, id)
                .unwrap(),
            ReorderUpdate::SelectionChanged { selected: Some(0) }
        );
        assert_eq!(state.selected(), Some(0));

        let mut empty = Vec::<Item>::new();
        assert_eq!(
            state.handle(ReorderCommand::Next, &mut empty, id).unwrap(),
            ReorderUpdate::SelectionChanged { selected: None }
        );
        assert_eq!(state.selected(), None);
    }

    #[test]
    fn move_to_supports_hit_tested_drag_targets() {
        let mut items = vec![item("a"), item("b"), item("c"), item("d")];
        let mut state = ReorderState::new().with_selected(0);
        state
            .handle(ReorderCommand::ToggleGrab, &mut items, id)
            .unwrap();

        assert_eq!(
            state.handle(ReorderCommand::MoveTo(3), &mut items, id),
            Ok(ReorderUpdate::Moved { from: 0, to: 3 })
        );
        assert_eq!(labels(&items), ["b", "c", "d", "a"]);

        assert_eq!(
            state.handle(ReorderCommand::MoveTo(1), &mut items, id),
            Ok(ReorderUpdate::Moved { from: 3, to: 1 })
        );
        assert_eq!(labels(&items), ["b", "a", "c", "d"]);
    }
}

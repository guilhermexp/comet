use serde::{Deserialize, Serialize};

use super::model::{ActionId, Alignment, Block, Constraint, Flex, Line, NodeId, Span, Style, Text};
use super::reorder::ItemId;

/// Describes how paragraph text wraps across lines.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Wrap {
    pub trim: bool,
}

/// Owned counterpart of `ratatui::widgets::Paragraph`.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Paragraph {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block: Option<Block>,
    #[serde(default, skip_serializing_if = "Style::is_empty")]
    pub style: Style,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wrap: Option<Wrap>,
    pub text: Text,
    /// Ratatui order: vertical rows, then horizontal columns.
    #[serde(default, skip_serializing_if = "is_zero_scroll")]
    pub scroll: (u16, u16),
    #[serde(default)]
    pub alignment: Alignment,
    #[serde(skip)]
    pub(crate) node_id: Option<NodeId>,
}

fn is_zero_scroll(scroll: &(u16, u16)) -> bool {
    *scroll == (0, 0)
}

impl Paragraph {
    pub fn new<T>(text: T) -> Self
    where
        T: Into<Text>,
    {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn id(mut self, id: impl Into<NodeId>) -> Self {
        self.node_id = Some(id.into());
        self
    }

    #[must_use]
    pub fn block(mut self, block: Block) -> Self {
        self.block = Some(block);
        self
    }

    #[must_use]
    pub fn style(mut self, style: impl Into<Style>) -> Self {
        self.style = style.into();
        self
    }

    #[must_use]
    pub fn wrap(mut self, wrap: Wrap) -> Self {
        self.wrap = Some(wrap);
        self
    }

    #[must_use]
    pub fn scroll(mut self, scroll: (u16, u16)) -> Self {
        self.scroll = scroll;
        self
    }

    #[must_use]
    pub fn alignment(mut self, alignment: Alignment) -> Self {
        self.alignment = alignment;
        self
    }

    #[must_use]
    pub fn left_aligned(self) -> Self {
        self.alignment(Alignment::Left)
    }

    #[must_use]
    pub fn centered(self) -> Self {
        self.alignment(Alignment::Center)
    }

    #[must_use]
    pub fn right_aligned(self) -> Self {
        self.alignment(Alignment::Right)
    }
}

/// Direction in which list items are rendered.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ListDirection {
    #[default]
    TopToBottom,
    BottomToTop,
}

/// When Ratatui reserves space for a list or table highlight symbol.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HighlightSpacing {
    Always,
    #[default]
    WhenSelected,
    Never,
}

/// Owned counterpart of `ratatui::widgets::ListItem`, with optional stable
/// identity for semantic reordering.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<ItemId>,
    pub content: Text,
    #[serde(default, skip_serializing_if = "Style::is_empty")]
    pub style: Style,
}

impl ListItem {
    pub fn new<T>(content: T) -> Self
    where
        T: Into<Text>,
    {
        Self {
            content: content.into(),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn id(mut self, id: impl Into<ItemId>) -> Self {
        self.id = Some(id.into());
        self
    }

    #[must_use]
    pub fn style(mut self, style: impl Into<Style>) -> Self {
        self.style = style.into();
        self
    }
}

impl<T> From<T> for ListItem
where
    T: Into<Text>,
{
    fn from(content: T) -> Self {
        Self::new(content)
    }
}

/// Owned counterpart of `ratatui::widgets::List`, including its state and a
/// semantic reorder action for native, web, and terminal renderers.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct List {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block: Option<Block>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<ListItem>,
    #[serde(default, skip_serializing_if = "Style::is_empty")]
    pub style: Style,
    #[serde(default)]
    pub direction: ListDirection,
    #[serde(default, skip_serializing_if = "Style::is_empty")]
    pub highlight_style: Style,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub highlight_symbol: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub repeat_highlight_symbol: bool,
    #[serde(default)]
    pub highlight_spacing: HighlightSpacing,
    #[serde(default, skip_serializing_if = "is_zero_usize")]
    pub scroll_padding: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected: Option<usize>,
    #[serde(default, skip_serializing_if = "is_zero_usize")]
    pub offset: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_reorder: Option<ActionId>,
    #[serde(skip)]
    pub(crate) node_id: Option<NodeId>,
}

impl List {
    pub fn new<I, T>(items: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<ListItem>,
    {
        Self {
            items: items.into_iter().map(Into::into).collect(),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn id(mut self, id: impl Into<NodeId>) -> Self {
        self.node_id = Some(id.into());
        self
    }

    #[must_use]
    pub fn reorderable(mut self, action: impl Into<ActionId>) -> Self {
        self.on_reorder = Some(action.into());
        self
    }

    #[must_use]
    pub fn items<I, T>(mut self, items: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<ListItem>,
    {
        self.items = items.into_iter().map(Into::into).collect();
        self
    }

    #[must_use]
    pub fn block(mut self, block: Block) -> Self {
        self.block = Some(block);
        self
    }

    #[must_use]
    pub fn style(mut self, style: impl Into<Style>) -> Self {
        self.style = style.into();
        self
    }

    #[must_use]
    pub fn highlight_symbol(mut self, highlight_symbol: impl Into<String>) -> Self {
        self.highlight_symbol = Some(highlight_symbol.into());
        self
    }

    #[must_use]
    pub fn highlight_style(mut self, style: impl Into<Style>) -> Self {
        self.highlight_style = style.into();
        self
    }

    #[must_use]
    pub fn repeat_highlight_symbol(mut self, repeat: bool) -> Self {
        self.repeat_highlight_symbol = repeat;
        self
    }

    #[must_use]
    pub fn highlight_spacing(mut self, value: HighlightSpacing) -> Self {
        self.highlight_spacing = value;
        self
    }

    #[must_use]
    pub fn direction(mut self, direction: ListDirection) -> Self {
        self.direction = direction;
        self
    }

    #[must_use]
    pub fn scroll_padding(mut self, padding: usize) -> Self {
        self.scroll_padding = padding;
        self
    }

    #[must_use]
    pub fn select<T>(mut self, selected: T) -> Self
    where
        T: Into<Option<usize>>,
    {
        self.selected = selected.into();
        self
    }

    #[must_use]
    pub fn offset(mut self, offset: usize) -> Self {
        self.offset = offset;
        self
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

impl<T> FromIterator<T> for List
where
    T: Into<ListItem>,
{
    fn from_iter<I: IntoIterator<Item = T>>(items: I) -> Self {
        Self::new(items)
    }
}

/// Owned counterpart of `ratatui::widgets::Cell`.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cell {
    pub content: Text,
    #[serde(default, skip_serializing_if = "Style::is_empty")]
    pub style: Style,
}

impl Cell {
    pub fn new<T>(content: T) -> Self
    where
        T: Into<Text>,
    {
        Self {
            content: content.into(),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn content<T>(mut self, content: T) -> Self
    where
        T: Into<Text>,
    {
        self.content = content.into();
        self
    }

    #[must_use]
    pub fn style(mut self, style: impl Into<Style>) -> Self {
        self.style = style.into();
        self
    }
}

impl<T> From<T> for Cell
where
    T: Into<Text>,
{
    fn from(content: T) -> Self {
        Self::new(content)
    }
}

/// Owned counterpart of `ratatui::widgets::Row`, with optional stable body
/// row identity for semantic reordering.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<ItemId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cells: Vec<Cell>,
    #[serde(default = "one_u16", skip_serializing_if = "is_one_u16")]
    pub height: u16,
    #[serde(default, skip_serializing_if = "is_zero_u16")]
    pub top_margin: u16,
    #[serde(default, skip_serializing_if = "is_zero_u16")]
    pub bottom_margin: u16,
    #[serde(default, skip_serializing_if = "Style::is_empty")]
    pub style: Style,
}

impl Row {
    pub fn new<I, T>(cells: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<Cell>,
    {
        Self {
            cells: cells.into_iter().map(Into::into).collect(),
            height: 1,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn id(mut self, id: impl Into<ItemId>) -> Self {
        self.id = Some(id.into());
        self
    }

    #[must_use]
    pub fn cells<I, T>(mut self, cells: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<Cell>,
    {
        self.cells = cells.into_iter().map(Into::into).collect();
        self
    }

    #[must_use]
    pub fn height(mut self, height: u16) -> Self {
        self.height = height;
        self
    }

    #[must_use]
    pub fn top_margin(mut self, margin: u16) -> Self {
        self.top_margin = margin;
        self
    }

    #[must_use]
    pub fn bottom_margin(mut self, margin: u16) -> Self {
        self.bottom_margin = margin;
        self
    }

    #[must_use]
    pub fn style(mut self, style: impl Into<Style>) -> Self {
        self.style = style.into();
        self
    }
}

impl<T> FromIterator<T> for Row
where
    T: Into<Cell>,
{
    fn from_iter<I: IntoIterator<Item = T>>(cells: I) -> Self {
        Self::new(cells)
    }
}

/// Owned counterpart of `ratatui::widgets::Table`, including its state and
/// separate semantic reorder actions for body rows and columns.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Table {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rows: Vec<Row>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<Row>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub footer: Option<Row>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub widths: Vec<Constraint>,
    #[serde(default = "one_u16", skip_serializing_if = "is_one_u16")]
    pub column_spacing: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block: Option<Block>,
    #[serde(default, skip_serializing_if = "Style::is_empty")]
    pub style: Style,
    #[serde(default, skip_serializing_if = "Style::is_empty")]
    pub row_highlight_style: Style,
    #[serde(default, skip_serializing_if = "Style::is_empty")]
    pub column_highlight_style: Style,
    #[serde(default, skip_serializing_if = "Style::is_empty")]
    pub cell_highlight_style: Style,
    #[serde(default, skip_serializing_if = "is_empty_text")]
    pub highlight_symbol: Text,
    #[serde(default)]
    pub highlight_spacing: HighlightSpacing,
    #[serde(default)]
    pub flex: Flex,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_column: Option<usize>,
    #[serde(default, skip_serializing_if = "is_zero_usize")]
    pub offset: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub column_ids: Vec<ItemId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_reorder_rows: Option<ActionId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_reorder_columns: Option<ActionId>,
    /// Builder-only arity error retained until `Node::validate` so fluent
    /// construction never panics on generated or dynamic input.
    #[serde(skip)]
    pub(crate) row_id_count_error: Option<(usize, usize)>,
    #[serde(skip)]
    pub(crate) node_id: Option<NodeId>,
}

impl Default for Table {
    fn default() -> Self {
        Self {
            rows: Vec::new(),
            header: None,
            footer: None,
            widths: Vec::new(),
            column_spacing: 1,
            block: None,
            style: Style::default(),
            row_highlight_style: Style::default(),
            column_highlight_style: Style::default(),
            cell_highlight_style: Style::default(),
            highlight_symbol: Text::default(),
            highlight_spacing: HighlightSpacing::default(),
            flex: Flex::Start,
            selected: None,
            selected_column: None,
            offset: 0,
            column_ids: Vec::new(),
            on_reorder_rows: None,
            on_reorder_columns: None,
            row_id_count_error: None,
            node_id: None,
        }
    }
}

impl Table {
    pub fn new<R, RI, C, CI>(rows: R, widths: C) -> Self
    where
        R: IntoIterator<Item = RI>,
        RI: Into<Row>,
        C: IntoIterator<Item = CI>,
        CI: Into<Constraint>,
    {
        Self {
            rows: rows.into_iter().map(Into::into).collect(),
            widths: widths.into_iter().map(Into::into).collect(),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn id(mut self, id: impl Into<NodeId>) -> Self {
        self.node_id = Some(id.into());
        self
    }

    #[must_use]
    pub fn rows<I, T>(mut self, rows: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<Row>,
    {
        self.rows = rows.into_iter().map(Into::into).collect();
        self.row_id_count_error = None;
        self
    }

    #[must_use]
    pub fn row_ids<I, T>(mut self, ids: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<ItemId>,
    {
        let ids = ids.into_iter().map(Into::into).collect::<Vec<_>>();
        self.row_id_count_error =
            (ids.len() != self.rows.len()).then_some((ids.len(), self.rows.len()));
        for (row, id) in self.rows.iter_mut().zip(ids) {
            row.id = Some(id);
        }
        self
    }

    #[must_use]
    pub fn column_ids<I, T>(mut self, ids: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<ItemId>,
    {
        self.column_ids = ids.into_iter().map(Into::into).collect();
        self
    }

    #[must_use]
    pub fn reorderable_rows(mut self, action: impl Into<ActionId>) -> Self {
        self.on_reorder_rows = Some(action.into());
        self
    }

    #[must_use]
    pub fn reorderable_columns(mut self, action: impl Into<ActionId>) -> Self {
        self.on_reorder_columns = Some(action.into());
        self
    }

    #[must_use]
    pub fn header(mut self, header: Row) -> Self {
        self.header = Some(header);
        self
    }

    #[must_use]
    pub fn footer(mut self, footer: Row) -> Self {
        self.footer = Some(footer);
        self
    }

    #[must_use]
    pub fn widths<I, T>(mut self, widths: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<Constraint>,
    {
        self.widths = widths.into_iter().map(Into::into).collect();
        self
    }

    #[must_use]
    pub fn column_spacing(mut self, spacing: u16) -> Self {
        self.column_spacing = spacing;
        self
    }

    #[must_use]
    pub fn block(mut self, block: Block) -> Self {
        self.block = Some(block);
        self
    }

    #[must_use]
    pub fn style(mut self, style: impl Into<Style>) -> Self {
        self.style = style.into();
        self
    }

    #[must_use]
    pub fn row_highlight_style(mut self, style: impl Into<Style>) -> Self {
        self.row_highlight_style = style.into();
        self
    }

    #[must_use]
    pub fn column_highlight_style(mut self, style: impl Into<Style>) -> Self {
        self.column_highlight_style = style.into();
        self
    }

    #[must_use]
    pub fn cell_highlight_style(mut self, style: impl Into<Style>) -> Self {
        self.cell_highlight_style = style.into();
        self
    }

    #[must_use]
    #[deprecated(note = "use `Table::row_highlight_style` instead")]
    pub fn highlight_style(self, style: impl Into<Style>) -> Self {
        self.row_highlight_style(style)
    }

    #[must_use]
    pub fn highlight_symbol(mut self, highlight_symbol: impl Into<Text>) -> Self {
        self.highlight_symbol = highlight_symbol.into();
        self
    }

    #[must_use]
    pub fn highlight_spacing(mut self, value: HighlightSpacing) -> Self {
        self.highlight_spacing = value;
        self
    }

    #[must_use]
    pub fn flex(mut self, flex: Flex) -> Self {
        self.flex = flex;
        self
    }

    #[must_use]
    pub fn select<T>(mut self, selected: T) -> Self
    where
        T: Into<Option<usize>>,
    {
        self.selected = selected.into();
        self
    }

    #[must_use]
    pub fn select_column<T>(mut self, selected: T) -> Self
    where
        T: Into<Option<usize>>,
    {
        self.selected_column = selected.into();
        self
    }

    #[must_use]
    pub fn select_cell<T>(mut self, selected: T) -> Self
    where
        T: Into<Option<(usize, usize)>>,
    {
        match selected.into() {
            Some((row, column)) => {
                self.selected = Some(row);
                self.selected_column = Some(column);
            }
            None => {
                self.selected = None;
                self.selected_column = None;
            }
        }
        self
    }

    #[must_use]
    pub fn offset(mut self, offset: usize) -> Self {
        self.offset = offset;
        self
    }
}

impl<T> FromIterator<T> for Table
where
    T: Into<Row>,
{
    fn from_iter<I: IntoIterator<Item = T>>(rows: I) -> Self {
        Self {
            rows: rows.into_iter().map(Into::into).collect(),
            ..Self::default()
        }
    }
}

const fn is_false(value: &bool) -> bool {
    !*value
}

const fn is_zero_u16(value: &u16) -> bool {
    *value == 0
}

const fn one_u16() -> u16 {
    1
}

const fn is_one_u16(value: &u16) -> bool {
    *value == 1
}

const fn is_zero_usize(value: &usize) -> bool {
    *value == 0
}

fn is_empty_text(value: &Text) -> bool {
    value == &Text::default()
}

/// Owned counterpart of `ratatui::widgets::Tabs`, plus a semantic select
/// action for native input.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tabs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block: Option<Block>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub titles: Vec<Line>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected: Option<usize>,
    #[serde(default, skip_serializing_if = "Style::is_empty")]
    pub style: Style,
    #[serde(default, skip_serializing_if = "Style::is_empty")]
    pub highlight_style: Style,
    pub divider: Span,
    pub padding_left: Line,
    pub padding_right: Line,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tab_ids: Vec<ItemId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_select: Option<ActionId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_reorder: Option<ActionId>,
    #[serde(skip)]
    pub(crate) node_id: Option<NodeId>,
}

impl Default for Tabs {
    fn default() -> Self {
        Self::new(Vec::<Line>::new())
    }
}

impl Tabs {
    pub fn new<I, T>(titles: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<Line>,
    {
        let titles: Vec<Line> = titles.into_iter().map(Into::into).collect();
        let selected = if titles.is_empty() { None } else { Some(0) };
        Self {
            block: None,
            titles,
            selected,
            style: Style::default(),
            highlight_style: Style::new().reversed(),
            divider: Span::raw("│"),
            padding_left: Line::raw(" "),
            padding_right: Line::raw(" "),
            tab_ids: Vec::new(),
            on_select: None,
            on_reorder: None,
            node_id: None,
        }
    }

    #[must_use]
    pub fn id(mut self, id: impl Into<NodeId>) -> Self {
        self.node_id = Some(id.into());
        self
    }

    #[must_use]
    pub fn on_select(mut self, action: impl Into<ActionId>) -> Self {
        self.on_select = Some(action.into());
        self
    }

    #[must_use]
    pub fn tab_ids<I, T>(mut self, ids: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<ItemId>,
    {
        self.tab_ids = ids.into_iter().map(Into::into).collect();
        self
    }

    #[must_use]
    pub fn reorderable(mut self, action: impl Into<ActionId>) -> Self {
        self.on_reorder = Some(action.into());
        self
    }

    #[must_use]
    pub fn block(mut self, block: Block) -> Self {
        self.block = Some(block);
        self
    }

    #[must_use]
    pub fn titles<I, T>(mut self, titles: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<Line>,
    {
        self.titles = titles.into_iter().map(Into::into).collect();
        self.selected = match self.titles.len() {
            0 => None,
            len => Some(self.selected.unwrap_or(0).min(len - 1)),
        };
        self
    }

    #[must_use]
    pub fn select<T>(mut self, selected: T) -> Self
    where
        T: Into<Option<usize>>,
    {
        self.selected = selected.into();
        self
    }

    #[must_use]
    pub fn style(mut self, style: impl Into<Style>) -> Self {
        self.style = style.into();
        self
    }

    #[must_use]
    pub fn highlight_style(mut self, style: impl Into<Style>) -> Self {
        self.highlight_style = style.into();
        self
    }

    #[must_use]
    pub fn divider(mut self, divider: impl Into<Span>) -> Self {
        self.divider = divider.into();
        self
    }

    #[must_use]
    pub fn padding<L, R>(mut self, left: L, right: R) -> Self
    where
        L: Into<Line>,
        R: Into<Line>,
    {
        self.padding_left = left.into();
        self.padding_right = right.into();
        self
    }

    #[must_use]
    pub fn padding_left(mut self, padding: impl Into<Line>) -> Self {
        self.padding_left = padding.into();
        self
    }

    #[must_use]
    pub fn padding_right(mut self, padding: impl Into<Line>) -> Self {
        self.padding_right = padding.into();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::model::{Color, Element, Node};

    #[test]
    fn tabs_builder_is_ratatui_shaped_and_keeps_semantics() {
        let node: Node = Tabs::new(["Overview", "Activity"])
            .select(1)
            .highlight_style(Style::new().fg(Color::Magenta).bold())
            .divider("•")
            .padding(" ", " ")
            .id("main-tabs")
            .on_select("select-tab")
            .into();
        assert_eq!(node.id.as_ref().unwrap().0, "main-tabs");
        let Element::Tabs(tabs) = &node.element else {
            panic!("expected tabs")
        };
        assert_eq!(tabs.selected, Some(1));
        assert_eq!(tabs.on_select.as_ref().unwrap().0, "select-tab");
        node.validate().unwrap();
    }

    #[test]
    fn tabs_can_clear_selection_like_ratatui() {
        let node: Node = Tabs::new(["Overview", "Activity"]).select(None).into();
        let Element::Tabs(tabs) = node.element else {
            panic!("expected tabs")
        };
        assert_eq!(tabs.selected, None);
    }

    #[test]
    fn list_builder_keeps_ratatui_options_and_reorder_identity() {
        let list = List::new([
            ListItem::new("Alpha").id("alpha"),
            ListItem::new("Beta").id("beta"),
        ])
        .id("cards")
        .reorderable("reorder-cards")
        .block(Block::bordered().title("Cards"))
        .style(Color::White)
        .direction(ListDirection::BottomToTop)
        .highlight_style(Style::new().reversed())
        .highlight_symbol(">> ")
        .repeat_highlight_symbol(true)
        .highlight_spacing(HighlightSpacing::Always)
        .scroll_padding(2)
        .select(1)
        .offset(1);

        assert_eq!(list.items[0].id.as_ref().unwrap().as_str(), "alpha");
        assert_eq!(list.direction, ListDirection::BottomToTop);
        assert_eq!(list.selected, Some(1));
        assert_eq!(list.offset, 1);
        assert_eq!(list.on_reorder.as_ref().unwrap().as_str(), "reorder-cards");
        assert_eq!(list.node_id.as_ref().unwrap().as_str(), "cards");
    }

    #[test]
    fn table_builder_keeps_rows_columns_state_and_reorder_actions() {
        let table = Table::new(
            [Row::new(["Alpha", "Ready"]), Row::new(["Beta", "Working"])],
            [Constraint::Percentage(70), Constraint::Percentage(30)],
        )
        .id("tasks")
        .row_ids(["alpha", "beta"])
        .column_ids(["title", "status"])
        .reorderable_rows("reorder-rows")
        .reorderable_columns("reorder-columns")
        .header(Row::new(["Title", "Status"]).style(Style::new().bold()))
        .footer(Row::new(["2 tasks", ""]))
        .column_spacing(2)
        .row_highlight_style(Style::new().reversed())
        .column_highlight_style(Style::new().underlined())
        .cell_highlight_style(Style::new().bold())
        .highlight_symbol("▶ ")
        .highlight_spacing(HighlightSpacing::Always)
        .flex(Flex::SpaceBetween)
        .select_cell(Some((1, 1)))
        .offset(1);

        assert_eq!(table.rows[0].id.as_ref().unwrap().as_str(), "alpha");
        assert_eq!(table.column_ids[1].as_str(), "status");
        assert_eq!(table.selected, Some(1));
        assert_eq!(table.selected_column, Some(1));
        assert_eq!(table.column_spacing, 2);
        assert_eq!(
            table.on_reorder_rows.as_ref().unwrap().as_str(),
            "reorder-rows"
        );
        assert_eq!(
            table.on_reorder_columns.as_ref().unwrap().as_str(),
            "reorder-columns"
        );
    }

    #[test]
    fn row_id_count_mismatch_is_reported_by_validation_without_panicking() {
        let node: Node = Table::new(
            [Row::new(["Alpha"]), Row::new(["Beta"])],
            [Constraint::Fill(1)],
        )
        .id("tasks")
        .row_ids(["alpha"])
        .reorderable_rows("reorder-rows")
        .into();

        assert!(node.validate().is_err());
    }

    #[test]
    fn tabs_can_declare_stable_ids_and_reorder_action() {
        let tabs = Tabs::new(["Overview", "Activity"])
            .id("main-tabs")
            .tab_ids(["overview", "activity"])
            .reorderable("reorder-tabs");

        assert_eq!(tabs.tab_ids[0].as_str(), "overview");
        assert_eq!(tabs.on_reorder.as_ref().unwrap().as_str(), "reorder-tabs");
    }

    #[test]
    fn row_constructor_and_serde_preserve_ratatui_height() {
        let row = Row::new(["Cell"]);
        assert_eq!(row.height, 1);

        let json = serde_json::to_string(&row).unwrap();
        assert!(!json.contains("height"));
        let round_trip: Row = serde_json::from_str(&json).unwrap();
        assert_eq!(round_trip.height, 1);

        let default_row = Row::default();
        assert_eq!(default_row.height, 0);
        let default_json = serde_json::to_string(&default_row).unwrap();
        assert!(default_json.contains("\"height\":0"));
        let default_round_trip: Row = serde_json::from_str(&default_json).unwrap();
        assert_eq!(default_round_trip.height, 0);
    }
}

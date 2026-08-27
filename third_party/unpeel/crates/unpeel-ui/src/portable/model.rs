use std::collections::HashSet;
use std::fmt;

use serde::{Deserialize, Serialize};

use super::canvas::Canvas;
use super::reorder::ItemId;
use super::widgets::{List, Paragraph, Table, Tabs};

/// Stable identity for an interactive or explicitly keyed view node.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodeId(pub String);

impl NodeId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl From<&str> for NodeId {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl From<String> for NodeId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<ItemId> for NodeId {
    fn from(value: ItemId) -> Self {
        Self(value.into_string())
    }
}

impl From<&ItemId> for NodeId {
    fn from(value: &ItemId) -> Self {
        Self(value.as_str().to_owned())
    }
}

impl From<NodeId> for ItemId {
    fn from(value: NodeId) -> Self {
        Self(value.0)
    }
}

impl From<&NodeId> for ItemId {
    fn from(value: &NodeId) -> Self {
        Self(value.0.clone())
    }
}

/// Stable semantic command emitted by an interactive node.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ActionId(pub String);

impl ActionId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ActionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl From<&str> for ActionId {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl From<String> for ActionId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

/// A keyed element in the portable tree.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<NodeId>,
    #[serde(flatten)]
    pub element: Element,
}

impl Node {
    pub fn new(element: impl Into<Element>) -> Self {
        let mut element = element.into();
        let id = match &mut element {
            Element::Layout(layout) => layout.node_id.take(),
            Element::Paragraph(paragraph) => paragraph.node_id.take(),
            Element::Tabs(tabs) => tabs.node_id.take(),
            Element::List(list) => list.node_id.take(),
            Element::Table(table) => table.node_id.take(),
            Element::Canvas(canvas) => canvas.node_id.take(),
        };
        Self { id, element }
    }

    #[must_use]
    pub fn id(mut self, id: impl Into<NodeId>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Validate invariants that serde alone cannot express.
    pub fn validate(&self) -> Result<(), ValidationError> {
        let mut ids = HashSet::new();
        self.validate_at("root", &mut ids)
    }

    fn validate_at(&self, path: &str, ids: &mut HashSet<String>) -> Result<(), ValidationError> {
        if let Some(id) = &self.id {
            validate_name(&id.0, path, "node id")?;
            if !ids.insert(id.0.clone()) {
                return Err(ValidationError::new(
                    path,
                    format!("duplicate node id `{}`", id.0),
                ));
            }
        }
        match &self.element {
            Element::Layout(layout) => {
                if layout.constraints.len() != layout.children.len() {
                    return Err(ValidationError::new(
                        path,
                        format!(
                            "layout has {} constraints for {} children",
                            layout.constraints.len(),
                            layout.children.len()
                        ),
                    ));
                }
                for (index, constraint) in layout.constraints.iter().enumerate() {
                    constraint.validate(&format!("{path}.constraints[{index}]"))?;
                }
                for (index, child) in layout.children.iter().enumerate() {
                    child.validate_at(&format!("{path}.children[{index}]"), ids)?;
                }
                validate_reorderable_nodes(
                    self.id.as_ref(),
                    layout.on_reorder.as_ref(),
                    &layout.children,
                    path,
                    "layout children",
                )?;
            }
            Element::Paragraph(_) => {}
            Element::Tabs(tabs) => {
                if let Some(selected) = tabs.selected {
                    if selected >= tabs.titles.len() {
                        return Err(ValidationError::new(
                            path,
                            format!(
                                "selected tab {selected} is outside {} titles",
                                tabs.titles.len()
                            ),
                        ));
                    }
                }
                if let Some(action) = &tabs.on_select {
                    validate_interaction(self.id.as_ref(), action, path, "selectable tabs")?;
                }
                validate_complete_ids(
                    &tabs.tab_ids,
                    tabs.titles.len(),
                    tabs.on_reorder.is_some(),
                    &format!("{path}.tabIds"),
                    "tab",
                )?;
                if let Some(action) = &tabs.on_reorder {
                    validate_interaction(self.id.as_ref(), action, path, "reorderable tabs")?;
                }
            }
            Element::List(list) => validate_list(self.id.as_ref(), list, path)?,
            Element::Table(table) => validate_table(self.id.as_ref(), table, path)?,
            Element::Canvas(canvas) => canvas.validate(path)?,
        }
        Ok(())
    }
}

/// Closed v1 element vocabulary.
///
/// This pre-release v1 includes the built-in slice implemented by this crate.
/// Once a non-Rust decoder ships, new discriminators require negotiation or a
/// protocol-version bump rather than being added silently.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Element {
    Layout(Box<Layout>),
    Paragraph(Box<Paragraph>),
    Tabs(Box<Tabs>),
    List(Box<List>),
    Table(Box<Table>),
    Canvas(Box<Canvas>),
}

macro_rules! impl_node_from_element {
    ($type:ty, $variant:ident) => {
        impl From<$type> for Element {
            fn from(value: $type) -> Self {
                Self::$variant(Box::new(value))
            }
        }

        impl From<$type> for Node {
            fn from(value: $type) -> Self {
                Self::new(value)
            }
        }
    };
}

impl_node_from_element!(Layout, Layout);
impl_node_from_element!(Paragraph, Paragraph);
impl_node_from_element!(Tabs, Tabs);
impl_node_from_element!(List, List);
impl_node_from_element!(Table, Table);
impl_node_from_element!(Canvas, Canvas);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationError {
    pub path: String,
    pub message: String,
}

impl ValidationError {
    pub fn new(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path, self.message)
    }
}

impl std::error::Error for ValidationError {}

fn validate_name(value: &str, path: &str, description: &str) -> Result<(), ValidationError> {
    if value.trim().is_empty() {
        Err(ValidationError::new(
            path,
            format!("{description} is empty"),
        ))
    } else {
        Ok(())
    }
}

fn validate_interaction(
    node_id: Option<&NodeId>,
    action: &ActionId,
    path: &str,
    description: &str,
) -> Result<(), ValidationError> {
    validate_name(&action.0, path, "action id")?;
    if node_id.is_none() {
        return Err(ValidationError::new(
            path,
            format!("{description} require a stable node id"),
        ));
    }
    Ok(())
}

fn validate_complete_ids(
    item_ids: &[ItemId],
    expected: usize,
    required: bool,
    path: &str,
    description: &str,
) -> Result<(), ValidationError> {
    if (required || !item_ids.is_empty()) && item_ids.len() != expected {
        return Err(ValidationError::new(
            path,
            format!(
                "{} {description} ids were provided for {expected} {description}s",
                item_ids.len()
            ),
        ));
    }
    validate_unique_item_ids(item_ids.iter(), path, description)
}

fn validate_optional_ids<'a>(
    item_ids: impl IntoIterator<Item = Option<&'a ItemId>>,
    expected: usize,
    required: bool,
    path: &str,
    description: &str,
) -> Result<(), ValidationError> {
    let item_ids = item_ids.into_iter().flatten().collect::<Vec<_>>();
    if required && item_ids.len() != expected {
        return Err(ValidationError::new(
            path,
            format!(
                "reorderable {description}s require a stable id on every item; found {} ids for {expected} items",
                item_ids.len()
            ),
        ));
    }
    validate_unique_item_ids(item_ids, path, description)
}

fn validate_unique_item_ids<'a>(
    item_ids: impl IntoIterator<Item = &'a ItemId>,
    path: &str,
    description: &str,
) -> Result<(), ValidationError> {
    let mut seen = HashSet::new();
    for item_id in item_ids {
        validate_name(item_id.as_str(), path, &format!("{description} id"))?;
        if !seen.insert(item_id.as_str()) {
            return Err(ValidationError::new(
                path,
                format!("duplicate {description} id `{item_id}`"),
            ));
        }
    }
    Ok(())
}

fn validate_reorderable_nodes(
    node_id: Option<&NodeId>,
    action: Option<&ActionId>,
    children: &[Node],
    path: &str,
    description: &str,
) -> Result<(), ValidationError> {
    let Some(action) = action else {
        return Ok(());
    };
    validate_interaction(node_id, action, path, &format!("reorderable {description}"))?;
    if let Some((index, _)) = children
        .iter()
        .enumerate()
        .find(|(_, child)| child.id.is_none())
    {
        return Err(ValidationError::new(
            format!("{path}.children[{index}]"),
            format!("reorderable {description} require every child to have a stable node id"),
        ));
    }
    Ok(())
}

fn validate_list(node_id: Option<&NodeId>, list: &List, path: &str) -> Result<(), ValidationError> {
    if let Some(selected) = list.selected {
        if selected >= list.items.len() {
            return Err(ValidationError::new(
                path,
                format!(
                    "selected list item {selected} is outside {} items",
                    list.items.len()
                ),
            ));
        }
    }
    validate_safe_usize(list.offset, &format!("{path}.offset"), "list offset")?;
    validate_safe_usize(
        list.scroll_padding,
        &format!("{path}.scrollPadding"),
        "list scroll padding",
    )?;
    validate_optional_ids(
        list.items.iter().map(|item| item.id.as_ref()),
        list.items.len(),
        list.on_reorder.is_some(),
        &format!("{path}.items"),
        "list item",
    )?;
    if let Some(action) = &list.on_reorder {
        validate_interaction(node_id, action, path, "reorderable lists")?;
    }
    Ok(())
}

fn validate_table(
    node_id: Option<&NodeId>,
    table: &Table,
    path: &str,
) -> Result<(), ValidationError> {
    for (index, constraint) in table.widths.iter().enumerate() {
        constraint.validate(&format!("{path}.widths[{index}]"))?;
    }
    if let Some((received, expected)) = table.row_id_count_error {
        return Err(ValidationError::new(
            format!("{path}.rows"),
            format!("{received} table row ids were provided for {expected} rows"),
        ));
    }
    if let Some(selected) = table.selected {
        if selected >= table.rows.len() {
            return Err(ValidationError::new(
                path,
                format!(
                    "selected table row {selected} is outside {} rows",
                    table.rows.len()
                ),
            ));
        }
    }

    let column_count = table
        .rows
        .iter()
        .map(|row| row.cells.len())
        .chain(table.header.iter().map(|row| row.cells.len()))
        .chain(table.footer.iter().map(|row| row.cells.len()))
        .max()
        .unwrap_or(0);
    if let Some(selected) = table.selected_column {
        if selected >= column_count {
            return Err(ValidationError::new(
                path,
                format!("selected table column {selected} is outside {column_count} columns"),
            ));
        }
    }
    validate_safe_usize(table.offset, &format!("{path}.offset"), "table offset")?;

    validate_optional_ids(
        table.rows.iter().map(|row| row.id.as_ref()),
        table.rows.len(),
        table.on_reorder_rows.is_some(),
        &format!("{path}.rows"),
        "table row",
    )?;
    validate_complete_ids(
        &table.column_ids,
        column_count,
        table.on_reorder_columns.is_some(),
        &format!("{path}.columnIds"),
        "table column",
    )?;

    if let Some(action) = &table.on_reorder_rows {
        validate_interaction(node_id, action, path, "reorderable table rows")?;
    }
    if let Some(action) = &table.on_reorder_columns {
        validate_interaction(node_id, action, path, "reorderable table columns")?;
        validate_table_column_arity(table, column_count, path)?;
    }
    if table.on_reorder_rows.is_some()
        && table.on_reorder_rows.as_ref() == table.on_reorder_columns.as_ref()
    {
        return Err(ValidationError::new(
            path,
            "table row and column reorder actions must be different",
        ));
    }
    Ok(())
}

fn validate_safe_usize(value: usize, path: &str, description: &str) -> Result<(), ValidationError> {
    const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
    if (value as u64) > MAX_SAFE_INTEGER {
        return Err(ValidationError::new(
            path,
            format!("{description} exceeds the cross-platform safe integer {MAX_SAFE_INTEGER}"),
        ));
    }
    Ok(())
}

fn validate_table_column_arity(
    table: &Table,
    column_count: usize,
    path: &str,
) -> Result<(), ValidationError> {
    if !table.widths.is_empty() && table.widths.len() != column_count {
        return Err(ValidationError::new(
            format!("{path}.widths"),
            format!(
                "reorderable table columns require {column_count} widths or no explicit widths"
            ),
        ));
    }
    for (index, row) in table.rows.iter().enumerate() {
        if row.cells.len() != column_count {
            return Err(ValidationError::new(
                format!("{path}.rows[{index}].cells"),
                format!("reorderable table columns require exactly {column_count} cells per row"),
            ));
        }
    }
    for (name, row) in [
        ("header", table.header.as_ref()),
        ("footer", table.footer.as_ref()),
    ] {
        if let Some(row) = row {
            if row.cells.len() != column_count {
                return Err(ValidationError::new(
                    format!("{path}.{name}.cells"),
                    format!(
                        "reorderable table columns require exactly {column_count} {name} cells"
                    ),
                ));
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Alignment {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Text {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lines: Vec<Line>,
    #[serde(default, skip_serializing_if = "Style::is_empty")]
    pub style: Style,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alignment: Option<Alignment>,
}

impl Text {
    pub fn raw(content: impl Into<String>) -> Self {
        content.into().into()
    }

    pub fn styled(content: impl Into<String>, style: impl Into<Style>) -> Self {
        let mut text = Self::raw(content);
        text.style = style.into();
        text
    }

    #[must_use]
    pub fn style(mut self, style: impl Into<Style>) -> Self {
        self.style = style.into();
        self
    }

    #[must_use]
    pub fn alignment(mut self, alignment: Alignment) -> Self {
        self.alignment = Some(alignment);
        self
    }
}

impl From<&str> for Text {
    fn from(value: &str) -> Self {
        value.to_owned().into()
    }
}

impl From<String> for Text {
    fn from(value: String) -> Self {
        Self {
            lines: value.split('\n').map(Line::raw).collect(),
            ..Self::default()
        }
    }
}

impl From<Line> for Text {
    fn from(value: Line) -> Self {
        Self {
            lines: vec![value],
            ..Self::default()
        }
    }
}

impl From<Vec<Line>> for Text {
    fn from(lines: Vec<Line>) -> Self {
        Self {
            lines,
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Line {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spans: Vec<Span>,
    #[serde(default, skip_serializing_if = "Style::is_empty")]
    pub style: Style,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alignment: Option<Alignment>,
}

impl Line {
    pub fn raw(content: impl Into<String>) -> Self {
        Self::from(Span::raw(content))
    }

    pub fn styled(content: impl Into<String>, style: impl Into<Style>) -> Self {
        Self::from(Span::styled(content, style))
    }

    #[must_use]
    pub fn style(mut self, style: impl Into<Style>) -> Self {
        self.style = style.into();
        self
    }

    #[must_use]
    pub fn alignment(mut self, alignment: Alignment) -> Self {
        self.alignment = Some(alignment);
        self
    }
}

impl From<&str> for Line {
    fn from(value: &str) -> Self {
        Self::raw(value)
    }
}

impl From<String> for Line {
    fn from(value: String) -> Self {
        Self::raw(value)
    }
}

impl From<Span> for Line {
    fn from(value: Span) -> Self {
        Self {
            spans: vec![value],
            ..Self::default()
        }
    }
}

impl From<Vec<Span>> for Line {
    fn from(spans: Vec<Span>) -> Self {
        Self {
            spans,
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Span {
    pub content: String,
    #[serde(default, skip_serializing_if = "Style::is_empty")]
    pub style: Style,
}

impl Span {
    pub fn raw(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            style: Style::default(),
        }
    }

    pub fn styled(content: impl Into<String>, style: impl Into<Style>) -> Self {
        Self {
            content: content.into(),
            style: style.into(),
        }
    }

    #[must_use]
    pub fn style(mut self, style: impl Into<Style>) -> Self {
        self.style = style.into();
        self
    }
}

impl From<&str> for Span {
    fn from(value: &str) -> Self {
        Self::raw(value)
    }
}

impl From<String> for Span {
    fn from(value: String) -> Self {
        Self::raw(value)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Color {
    #[default]
    Reset,
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    Gray,
    DarkGray,
    LightRed,
    LightGreen,
    LightYellow,
    LightBlue,
    LightMagenta,
    LightCyan,
    White,
    Rgb {
        red: u8,
        green: u8,
        blue: u8,
    },
    Indexed {
        index: u8,
    },
}

impl From<ratatui::style::Color> for Color {
    fn from(value: ratatui::style::Color) -> Self {
        match value {
            ratatui::style::Color::Reset => Self::Reset,
            ratatui::style::Color::Black => Self::Black,
            ratatui::style::Color::Red => Self::Red,
            ratatui::style::Color::Green => Self::Green,
            ratatui::style::Color::Yellow => Self::Yellow,
            ratatui::style::Color::Blue => Self::Blue,
            ratatui::style::Color::Magenta => Self::Magenta,
            ratatui::style::Color::Cyan => Self::Cyan,
            ratatui::style::Color::Gray => Self::Gray,
            ratatui::style::Color::DarkGray => Self::DarkGray,
            ratatui::style::Color::LightRed => Self::LightRed,
            ratatui::style::Color::LightGreen => Self::LightGreen,
            ratatui::style::Color::LightYellow => Self::LightYellow,
            ratatui::style::Color::LightBlue => Self::LightBlue,
            ratatui::style::Color::LightMagenta => Self::LightMagenta,
            ratatui::style::Color::LightCyan => Self::LightCyan,
            ratatui::style::Color::White => Self::White,
            ratatui::style::Color::Rgb(red, green, blue) => Self::Rgb { red, green, blue },
            ratatui::style::Color::Indexed(index) => Self::Indexed { index },
        }
    }
}

impl From<Color> for ratatui::style::Color {
    fn from(value: Color) -> Self {
        match value {
            Color::Reset => Self::Reset,
            Color::Black => Self::Black,
            Color::Red => Self::Red,
            Color::Green => Self::Green,
            Color::Yellow => Self::Yellow,
            Color::Blue => Self::Blue,
            Color::Magenta => Self::Magenta,
            Color::Cyan => Self::Cyan,
            Color::Gray => Self::Gray,
            Color::DarkGray => Self::DarkGray,
            Color::LightRed => Self::LightRed,
            Color::LightGreen => Self::LightGreen,
            Color::LightYellow => Self::LightYellow,
            Color::LightBlue => Self::LightBlue,
            Color::LightMagenta => Self::LightMagenta,
            Color::LightCyan => Self::LightCyan,
            Color::White => Self::White,
            Color::Rgb { red, green, blue } => Self::Rgb(red, green, blue),
            Color::Indexed { index } => Self::Indexed(index),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Modifier {
    Bold,
    Dim,
    Italic,
    Underlined,
    SlowBlink,
    RapidBlink,
    Reversed,
    Hidden,
    CrossedOut,
}

impl From<Modifier> for ratatui::style::Modifier {
    fn from(value: Modifier) -> Self {
        match value {
            Modifier::Bold => Self::BOLD,
            Modifier::Dim => Self::DIM,
            Modifier::Italic => Self::ITALIC,
            Modifier::Underlined => Self::UNDERLINED,
            Modifier::SlowBlink => Self::SLOW_BLINK,
            Modifier::RapidBlink => Self::RAPID_BLINK,
            Modifier::Reversed => Self::REVERSED,
            Modifier::Hidden => Self::HIDDEN,
            Modifier::CrossedOut => Self::CROSSED_OUT,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Style {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fg: Option<Color>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bg: Option<Color>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub underline_color: Option<Color>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub add_modifiers: Vec<Modifier>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sub_modifiers: Vec<Modifier>,
}

impl Style {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }

    #[must_use]
    pub fn fg(mut self, color: impl Into<Color>) -> Self {
        self.fg = Some(color.into());
        self
    }

    #[must_use]
    pub fn bg(mut self, color: impl Into<Color>) -> Self {
        self.bg = Some(color.into());
        self
    }

    #[must_use]
    pub fn underline_color(mut self, color: impl Into<Color>) -> Self {
        self.underline_color = Some(color.into());
        self
    }

    #[must_use]
    pub fn add_modifier(mut self, modifier: Modifier) -> Self {
        self.sub_modifiers
            .retain(|candidate| *candidate != modifier);
        if !self.add_modifiers.contains(&modifier) {
            self.add_modifiers.push(modifier);
        }
        self
    }

    #[must_use]
    pub fn remove_modifier(mut self, modifier: Modifier) -> Self {
        self.add_modifiers
            .retain(|candidate| *candidate != modifier);
        if !self.sub_modifiers.contains(&modifier) {
            self.sub_modifiers.push(modifier);
        }
        self
    }

    #[must_use]
    pub fn bold(self) -> Self {
        self.add_modifier(Modifier::Bold)
    }

    #[must_use]
    pub fn italic(self) -> Self {
        self.add_modifier(Modifier::Italic)
    }

    #[must_use]
    pub fn underlined(self) -> Self {
        self.add_modifier(Modifier::Underlined)
    }

    #[must_use]
    pub fn reversed(self) -> Self {
        self.add_modifier(Modifier::Reversed)
    }

    #[must_use]
    pub fn crossed_out(self) -> Self {
        self.add_modifier(Modifier::CrossedOut)
    }
}

impl From<Color> for Style {
    fn from(value: Color) -> Self {
        Self::new().fg(value)
    }
}

impl From<Modifier> for Style {
    fn from(value: Modifier) -> Self {
        Self::new().add_modifier(value)
    }
}

impl From<ratatui::style::Color> for Style {
    fn from(value: ratatui::style::Color) -> Self {
        Self::from(Color::from(value))
    }
}

impl From<ratatui::style::Modifier> for Style {
    fn from(value: ratatui::style::Modifier) -> Self {
        Self {
            add_modifiers: portable_modifiers(value),
            ..Self::default()
        }
    }
}

impl From<ratatui::style::Style> for Style {
    fn from(value: ratatui::style::Style) -> Self {
        Self {
            fg: value.fg.map(Color::from),
            bg: value.bg.map(Color::from),
            underline_color: value.underline_color.map(Color::from),
            add_modifiers: portable_modifiers(value.add_modifier),
            sub_modifiers: portable_modifiers(value.sub_modifier),
        }
    }
}

impl From<&Style> for ratatui::style::Style {
    fn from(value: &Style) -> Self {
        Self {
            fg: value.fg.map(ratatui::style::Color::from),
            bg: value.bg.map(ratatui::style::Color::from),
            underline_color: value.underline_color.map(ratatui::style::Color::from),
            add_modifier: ratatui_modifiers(&value.add_modifiers),
            sub_modifier: ratatui_modifiers(&value.sub_modifiers),
        }
    }
}

impl From<Style> for ratatui::style::Style {
    fn from(value: Style) -> Self {
        Self::from(&value)
    }
}

fn portable_modifiers(value: ratatui::style::Modifier) -> Vec<Modifier> {
    MODIFIERS
        .iter()
        .filter_map(|(portable, ratatui)| value.contains(*ratatui).then_some(*portable))
        .collect()
}

fn ratatui_modifiers(value: &[Modifier]) -> ratatui::style::Modifier {
    value
        .iter()
        .copied()
        .fold(ratatui::style::Modifier::empty(), |modifiers, modifier| {
            modifiers | ratatui::style::Modifier::from(modifier)
        })
}

const MODIFIERS: [(Modifier, ratatui::style::Modifier); 9] = [
    (Modifier::Bold, ratatui::style::Modifier::BOLD),
    (Modifier::Dim, ratatui::style::Modifier::DIM),
    (Modifier::Italic, ratatui::style::Modifier::ITALIC),
    (Modifier::Underlined, ratatui::style::Modifier::UNDERLINED),
    (Modifier::SlowBlink, ratatui::style::Modifier::SLOW_BLINK),
    (Modifier::RapidBlink, ratatui::style::Modifier::RAPID_BLINK),
    (Modifier::Reversed, ratatui::style::Modifier::REVERSED),
    (Modifier::Hidden, ratatui::style::Modifier::HIDDEN),
    (Modifier::CrossedOut, ratatui::style::Modifier::CROSSED_OUT),
];

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Block {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<Line>,
    #[serde(default, skip_serializing_if = "Borders::is_none")]
    pub borders: Borders,
    #[serde(default)]
    pub border_type: BorderType,
    #[serde(default, skip_serializing_if = "Style::is_empty")]
    pub style: Style,
    #[serde(default, skip_serializing_if = "Style::is_empty")]
    pub border_style: Style,
    #[serde(default, skip_serializing_if = "Style::is_empty")]
    pub title_style: Style,
    #[serde(default, skip_serializing_if = "Padding::is_zero")]
    pub padding: Padding,
}

impl Block {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bordered() -> Self {
        Self {
            borders: Borders::ALL,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn title(mut self, title: impl Into<Line>) -> Self {
        self.title = Some(title.into());
        self
    }

    #[must_use]
    pub fn borders(mut self, borders: Borders) -> Self {
        self.borders = borders;
        self
    }

    #[must_use]
    pub fn border_type(mut self, border_type: BorderType) -> Self {
        self.border_type = border_type;
        self
    }

    #[must_use]
    pub fn style(mut self, style: impl Into<Style>) -> Self {
        self.style = style.into();
        self
    }

    #[must_use]
    pub fn border_style(mut self, style: impl Into<Style>) -> Self {
        self.border_style = style.into();
        self
    }

    #[must_use]
    pub fn title_style(mut self, style: impl Into<Style>) -> Self {
        self.title_style = style.into();
        self
    }

    #[must_use]
    pub fn padding(mut self, padding: Padding) -> Self {
        self.padding = padding;
        self
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BorderType {
    #[default]
    Plain,
    Rounded,
    Double,
    Thick,
    QuadrantInside,
    QuadrantOutside,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Borders {
    #[serde(default)]
    pub top: bool,
    #[serde(default)]
    pub right: bool,
    #[serde(default)]
    pub bottom: bool,
    #[serde(default)]
    pub left: bool,
}

impl Borders {
    pub const NONE: Self = Self::new(false, false, false, false);
    pub const ALL: Self = Self::new(true, true, true, true);
    pub const TOP: Self = Self::new(true, false, false, false);
    pub const RIGHT: Self = Self::new(false, true, false, false);
    pub const BOTTOM: Self = Self::new(false, false, true, false);
    pub const LEFT: Self = Self::new(false, false, false, true);

    pub const fn new(top: bool, right: bool, bottom: bool, left: bool) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }

    pub const fn is_none(&self) -> bool {
        !self.top && !self.right && !self.bottom && !self.left
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Padding {
    pub left: u16,
    pub right: u16,
    pub top: u16,
    pub bottom: u16,
}

impl Padding {
    pub const ZERO: Self = Self::uniform(0);

    pub const fn new(left: u16, right: u16, top: u16, bottom: u16) -> Self {
        Self {
            left,
            right,
            top,
            bottom,
        }
    }

    pub const fn uniform(value: u16) -> Self {
        Self::new(value, value, value, value)
    }

    pub const fn horizontal(value: u16) -> Self {
        Self::new(value, value, 0, 0)
    }

    pub const fn vertical(value: u16) -> Self {
        Self::new(0, 0, value, value)
    }

    pub const fn is_zero(&self) -> bool {
        self.left == 0 && self.right == 0 && self.top == 0 && self.bottom == 0
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    Horizontal,
    #[default]
    Vertical,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Flex {
    Legacy,
    #[default]
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum Constraint {
    Min(u16),
    Max(u16),
    Length(u16),
    Percentage(u16),
    Ratio(u32, u32),
    Fill(u16),
}

impl Constraint {
    fn validate(&self, path: &str) -> Result<(), ValidationError> {
        match *self {
            Self::Percentage(value) if value > 100 => Err(ValidationError::new(
                path,
                "percentage must be between 0 and 100",
            )),
            Self::Ratio(_, 0) => Err(ValidationError::new(
                path,
                "ratio denominator must not be zero",
            )),
            _ => Ok(()),
        }
    }
}

impl From<u16> for Constraint {
    fn from(value: u16) -> Self {
        Self::Length(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum Spacing {
    Space(u16),
    Overlap(u16),
}

impl Default for Spacing {
    fn default() -> Self {
        Self::Space(0)
    }
}

impl From<u16> for Spacing {
    fn from(value: u16) -> Self {
        Self::Space(value)
    }
}

impl From<i16> for Spacing {
    fn from(value: i16) -> Self {
        if value < 0 {
            Self::Overlap(value.unsigned_abs())
        } else {
            Self::Space(value as u16)
        }
    }
}

/// A Ratatui layout plus the child nodes it divides its area between.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    pub direction: Direction,
    pub constraints: Vec<Constraint>,
    #[serde(default, skip_serializing_if = "Padding::is_zero")]
    pub margin: Padding,
    #[serde(default)]
    pub flex: Flex,
    #[serde(default)]
    pub spacing: Spacing,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Node>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_reorder: Option<ActionId>,
    #[serde(skip)]
    pub(crate) node_id: Option<NodeId>,
}

impl Default for Layout {
    fn default() -> Self {
        Self::vertical(Vec::<Constraint>::new())
    }
}

impl Layout {
    pub fn new<I>(direction: Direction, constraints: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<Constraint>,
    {
        Self {
            direction,
            constraints: constraints.into_iter().map(Into::into).collect(),
            margin: Padding::ZERO,
            flex: Flex::default(),
            spacing: Spacing::default(),
            children: Vec::new(),
            on_reorder: None,
            node_id: None,
        }
    }

    pub fn vertical<I>(constraints: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<Constraint>,
    {
        Self::new(Direction::Vertical, constraints)
    }

    pub fn horizontal<I>(constraints: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<Constraint>,
    {
        Self::new(Direction::Horizontal, constraints)
    }

    #[must_use]
    pub fn id(mut self, id: impl Into<NodeId>) -> Self {
        self.node_id = Some(id.into());
        self
    }

    #[must_use]
    pub fn margin(mut self, margin: u16) -> Self {
        self.margin = Padding::uniform(margin);
        self
    }

    #[must_use]
    pub fn horizontal_margin(mut self, margin: u16) -> Self {
        self.margin.left = margin;
        self.margin.right = margin;
        self
    }

    #[must_use]
    pub fn vertical_margin(mut self, margin: u16) -> Self {
        self.margin.top = margin;
        self.margin.bottom = margin;
        self
    }

    #[must_use]
    pub fn flex(mut self, flex: Flex) -> Self {
        self.flex = flex;
        self
    }

    #[must_use]
    pub fn spacing(mut self, spacing: impl Into<Spacing>) -> Self {
        self.spacing = spacing.into();
        self
    }

    #[must_use]
    pub fn children<I, N>(mut self, children: I) -> Self
    where
        I: IntoIterator<Item = N>,
        N: Into<Node>,
    {
        self.children = children.into_iter().map(Into::into).collect();
        self
    }

    /// Make the child order interactive for native renderers and TUI helpers.
    ///
    /// Every child must have a stable node id. Reorder events carry the full
    /// ordered list of those ids as `EventValue::TextList`.
    #[must_use]
    pub fn reorderable(mut self, action: impl Into<ActionId>) -> Self {
        self.on_reorder = Some(action.into());
        self
    }

    pub fn push(&mut self, child: impl Into<Node>) {
        self.children.push(child.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::widgets::{List, ListItem, Row, Table, Tabs};

    #[test]
    fn text_values_are_owned_and_round_trip() {
        let text = Text::from(vec![Line::from(vec![
            Span::raw("hello "),
            Span::styled("world", Style::new().bold().fg(Color::Magenta)),
        ])]);
        let json = serde_json::to_string(&text).unwrap();
        assert_eq!(serde_json::from_str::<Text>(&json).unwrap(), text);
    }

    #[test]
    fn validation_requires_ids_for_interaction_and_rejects_duplicates() {
        let invalid: Node = Tabs::new(["One", "Two"]).on_select("select-tab").into();
        assert!(invalid.validate().is_err());

        let child: Node = Tabs::new(["One"]).id("tabs").into();
        let tree: Node = Layout::vertical([Constraint::Fill(1), Constraint::Fill(1)])
            .children([child.clone(), child])
            .into();
        assert!(tree.validate().is_err());
    }

    #[test]
    fn node_new_preserves_an_id_declared_by_a_widget_builder() {
        let node = Node::new(Tabs::new(["One"]).id("tabs").on_select("select-tab"));
        assert_eq!(node.id.as_ref().map(NodeId::as_str), Some("tabs"));
        node.validate().unwrap();
    }

    #[test]
    fn reorderable_layout_requires_parent_and_child_ids() {
        let missing_parent: Node = Layout::vertical([Constraint::Fill(1), Constraint::Fill(1)])
            .children([
                Node::from(Paragraph::new("One")).id("one"),
                Node::from(Paragraph::new("Two")).id("two"),
            ])
            .reorderable("reorder-cards")
            .into();
        assert!(missing_parent.validate().is_err());

        let missing_child: Node = Layout::vertical([Constraint::Fill(1), Constraint::Fill(1)])
            .id("cards")
            .children([
                Node::from(Paragraph::new("One")).id("one"),
                Node::from(Paragraph::new("Two")),
            ])
            .reorderable("reorder-cards")
            .into();
        assert!(missing_child.validate().is_err());

        let valid: Node = Layout::vertical([Constraint::Fill(1), Constraint::Fill(1)])
            .id("cards")
            .children([
                Node::from(Paragraph::new("One")).id("one"),
                Node::from(Paragraph::new("Two")).id("two"),
            ])
            .reorderable("reorder-cards")
            .into();
        valid.validate().unwrap();
    }

    #[test]
    fn tabs_and_lists_require_complete_unique_reorder_ids() {
        let tabs: Node = Tabs::new(["One", "Two"])
            .id("tabs")
            .tab_ids(["same", "same"])
            .reorderable("reorder-tabs")
            .into();
        assert!(tabs.validate().is_err());

        let list: Node = List::new([ListItem::new("One").id("one"), ListItem::new("Two")])
            .id("list")
            .reorderable("reorder-items")
            .into();
        assert!(list.validate().is_err());
    }

    #[test]
    fn table_validates_row_and_column_reorder_scopes() {
        let valid: Node = Table::new(
            [
                Row::new(["One", "Ready"]).id("one"),
                Row::new(["Two", "Busy"]).id("two"),
            ],
            [Constraint::Percentage(60), Constraint::Percentage(40)],
        )
        .id("tasks")
        .column_ids(["title", "status"])
        .reorderable_rows("reorder-rows")
        .reorderable_columns("reorder-columns")
        .into();
        valid.validate().unwrap();

        let same_action: Node = Table::new(
            [Row::new(["One", "Ready"]).id("one")],
            [Constraint::Percentage(60), Constraint::Percentage(40)],
        )
        .id("tasks")
        .column_ids(["title", "status"])
        .reorderable_rows("reorder")
        .reorderable_columns("reorder")
        .into();
        assert!(same_action.validate().is_err());

        let ragged_columns: Node = Table::new(
            [Row::new(["One", "Ready"]), Row::new(["Two"])],
            [Constraint::Percentage(60), Constraint::Percentage(40)],
        )
        .id("tasks")
        .column_ids(["title", "status"])
        .reorderable_columns("reorder-columns")
        .into();
        assert!(ragged_columns.validate().is_err());

        let empty_rows: Node = Table::default()
            .header(Row::new(["Title", "Status"]))
            .id("empty-table")
            .column_ids(["title", "status"])
            .reorderable_columns("reorder-columns")
            .into();
        empty_rows.validate().unwrap();
    }

    #[test]
    fn table_rejects_widths_that_ratatui_cannot_construct() {
        let invalid_percentage: Node =
            Table::new([Row::new(["One"])], [Constraint::Percentage(101)]).into();
        assert!(invalid_percentage.validate().is_err());

        let invalid_ratio: Node = Table::new([Row::new(["One"])], [Constraint::Ratio(1, 0)]).into();
        assert!(invalid_ratio.validate().is_err());
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn collection_offsets_stay_in_the_cross_platform_safe_range() {
        let unsafe_offset = 9_007_199_254_740_992usize;
        let list: Node = List::new(["One"]).offset(unsafe_offset).into();
        assert!(list.validate().is_err());

        let table: Node = Table::new([Row::new(["One"])], [Constraint::Fill(1)])
            .offset(unsafe_offset)
            .into();
        assert!(table.validate().is_err());
    }

    #[test]
    fn ratatui_styles_and_shared_colors_are_portable() {
        let raw = ratatui::style::Style::new()
            .fg(crate::style::FOCUS)
            .bg(ratatui::style::Color::Black)
            .underline_color(ratatui::style::Color::LightBlue)
            .add_modifier(ratatui::style::Modifier::BOLD | ratatui::style::Modifier::ITALIC)
            .remove_modifier(ratatui::style::Modifier::DIM);

        let portable = Style::from(raw);
        assert_eq!(
            portable.fg,
            Some(Color::Rgb {
                red: 156,
                green: 147,
                blue: 184,
            })
        );
        assert_eq!(
            portable.add_modifiers,
            vec![Modifier::Bold, Modifier::Italic]
        );
        assert_eq!(portable.sub_modifiers, vec![Modifier::Dim]);
        assert_eq!(ratatui::style::Style::from(&portable), raw);

        let span = Span::styled("selected", crate::style::FOCUS);
        assert_eq!(span.style.fg, portable.fg);
    }
}

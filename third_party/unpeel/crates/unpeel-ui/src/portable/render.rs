//! Ratatui renderer for the portable Unpeel App view tree.

use ratatui::buffer::Buffer;
use ratatui::layout::{
    Alignment as RatatuiAlignment, Constraint as RatatuiConstraint, Direction as RatatuiDirection,
    Flex as RatatuiFlex, Layout as RatatuiLayout, Rect, Spacing as RatatuiSpacing,
};
use ratatui::style::{Color as RatatuiColor, Modifier as RatatuiModifier, Style as RatatuiStyle};
use ratatui::symbols::Marker as RatatuiMarker;
use ratatui::text::{Line as RatatuiLine, Span as RatatuiSpan, Text as RatatuiText};
use ratatui::widgets::canvas::{
    Canvas as RatatuiCanvas, Circle as RatatuiCircle, Context as RatatuiCanvasContext,
    Line as RatatuiCanvasLine, Map as RatatuiMap, MapResolution as RatatuiMapResolution,
    Points as RatatuiPoints, Rectangle as RatatuiRectangle,
};
use ratatui::widgets::{
    Block as RatatuiBlock, BorderType as RatatuiBorderType, Borders as RatatuiBorders,
    Cell as RatatuiCell, HighlightSpacing as RatatuiHighlightSpacing, List as RatatuiList,
    ListDirection as RatatuiListDirection, ListItem as RatatuiListItem,
    ListState as RatatuiListState, Padding as RatatuiPadding, Paragraph as RatatuiParagraph,
    Row as RatatuiRow, StatefulWidget, Table as RatatuiTable, TableState as RatatuiTableState,
    Tabs as RatatuiTabs, Widget, Wrap,
};

use super::canvas::{Canvas, MapResolution, Marker, Primitive};
use super::hit::{HitMap, HitTarget};
use super::model::{
    Alignment, Block, BorderType, Borders, Color, Constraint, Direction, Element, Flex, Layout,
    Line, Modifier, Node, NodeId, Padding, Spacing, Span, Style, Text,
};
use super::widgets::{
    Cell, HighlightSpacing, List, ListDirection, ListItem, Paragraph, Row, Table, Tabs,
};

/// Render a portable node into a Ratatui buffer.
///
/// Layout nodes are lowered through Ratatui's own constraint solver. Leaf
/// nodes are rendered by their matching Ratatui widget, so standalone Apps
/// keep normal terminal behavior while the same tree remains available to
/// native renderers.
pub fn render(node: &Node, area: Rect, buffer: &mut Buffer) {
    render_node(node, area, buffer, None);
}

/// Render a portable node while recording where identified nodes (and their
/// tabs, list items, and table rows) landed, for renderer-local hit testing.
///
/// Appends to `hits` without clearing it, so one map can cover a frame
/// composed of several trees; clear it once per frame, or render through
/// [`super::hit::HitTestWidget`] which does.
pub fn render_with_hits(node: &Node, area: Rect, buffer: &mut Buffer, hits: &mut HitMap) {
    render_node(node, area, buffer, Some(hits));
}

fn render_node(node: &Node, area: Rect, buffer: &mut Buffer, mut hits: Option<&mut HitMap>) {
    if let (Some(hits), Some(id)) = (hits.as_deref_mut(), &node.id) {
        hits.push(id, HitTarget::Node, area);
    }
    match &node.element {
        Element::Layout(layout) => render_layout(layout, area, buffer, hits),
        Element::Paragraph(paragraph) => render_paragraph(paragraph, area, buffer),
        Element::Tabs(tabs) => render_tabs(tabs, area, buffer, hit_scope(hits, &node.id)),
        Element::List(list) => render_list(list, area, buffer, hit_scope(hits, &node.id)),
        Element::Table(table) => render_table(table, area, buffer, hit_scope(hits, &node.id)),
        Element::Canvas(canvas) => render_canvas(canvas, area, buffer),
    }
}

/// Item-level regions are only recordable for nodes with a stable id — the
/// same precondition the action contract puts on interactive nodes.
fn hit_scope<'a>(
    hits: Option<&'a mut HitMap>,
    id: &'a Option<NodeId>,
) -> Option<(&'a mut HitMap, &'a NodeId)> {
    match (hits, id) {
        (Some(hits), Some(id)) => Some((hits, id)),
        _ => None,
    }
}

/// Adapter that lets a portable tree be passed to `Frame::render_widget`.
#[derive(Clone, Copy, Debug)]
pub struct RatatuiWidget<'a> {
    node: &'a Node,
}

impl<'a> RatatuiWidget<'a> {
    pub const fn new(node: &'a Node) -> Self {
        Self { node }
    }

    pub const fn node(&self) -> &'a Node {
        self.node
    }
}

impl<'a> From<&'a Node> for RatatuiWidget<'a> {
    fn from(node: &'a Node) -> Self {
        Self::new(node)
    }
}

impl Widget for RatatuiWidget<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        render(self.node, area, buffer);
    }
}

/// Render a portable tree by reference anywhere Ratatui accepts a `Widget`.
impl Widget for &Node {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        render(self, area, buffer);
    }
}

fn render_layout(layout: &Layout, area: Rect, buffer: &mut Buffer, mut hits: Option<&mut HitMap>) {
    let area = inset(area, layout.margin);
    if area.is_empty() || layout.children.is_empty() {
        return;
    }

    let constraints = layout.constraints.iter().copied().map(ratatui_constraint);
    let chunks = RatatuiLayout::new(ratatui_direction(layout.direction), constraints)
        .flex(ratatui_flex(layout.flex))
        .spacing(ratatui_spacing(layout.spacing))
        .split(area);

    for (child, child_area) in layout.children.iter().zip(chunks.iter().copied()) {
        render_node(child, child_area, buffer, hits.as_deref_mut());
    }
}

fn render_paragraph(paragraph: &Paragraph, area: Rect, buffer: &mut Buffer) {
    let mut widget = RatatuiParagraph::new(ratatui_text(&paragraph.text))
        .style(ratatui_style(&paragraph.style))
        .scroll(paragraph.scroll)
        .alignment(ratatui_alignment(paragraph.alignment));
    if let Some(wrap) = paragraph.wrap {
        widget = widget.wrap(Wrap { trim: wrap.trim });
    }
    if let Some(block) = &paragraph.block {
        widget = widget.block(ratatui_block(block));
    }
    widget.render(area, buffer);
}

fn render_tabs(tabs: &Tabs, area: Rect, buffer: &mut Buffer, hits: Option<(&mut HitMap, &NodeId)>) {
    let titles = tabs.titles.iter().map(ratatui_line);
    let mut widget = RatatuiTabs::new(titles)
        .select(tabs.selected)
        .style(ratatui_style(&tabs.style))
        .highlight_style(ratatui_style(&tabs.highlight_style))
        .divider(ratatui_span(&tabs.divider))
        .padding(
            ratatui_line(&tabs.padding_left),
            ratatui_line(&tabs.padding_right),
        );
    if let Some(block) = &tabs.block {
        widget = widget.block(ratatui_block(block));
    }
    widget.render(area, buffer);

    if let Some((hits, id)) = hits {
        record_tab_titles(hits, id, tabs, widget_inner(tabs.block.as_ref(), area));
    }
}

/// Walk the same left-to-right sequence Ratatui's `Tabs` renders — left
/// padding, title, right padding, divider between entries — recording the
/// cells each title occupies.
fn record_tab_titles(hits: &mut HitMap, id: &NodeId, tabs: &Tabs, inner: Rect) {
    if inner.is_empty() {
        return;
    }
    let advance = |x: u16, width: usize| -> u16 {
        x.saturating_add(u16::try_from(width).unwrap_or(u16::MAX))
            .min(inner.right())
    };
    let mut x = inner.left();
    let last = tabs.titles.len().saturating_sub(1);
    for (index, title) in tabs.titles.iter().enumerate() {
        if x >= inner.right() {
            break;
        }
        x = advance(x, ratatui_line(&tabs.padding_left).width());
        let title_end = advance(x, ratatui_line(title).width());
        hits.push(
            id,
            HitTarget::Tab { index },
            Rect::new(x, inner.y, title_end.saturating_sub(x), 1),
        );
        x = advance(title_end, ratatui_line(&tabs.padding_right).width());
        if index < last {
            x = advance(x, ratatui_span(&tabs.divider).width());
        }
    }
}

fn widget_inner(block: Option<&Block>, area: Rect) -> Rect {
    block.map_or(area, |block| ratatui_block(block).inner(area))
}

fn render_list(list: &List, area: Rect, buffer: &mut Buffer, hits: Option<(&mut HitMap, &NodeId)>) {
    let items = list.items.iter().map(ratatui_list_item);
    let mut widget = RatatuiList::new(items)
        .style(ratatui_style(&list.style))
        .direction(ratatui_list_direction(list.direction))
        .highlight_style(ratatui_style(&list.highlight_style))
        .repeat_highlight_symbol(list.repeat_highlight_symbol)
        .highlight_spacing(ratatui_highlight_spacing(list.highlight_spacing))
        .scroll_padding(list.scroll_padding);
    if let Some(symbol) = &list.highlight_symbol {
        widget = widget.highlight_symbol(symbol.as_str());
    }
    if let Some(block) = &list.block {
        widget = widget.block(ratatui_block(block));
    }
    let mut state = RatatuiListState::default()
        .with_offset(list.offset)
        .with_selected(list.selected);
    StatefulWidget::render(widget, area, buffer, &mut state);

    // Ratatui adjusts the offset during render to keep the selection visible;
    // reading it back keeps the recorded rows aligned with the drawn frame.
    if let Some((hits, id)) = hits {
        record_list_items(
            hits,
            id,
            list,
            widget_inner(list.block.as_ref(), area),
            state.offset(),
        );
    }
}

fn record_list_items(hits: &mut HitMap, id: &NodeId, list: &List, inner: Rect, offset: usize) {
    if inner.is_empty() {
        return;
    }
    let mut push = |index: usize, top: u16, height: u16| {
        hits.push(
            id,
            HitTarget::ListItem {
                index,
                item: list.items[index].id.clone(),
            },
            Rect::new(inner.x, top, inner.width, height),
        );
    };
    match list.direction {
        ListDirection::TopToBottom => {
            let mut y = inner.top();
            for (index, item) in list.items.iter().enumerate().skip(offset) {
                if y >= inner.bottom() {
                    break;
                }
                let height = u16::try_from(item.content.lines.len()).unwrap_or(u16::MAX);
                if height == 0 {
                    continue;
                }
                push(index, y, height.min(inner.bottom() - y));
                y = y.saturating_add(height);
            }
        }
        ListDirection::BottomToTop => {
            let mut bottom = inner.bottom();
            for (index, item) in list.items.iter().enumerate().skip(offset) {
                if bottom <= inner.top() {
                    break;
                }
                let height = u16::try_from(item.content.lines.len()).unwrap_or(u16::MAX);
                if height == 0 {
                    continue;
                }
                let shown = height.min(bottom - inner.top());
                push(index, bottom - shown, shown);
                bottom -= shown;
            }
        }
    }
}

fn render_table(
    table: &Table,
    area: Rect,
    buffer: &mut Buffer,
    hits: Option<(&mut HitMap, &NodeId)>,
) {
    let rows = table.rows.iter().map(ratatui_row);
    let widths = table.widths.iter().copied().map(ratatui_constraint);
    let mut widget = RatatuiTable::new(rows, widths)
        .column_spacing(table.column_spacing)
        .style(ratatui_style(&table.style))
        .row_highlight_style(ratatui_style(&table.row_highlight_style))
        .column_highlight_style(ratatui_style(&table.column_highlight_style))
        .cell_highlight_style(ratatui_style(&table.cell_highlight_style))
        .highlight_symbol(ratatui_text(&table.highlight_symbol))
        .highlight_spacing(ratatui_highlight_spacing(table.highlight_spacing))
        .flex(ratatui_flex(table.flex));
    if let Some(header) = &table.header {
        widget = widget.header(ratatui_row(header));
    }
    if let Some(footer) = &table.footer {
        widget = widget.footer(ratatui_row(footer));
    }
    if let Some(block) = &table.block {
        widget = widget.block(ratatui_block(block));
    }
    let mut state = RatatuiTableState::default()
        .with_offset(table.offset)
        .with_selected(table.selected)
        .with_selected_column(table.selected_column);
    StatefulWidget::render(widget, area, buffer, &mut state);

    if let Some((hits, id)) = hits {
        record_table_rows(
            hits,
            id,
            table,
            widget_inner(table.block.as_ref(), area),
            state.offset(),
        );
    }
}

fn record_table_rows(hits: &mut HitMap, id: &NodeId, table: &Table, inner: Rect, offset: usize) {
    if inner.is_empty() {
        return;
    }
    let total = |row: &Row| -> u16 {
        row.top_margin
            .saturating_add(row.height)
            .saturating_add(row.bottom_margin)
    };
    let mut y = inner.top();
    if let Some(header) = &table.header {
        y = y.saturating_add(total(header)).min(inner.bottom());
    }
    let mut bottom = inner.bottom();
    if let Some(footer) = &table.footer {
        bottom = bottom.saturating_sub(total(footer)).max(y);
    }
    for (index, row) in table.rows.iter().enumerate().skip(offset) {
        y = y.saturating_add(row.top_margin);
        if y >= bottom {
            break;
        }
        hits.push(
            id,
            HitTarget::TableRow {
                index,
                item: row.id.clone(),
            },
            Rect::new(inner.x, y, inner.width, row.height.min(bottom - y)),
        );
        y = y
            .saturating_add(row.height)
            .saturating_add(row.bottom_margin);
    }
}

fn render_canvas(canvas: &Canvas, area: Rect, buffer: &mut Buffer) {
    let mut widget = RatatuiCanvas::default()
        .x_bounds(canvas.x_bounds)
        .y_bounds(canvas.y_bounds)
        .background_color(ratatui_color(canvas.background_color))
        .marker(ratatui_marker(canvas.marker))
        .paint(|context| paint_canvas(context, canvas));
    if let Some(block) = &canvas.block {
        widget = widget.block(ratatui_block(block));
    }
    widget.render(area, buffer);
}

fn paint_canvas(context: &mut RatatuiCanvasContext<'_>, canvas: &Canvas) {
    for layer in &canvas.layers {
        for primitive in &layer.primitives {
            match primitive {
                Primitive::Line(line) => context.draw(&RatatuiCanvasLine {
                    x1: line.x1,
                    y1: line.y1,
                    x2: line.x2,
                    y2: line.y2,
                    color: ratatui_color(line.color),
                }),
                Primitive::Rectangle(rectangle) => context.draw(&RatatuiRectangle {
                    x: rectangle.x,
                    y: rectangle.y,
                    width: rectangle.width,
                    height: rectangle.height,
                    color: ratatui_color(rectangle.color),
                }),
                Primitive::Circle(circle) => context.draw(&RatatuiCircle {
                    x: circle.x,
                    y: circle.y,
                    radius: circle.radius,
                    color: ratatui_color(circle.color),
                }),
                Primitive::Points(points) => context.draw(&RatatuiPoints {
                    coords: &points.coords,
                    color: ratatui_color(points.color),
                }),
                Primitive::Map(map) => context.draw(&RatatuiMap {
                    resolution: ratatui_map_resolution(map.resolution),
                    color: ratatui_color(map.color),
                }),
            }
        }
        // The portable recorder preserves explicit layer boundaries, including
        // an intentionally empty layer.
        context.layer();
    }
    for label in &canvas.labels {
        // Canvas paint callbacks are higher-ranked over the Context label
        // lifetime. Own label strings so the callback never stores a borrow
        // of the portable tree in that context.
        context.print(label.x, label.y, ratatui_line_owned(&label.line));
    }
}

fn ratatui_text(text: &Text) -> RatatuiText<'_> {
    let mut value = RatatuiText::from(text.lines.iter().map(ratatui_line).collect::<Vec<_>>())
        .style(ratatui_style(&text.style));
    if let Some(alignment) = text.alignment {
        value = value.alignment(ratatui_alignment(alignment));
    }
    value
}

fn ratatui_line(line: &Line) -> RatatuiLine<'_> {
    let mut value = RatatuiLine::from(line.spans.iter().map(ratatui_span).collect::<Vec<_>>())
        .style(ratatui_style(&line.style));
    if let Some(alignment) = line.alignment {
        value = value.alignment(ratatui_alignment(alignment));
    }
    value
}

fn ratatui_line_owned(line: &Line) -> RatatuiLine<'static> {
    let spans = line
        .spans
        .iter()
        .map(|span| RatatuiSpan::styled(span.content.clone(), ratatui_style(&span.style)))
        .collect::<Vec<_>>();
    let mut value = RatatuiLine::from(spans).style(ratatui_style(&line.style));
    if let Some(alignment) = line.alignment {
        value = value.alignment(ratatui_alignment(alignment));
    }
    value
}

fn ratatui_span(span: &Span) -> RatatuiSpan<'_> {
    RatatuiSpan::styled(span.content.as_str(), ratatui_style(&span.style))
}

fn ratatui_list_item(item: &ListItem) -> RatatuiListItem<'_> {
    RatatuiListItem::new(ratatui_text(&item.content)).style(ratatui_style(&item.style))
}

fn ratatui_row(row: &Row) -> RatatuiRow<'_> {
    RatatuiRow::new(row.cells.iter().map(ratatui_cell))
        .height(row.height)
        .top_margin(row.top_margin)
        .bottom_margin(row.bottom_margin)
        .style(ratatui_style(&row.style))
}

fn ratatui_cell(cell: &Cell) -> RatatuiCell<'_> {
    RatatuiCell::new(ratatui_text(&cell.content)).style(ratatui_style(&cell.style))
}

fn ratatui_block(block: &Block) -> RatatuiBlock<'_> {
    let mut value = RatatuiBlock::default()
        .borders(ratatui_borders(block.borders))
        .border_type(ratatui_border_type(block.border_type))
        .style(ratatui_style(&block.style))
        .border_style(ratatui_style(&block.border_style))
        .title_style(ratatui_style(&block.title_style))
        .padding(RatatuiPadding::new(
            block.padding.left,
            block.padding.right,
            block.padding.top,
            block.padding.bottom,
        ));
    if let Some(title) = &block.title {
        value = value.title(ratatui_line(title));
    }
    value
}

fn ratatui_style(style: &Style) -> RatatuiStyle {
    let mut value = RatatuiStyle::new();
    if let Some(color) = style.fg {
        value = value.fg(ratatui_color(color));
    }
    if let Some(color) = style.bg {
        value = value.bg(ratatui_color(color));
    }
    if let Some(color) = style.underline_color {
        value = value.underline_color(ratatui_color(color));
    }
    for modifier in style.add_modifiers.iter().copied() {
        value = value.add_modifier(ratatui_modifier(modifier));
    }
    for modifier in style.sub_modifiers.iter().copied() {
        value = value.remove_modifier(ratatui_modifier(modifier));
    }
    value
}

const fn ratatui_color(color: Color) -> RatatuiColor {
    match color {
        Color::Reset => RatatuiColor::Reset,
        Color::Black => RatatuiColor::Black,
        Color::Red => RatatuiColor::Red,
        Color::Green => RatatuiColor::Green,
        Color::Yellow => RatatuiColor::Yellow,
        Color::Blue => RatatuiColor::Blue,
        Color::Magenta => RatatuiColor::Magenta,
        Color::Cyan => RatatuiColor::Cyan,
        Color::Gray => RatatuiColor::Gray,
        Color::DarkGray => RatatuiColor::DarkGray,
        Color::LightRed => RatatuiColor::LightRed,
        Color::LightGreen => RatatuiColor::LightGreen,
        Color::LightYellow => RatatuiColor::LightYellow,
        Color::LightBlue => RatatuiColor::LightBlue,
        Color::LightMagenta => RatatuiColor::LightMagenta,
        Color::LightCyan => RatatuiColor::LightCyan,
        Color::White => RatatuiColor::White,
        Color::Rgb { red, green, blue } => RatatuiColor::Rgb(red, green, blue),
        Color::Indexed { index } => RatatuiColor::Indexed(index),
    }
}

const fn ratatui_modifier(modifier: Modifier) -> RatatuiModifier {
    match modifier {
        Modifier::Bold => RatatuiModifier::BOLD,
        Modifier::Dim => RatatuiModifier::DIM,
        Modifier::Italic => RatatuiModifier::ITALIC,
        Modifier::Underlined => RatatuiModifier::UNDERLINED,
        Modifier::SlowBlink => RatatuiModifier::SLOW_BLINK,
        Modifier::RapidBlink => RatatuiModifier::RAPID_BLINK,
        Modifier::Reversed => RatatuiModifier::REVERSED,
        Modifier::Hidden => RatatuiModifier::HIDDEN,
        Modifier::CrossedOut => RatatuiModifier::CROSSED_OUT,
    }
}

const fn ratatui_alignment(alignment: Alignment) -> RatatuiAlignment {
    match alignment {
        Alignment::Left => RatatuiAlignment::Left,
        Alignment::Center => RatatuiAlignment::Center,
        Alignment::Right => RatatuiAlignment::Right,
    }
}

const fn ratatui_direction(direction: Direction) -> RatatuiDirection {
    match direction {
        Direction::Horizontal => RatatuiDirection::Horizontal,
        Direction::Vertical => RatatuiDirection::Vertical,
    }
}

const fn ratatui_flex(flex: Flex) -> RatatuiFlex {
    match flex {
        Flex::Legacy => RatatuiFlex::Legacy,
        Flex::Start => RatatuiFlex::Start,
        Flex::End => RatatuiFlex::End,
        Flex::Center => RatatuiFlex::Center,
        Flex::SpaceBetween => RatatuiFlex::SpaceBetween,
        Flex::SpaceAround => RatatuiFlex::SpaceAround,
    }
}

const fn ratatui_constraint(constraint: Constraint) -> RatatuiConstraint {
    match constraint {
        Constraint::Min(value) => RatatuiConstraint::Min(value),
        Constraint::Max(value) => RatatuiConstraint::Max(value),
        Constraint::Length(value) => RatatuiConstraint::Length(value),
        Constraint::Percentage(value) => RatatuiConstraint::Percentage(value),
        Constraint::Ratio(numerator, denominator) => {
            RatatuiConstraint::Ratio(numerator, denominator)
        }
        Constraint::Fill(value) => RatatuiConstraint::Fill(value),
    }
}

const fn ratatui_spacing(spacing: Spacing) -> RatatuiSpacing {
    match spacing {
        Spacing::Space(value) => RatatuiSpacing::Space(value),
        Spacing::Overlap(value) => RatatuiSpacing::Overlap(value),
    }
}

const fn ratatui_list_direction(direction: ListDirection) -> RatatuiListDirection {
    match direction {
        ListDirection::TopToBottom => RatatuiListDirection::TopToBottom,
        ListDirection::BottomToTop => RatatuiListDirection::BottomToTop,
    }
}

const fn ratatui_highlight_spacing(spacing: HighlightSpacing) -> RatatuiHighlightSpacing {
    match spacing {
        HighlightSpacing::Always => RatatuiHighlightSpacing::Always,
        HighlightSpacing::WhenSelected => RatatuiHighlightSpacing::WhenSelected,
        HighlightSpacing::Never => RatatuiHighlightSpacing::Never,
    }
}

const fn ratatui_marker(marker: Marker) -> RatatuiMarker {
    match marker {
        Marker::Dot => RatatuiMarker::Dot,
        Marker::Block => RatatuiMarker::Block,
        Marker::Bar => RatatuiMarker::Bar,
        Marker::Braille => RatatuiMarker::Braille,
        Marker::HalfBlock => RatatuiMarker::HalfBlock,
    }
}

const fn ratatui_map_resolution(resolution: MapResolution) -> RatatuiMapResolution {
    match resolution {
        MapResolution::Low => RatatuiMapResolution::Low,
        MapResolution::High => RatatuiMapResolution::High,
    }
}

const fn ratatui_border_type(border_type: BorderType) -> RatatuiBorderType {
    match border_type {
        BorderType::Plain => RatatuiBorderType::Plain,
        BorderType::Rounded => RatatuiBorderType::Rounded,
        BorderType::Double => RatatuiBorderType::Double,
        BorderType::Thick => RatatuiBorderType::Thick,
        BorderType::QuadrantInside => RatatuiBorderType::QuadrantInside,
        BorderType::QuadrantOutside => RatatuiBorderType::QuadrantOutside,
    }
}

fn ratatui_borders(borders: Borders) -> RatatuiBorders {
    let mut value = RatatuiBorders::NONE;
    if borders.top {
        value |= RatatuiBorders::TOP;
    }
    if borders.right {
        value |= RatatuiBorders::RIGHT;
    }
    if borders.bottom {
        value |= RatatuiBorders::BOTTOM;
    }
    if borders.left {
        value |= RatatuiBorders::LEFT;
    }
    value
}

fn inset(area: Rect, padding: Padding) -> Rect {
    let left = padding.left.min(area.width);
    let width_after_left = area.width.saturating_sub(left);
    let right = padding.right.min(width_after_left);
    let top = padding.top.min(area.height);
    let height_after_top = area.height.saturating_sub(top);
    let bottom = padding.bottom.min(height_after_top);

    Rect::new(
        area.x.saturating_add(left),
        area.y.saturating_add(top),
        width_after_left.saturating_sub(right),
        height_after_top.saturating_sub(bottom),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::canvas::{Canvas, Points};
    use crate::portable::model::{Constraint, Layout, Style};
    use crate::portable::widgets::{List, ListItem, Paragraph, Row, Table, Tabs};

    #[test]
    fn renders_tabs_with_selection_style() {
        let node: Node = Tabs::new(["Overview", "Activity"])
            .select(1)
            .padding("", "")
            .divider("|")
            .highlight_style(Style::new().fg(Color::Magenta).bold())
            .into();
        let area = Rect::new(0, 0, 20, 1);
        let mut buffer = Buffer::empty(area);

        render(&node, area, &mut buffer);

        let mut expected = Buffer::with_lines(["Overview|Activity   "]);
        expected.set_style(
            Rect::new(9, 0, 8, 1),
            RatatuiStyle::new()
                .fg(RatatuiColor::Magenta)
                .add_modifier(RatatuiModifier::BOLD),
        );
        assert_eq!(buffer, expected);
    }

    #[test]
    fn renders_layout_children_through_ratatui_constraints() {
        let node: Node = Layout::vertical([Constraint::Length(1), Constraint::Length(1)])
            .spacing(1_u16)
            .children([Paragraph::new("top"), Paragraph::new("bottom")])
            .into();
        let area = Rect::new(0, 0, 8, 3);
        let mut buffer = Buffer::empty(area);

        RatatuiWidget::new(&node).render(area, &mut buffer);

        assert_eq!(
            buffer,
            Buffer::with_lines(["top     ", "        ", "bottom  "])
        );
    }

    #[test]
    fn renders_list_through_ratatui_with_inline_state() {
        let node: Node = List::new([
            ListItem::new("Alpha").id("alpha"),
            ListItem::new("Beta").id("beta"),
        ])
        .id("tasks")
        .reorderable("reorder-tasks")
        .highlight_symbol("> ")
        .highlight_spacing(HighlightSpacing::Always)
        .highlight_style(Style::new().fg(Color::Magenta).bold())
        .select(1)
        .into();
        let area = Rect::new(0, 0, 10, 2);
        let mut buffer = Buffer::empty(area);

        render(&node, area, &mut buffer);

        assert_eq!(buffer[(0, 0)].symbol(), " ");
        assert_eq!(buffer[(2, 0)].symbol(), "A");
        assert_eq!(buffer[(0, 1)].symbol(), ">");
        assert_eq!(buffer[(2, 1)].symbol(), "B");
        assert_eq!(buffer[(2, 1)].fg, RatatuiColor::Magenta);
        assert!(buffer[(2, 1)].modifier.contains(RatatuiModifier::BOLD));
    }

    #[test]
    fn renders_table_through_ratatui_with_row_and_column_state() {
        let node: Node = Table::new(
            [
                Row::new(["Alpha", "Ready"]).id("alpha"),
                Row::new(["Beta", "Busy"]).id("beta"),
            ],
            [Constraint::Length(7), Constraint::Length(6)],
        )
        .id("tasks")
        .column_ids(["title", "status"])
        .reorderable_rows("reorder-rows")
        .reorderable_columns("reorder-columns")
        .header(Row::new(["Title", "Status"]))
        .highlight_symbol("> ")
        .highlight_spacing(HighlightSpacing::Always)
        .row_highlight_style(Style::new().fg(Color::Magenta))
        .column_highlight_style(Style::new().underlined())
        .select_cell(Some((1, 1)))
        .into();
        let area = Rect::new(0, 0, 18, 3);
        let mut buffer = Buffer::empty(area);

        render(&node, area, &mut buffer);

        assert!(buffer
            .content()
            .iter()
            .any(|cell| cell.symbol() == "B" && cell.fg == RatatuiColor::Magenta));
        assert!(buffer.content().iter().any(
            |cell| cell.symbol() == "u" && cell.modifier.contains(RatatuiModifier::UNDERLINED)
        ));
    }

    #[test]
    fn renders_recorded_canvas_primitives_and_labels() {
        let node: Node = Canvas::default()
            .marker(Marker::Block)
            .x_bounds([0.0, 10.0])
            .y_bounds([0.0, 10.0])
            .paint(|context| {
                context.draw(&Points::new([(0.0, 0.0), (10.0, 10.0)], Color::Cyan));
                context.print(5.0, 5.0, "X");
            })
            .into();
        let area = Rect::new(0, 0, 5, 3);
        let mut buffer = Buffer::empty(area);

        render(&node, area, &mut buffer);

        assert_eq!(buffer[(4, 0)].symbol(), "█");
        assert_eq!(buffer[(4, 0)].fg, RatatuiColor::Cyan);
        assert_eq!(buffer[(0, 2)].symbol(), "█");
        assert_eq!(buffer[(0, 2)].fg, RatatuiColor::Cyan);
        assert!(buffer.content().iter().any(|cell| cell.symbol() == "X"));
    }
}

//! Renderer-local hit testing for the portable tree.
//!
//! The wire contract never carries pointer coordinates or key codes; a
//! renderer maps its own gestures to semantic actions on stable node ids.
//! Native and web renderers get that mapping from their platform (a SwiftUI
//! `Button` or a DOM event already knows its node). The terminal renderer has
//! only a cell grid, so it records *where identified nodes actually rendered*
//! while drawing and answers "which node is at cell (x, y)" afterwards —
//! geometry never leaves the renderer.

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::widgets::Widget;

use super::model::{Node, NodeId};
use super::render::render_with_hits;
use super::reorder::ItemId;

/// What part of an identified node a point landed on.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HitTarget {
    /// The node's whole rendered area (including any block chrome).
    Node,
    /// One tab title of a `Tabs` node.
    Tab { index: usize },
    /// One visible item of a `List` node.
    ListItem { index: usize, item: Option<ItemId> },
    /// One visible body row of a `Table` node.
    TableRow { index: usize, item: Option<ItemId> },
}

/// One recorded rectangle owned by an identified node.
#[derive(Clone, Debug, PartialEq)]
pub struct HitRegion {
    pub node: NodeId,
    pub target: HitTarget,
    pub area: Rect,
}

/// Rectangles recorded during one render pass, in paint order.
///
/// Regions are pushed parent-first and owner-before-items, so the *last*
/// region containing a point is the most specific one. `render` (without a
/// map) costs nothing; `render_with_hits` appends to the map it is given, so
/// a frame composed of several trees (an overlay above a main view) can share
/// one map — clear it once per frame, or use [`HitTestWidget`] which does.
#[derive(Clone, Debug, Default)]
pub struct HitMap {
    regions: Vec<HitRegion>,
}

impl HitMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.regions.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.regions.is_empty()
    }

    pub fn regions(&self) -> &[HitRegion] {
        &self.regions
    }

    /// The most specific recorded region containing the cell, if any.
    pub fn hit(&self, x: u16, y: u16) -> Option<&HitRegion> {
        let position = Position::new(x, y);
        self.regions
            .iter()
            .rev()
            .find(|region| region.area.contains(position))
    }

    pub(crate) fn push(&mut self, node: &NodeId, target: HitTarget, area: Rect) {
        if area.is_empty() {
            return;
        }
        self.regions.push(HitRegion {
            node: node.clone(),
            target,
            area,
        });
    }
}

/// Frame-level adapter in Ratatui's `&mut self` widget style: rendering
/// repaints the tree and re-records where identified nodes landed, so the
/// event loop can resolve the next mouse click against the frame the user
/// actually saw.
#[derive(Debug)]
pub struct HitTestWidget<'a> {
    node: &'a Node,
    map: HitMap,
}

impl<'a> HitTestWidget<'a> {
    pub fn new(node: &'a Node) -> Self {
        Self {
            node,
            map: HitMap::new(),
        }
    }

    pub fn map(&self) -> &HitMap {
        &self.map
    }

    pub fn hit(&self, x: u16, y: u16) -> Option<&HitRegion> {
        self.map.hit(x, y)
    }
}

impl Widget for &mut HitTestWidget<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        self.map.clear();
        render_with_hits(self.node, area, buffer, &mut self.map);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::model::{Constraint, Layout, Node};
    use crate::portable::widgets::{List, ListDirection, ListItem, Paragraph, Row, Table, Tabs};
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    fn rendered(node: &Node, area: Rect) -> HitMap {
        let mut buffer = Buffer::empty(area);
        let mut map = HitMap::new();
        render_with_hits(node, area, &mut buffer, &mut map);
        map
    }

    #[test]
    fn nested_layout_resolves_to_the_deepest_identified_node() {
        let node: Node = Node::new(
            Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).children([
                Node::new(Paragraph::new("top")).id("top"),
                Node::new(Paragraph::new("bottom")).id("bottom"),
            ]),
        )
        .id("root");
        let map = rendered(&node, Rect::new(0, 0, 8, 2));

        assert_eq!(map.hit(3, 0).unwrap().node.as_str(), "top");
        assert_eq!(map.hit(3, 1).unwrap().node.as_str(), "bottom");
        assert!(map.hit(3, 5).is_none());
    }

    #[test]
    fn unidentified_nodes_record_nothing() {
        let node: Node = Paragraph::new("anonymous").into();
        let map = rendered(&node, Rect::new(0, 0, 10, 1));

        assert!(map.is_empty());
    }

    #[test]
    fn list_rows_map_to_items_after_scroll_adjustment() {
        let items =
            (0..6).map(|index| ListItem::new(format!("Item {index}")).id(format!("item-{index}")));
        let node: Node = List::new(items)
            .id("tasks")
            .block(crate::portable::model::Block::bordered())
            .select(5)
            .into();
        // Inner area is 2 rows; selecting the last item forces Ratatui to
        // scroll, so the recorded rows must come from the post-render offset.
        let map = rendered(&node, Rect::new(0, 0, 10, 4));

        let hit = map.hit(2, 1).unwrap();
        assert_eq!(hit.node.as_str(), "tasks");
        assert_eq!(
            hit.target,
            HitTarget::ListItem {
                index: 4,
                item: Some("item-4".into())
            }
        );
        let hit = map.hit(2, 2).unwrap();
        assert_eq!(
            hit.target,
            HitTarget::ListItem {
                index: 5,
                item: Some("item-5".into())
            }
        );
        // The border is inside the node's area but not any item's.
        assert_eq!(map.hit(0, 0).unwrap().target, HitTarget::Node);
    }

    #[test]
    fn bottom_to_top_list_records_upward() {
        let node: Node = List::new([
            ListItem::new("zero").id("zero"),
            ListItem::new("one").id("one"),
        ])
        .id("log")
        .direction(ListDirection::BottomToTop)
        .into();
        let map = rendered(&node, Rect::new(0, 0, 8, 3));

        assert_eq!(
            map.hit(0, 2).unwrap().target,
            HitTarget::ListItem {
                index: 0,
                item: Some("zero".into())
            }
        );
        assert_eq!(
            map.hit(0, 1).unwrap().target,
            HitTarget::ListItem {
                index: 1,
                item: Some("one".into())
            }
        );
    }

    #[test]
    fn table_rows_map_below_the_header() {
        let node: Node = Table::new(
            [
                Row::new(["Alpha"]).id("alpha"),
                Row::new(["Beta"]).id("beta"),
                Row::new(["Gamma"]).id("gamma"),
            ],
            [Constraint::Length(8)],
        )
        .id("grid")
        .header(Row::new(["Title"]))
        .into();
        let map = rendered(&node, Rect::new(0, 0, 10, 3));

        assert_eq!(map.hit(0, 0).unwrap().target, HitTarget::Node);
        assert_eq!(
            map.hit(0, 1).unwrap().target,
            HitTarget::TableRow {
                index: 0,
                item: Some("alpha".into())
            }
        );
        assert_eq!(
            map.hit(0, 2).unwrap().target,
            HitTarget::TableRow {
                index: 1,
                item: Some("beta".into())
            }
        );
    }

    #[test]
    fn tab_titles_map_individually_and_dividers_fall_back_to_the_node() {
        let node: Node = Tabs::new(["One", "Two"])
            .id("tabs")
            .padding("", "")
            .divider("|")
            .into();
        // Renders as `One|Two`.
        let map = rendered(&node, Rect::new(0, 0, 10, 1));

        assert_eq!(map.hit(1, 0).unwrap().target, HitTarget::Tab { index: 0 });
        assert_eq!(map.hit(3, 0).unwrap().target, HitTarget::Node);
        assert_eq!(map.hit(5, 0).unwrap().target, HitTarget::Tab { index: 1 });
    }

    #[test]
    fn hit_test_widget_rerecords_each_render() {
        let node: Node = Node::new(Paragraph::new("hello")).id("greeting");
        let area = Rect::new(0, 0, 8, 1);
        let mut widget = HitTestWidget::new(&node);
        let mut buffer = Buffer::empty(area);

        (&mut widget).render(area, &mut buffer);
        assert_eq!(widget.map().regions().len(), 1);
        (&mut widget).render(area, &mut buffer);
        assert_eq!(widget.map().regions().len(), 1);
        assert_eq!(widget.hit(0, 0).unwrap().node.as_str(), "greeting");
    }
}

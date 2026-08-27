//! Serializable, Ratatui-shaped UI values for Unpeel Apps.
//!
//! These types retain widget meaning until a frontend chooses how to render
//! it. The terminal renderer can lower them to real Ratatui widgets, while a
//! native client can map the same values to platform controls.

pub mod canvas;
pub mod hit;
pub mod model;
pub mod protocol;
pub mod render;
pub mod reorder;
pub mod widgets;

pub use canvas::{
    Canvas, CanvasContext, CanvasLayer, Circle, Label, Map, MapResolution, Marker, Points,
    Primitive, Rectangle, Shape,
};
pub use hit::{HitMap, HitRegion, HitTarget, HitTestWidget};
pub use model::{
    ActionId, Alignment, Block, BorderType, Borders, Color, Constraint, Direction, Element, Flex,
    Layout, Line, Modifier, Node, NodeId, Padding, Spacing, Span, Style, Text, ValidationError,
};
pub use protocol::{
    read_message, write_message, ActionEvent, AppMetadata, ClientHello, EventKind, EventValue,
    HostHello, Message, ProtocolError, RenderMode, Snapshot, MAX_FRAME_BYTES, MAX_SAFE_INTEGER,
    NAME as PROTOCOL_NAME, UI_MODE_ENV, VERSION as PROTOCOL_VERSION,
};
pub use render::{render, render_with_hits, RatatuiWidget};
pub use reorder::{
    apply_order, AppliedOrder, ItemId, ReorderCommand, ReorderError, ReorderState, ReorderUpdate,
};
pub use widgets::{
    Cell, HighlightSpacing, List, ListDirection, ListItem, Paragraph, Row, Table, Tabs, Wrap,
};

/// The imports most App views need.
pub mod prelude {
    pub use super::canvas::{
        Canvas, CanvasContext, Circle, Label, Map, MapResolution, Marker, Points, Rectangle,
    };
    pub use super::hit::{HitMap, HitRegion, HitTarget, HitTestWidget};
    pub use super::model::{
        ActionId, Alignment, Block, BorderType, Borders, Color, Constraint, Direction, Flex,
        Layout, Line, Modifier, Node, NodeId, Padding, Spacing, Span, Style, Text,
    };
    pub use super::protocol::{
        read_message, write_message, ActionEvent, AppMetadata, ClientHello, EventKind, EventValue,
        HostHello, Message, ProtocolError, RenderMode, Snapshot, MAX_FRAME_BYTES, MAX_SAFE_INTEGER,
        NAME as PROTOCOL_NAME, UI_MODE_ENV, VERSION as PROTOCOL_VERSION,
    };
    pub use super::render::{render, render_with_hits, RatatuiWidget};
    pub use super::reorder::{
        apply_order, AppliedOrder, ItemId, ReorderCommand, ReorderError, ReorderState,
        ReorderUpdate,
    };
    pub use super::widgets::{
        Cell, HighlightSpacing, List, ListDirection, ListItem, Paragraph, Row, Table, Tabs, Wrap,
    };
}

use serde::{Deserialize, Serialize};

use super::model::{Block, Color, Line as TextLine, NodeId, ValidationError};

/// Marker used when lowering a canvas to terminal cells.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Marker {
    Dot,
    Block,
    Bar,
    #[default]
    Braille,
    HalfBlock,
}

/// A serializable scene recorded by [`Canvas::paint`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Canvas {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block: Option<Block>,
    pub x_bounds: [f64; 2],
    pub y_bounds: [f64; 2],
    #[serde(default)]
    pub background_color: Color,
    #[serde(default)]
    pub marker: Marker,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<CanvasLayer>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<Label>,
    #[serde(skip)]
    pub(crate) node_id: Option<NodeId>,
}

impl Default for Canvas {
    fn default() -> Self {
        Self {
            block: None,
            x_bounds: [0.0, 0.0],
            y_bounds: [0.0, 0.0],
            background_color: Color::Reset,
            marker: Marker::Braille,
            layers: Vec::new(),
            labels: Vec::new(),
            node_id: None,
        }
    }
}

impl Canvas {
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
    pub fn x_bounds(mut self, bounds: [f64; 2]) -> Self {
        self.x_bounds = bounds;
        self
    }

    #[must_use]
    pub fn y_bounds(mut self, bounds: [f64; 2]) -> Self {
        self.y_bounds = bounds;
        self
    }

    #[must_use]
    pub fn background_color(mut self, color: impl Into<Color>) -> Self {
        self.background_color = color.into();
        self
    }

    #[must_use]
    pub fn marker(mut self, marker: Marker) -> Self {
        self.marker = marker;
        self
    }

    /// Record a Ratatui-like paint closure immediately into owned shapes.
    /// No closure, reference, or terminal buffer enters the wire model.
    #[must_use]
    pub fn paint<F>(mut self, paint: F) -> Self
    where
        F: FnOnce(&mut CanvasContext),
    {
        let mut context = CanvasContext::default();
        paint(&mut context);
        context.finish();
        self.layers = context.layers;
        self.labels = context.labels;
        self
    }

    pub(crate) fn validate(&self, path: &str) -> Result<(), ValidationError> {
        let has_coordinates =
            !self.labels.is_empty() || self.layers.iter().any(|layer| !layer.primitives.is_empty());
        validate_bounds(self.x_bounds, &format!("{path}.xBounds"), has_coordinates)?;
        validate_bounds(self.y_bounds, &format!("{path}.yBounds"), has_coordinates)?;
        for (layer_index, layer) in self.layers.iter().enumerate() {
            for (primitive_index, primitive) in layer.primitives.iter().enumerate() {
                primitive.validate(&format!(
                    "{path}.layers[{layer_index}].primitives[{primitive_index}]"
                ))?;
            }
        }
        for (index, label) in self.labels.iter().enumerate() {
            validate_finite(label.x, &format!("{path}.labels[{index}].x"))?;
            validate_finite(label.y, &format!("{path}.labels[{index}].y"))?;
        }
        Ok(())
    }
}

fn validate_bounds(
    bounds: [f64; 2],
    path: &str,
    require_non_degenerate: bool,
) -> Result<(), ValidationError> {
    validate_finite(bounds[0], &format!("{path}[0]"))?;
    validate_finite(bounds[1], &format!("{path}[1]"))?;
    if require_non_degenerate && bounds[0] >= bounds[1] {
        return Err(ValidationError::new(
            path,
            "lower bound must be less than upper bound",
        ));
    }
    Ok(())
}

fn validate_finite(value: f64, path: &str) -> Result<(), ValidationError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(ValidationError::new(path, "coordinate must be finite"))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasLayer {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub primitives: Vec<Primitive>,
}

/// Recording equivalent of `ratatui::widgets::canvas::Context`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CanvasContext {
    layers: Vec<CanvasLayer>,
    labels: Vec<Label>,
    current: Vec<Primitive>,
}

impl CanvasContext {
    pub fn draw<S>(&mut self, shape: &S)
    where
        S: Shape + ?Sized,
    {
        self.current.push(shape.to_primitive());
    }

    /// Finish the current layer and begin another, matching Ratatui Canvas.
    pub fn layer(&mut self) {
        self.layers.push(CanvasLayer {
            primitives: std::mem::take(&mut self.current),
        });
    }

    /// Labels are kept above all painted layers, matching Ratatui Canvas.
    pub fn print<T>(&mut self, x: f64, y: f64, line: T)
    where
        T: Into<TextLine>,
    {
        self.labels.push(Label {
            x,
            y,
            line: line.into(),
        });
    }

    fn finish(&mut self) {
        if !self.current.is_empty() {
            self.layer();
        }
    }
}

/// A canvas value that can be recorded as a portable primitive.
pub trait Shape {
    fn to_primitive(&self) -> Primitive;
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Primitive {
    Line(Line),
    Rectangle(Rectangle),
    Circle(Circle),
    Points(Points),
    Map(Map),
}

impl Primitive {
    fn validate(&self, path: &str) -> Result<(), ValidationError> {
        let values: &[f64] = match self {
            Self::Line(line) => &[line.x1, line.y1, line.x2, line.y2],
            Self::Rectangle(rectangle) => {
                &[rectangle.x, rectangle.y, rectangle.width, rectangle.height]
            }
            Self::Circle(circle) => &[circle.x, circle.y, circle.radius],
            Self::Points(points) => {
                for (index, (x, y)) in points.coords.iter().enumerate() {
                    validate_finite(*x, &format!("{path}.coords[{index}][0]"))?;
                    validate_finite(*y, &format!("{path}.coords[{index}][1]"))?;
                }
                return Ok(());
            }
            Self::Map(_) => return Ok(()),
        };
        for (index, value) in values.iter().enumerate() {
            validate_finite(*value, &format!("{path}.coordinate[{index}]"))?;
        }
        match self {
            Self::Rectangle(value) if value.width < 0.0 || value.height < 0.0 => Err(
                ValidationError::new(path, "rectangle width and height must not be negative"),
            ),
            Self::Circle(value) if value.radius < 0.0 => Err(ValidationError::new(
                path,
                "circle radius must not be negative",
            )),
            _ => Ok(()),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Line {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    #[serde(default)]
    pub color: Color,
}

impl Line {
    pub const fn new(x1: f64, y1: f64, x2: f64, y2: f64, color: Color) -> Self {
        Self {
            x1,
            y1,
            x2,
            y2,
            color,
        }
    }
}

impl Shape for Line {
    fn to_primitive(&self) -> Primitive {
        Primitive::Line(self.clone())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rectangle {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    #[serde(default)]
    pub color: Color,
}

impl Shape for Rectangle {
    fn to_primitive(&self) -> Primitive {
        Primitive::Rectangle(self.clone())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Circle {
    pub x: f64,
    pub y: f64,
    pub radius: f64,
    #[serde(default)]
    pub color: Color,
}

impl Shape for Circle {
    fn to_primitive(&self) -> Primitive {
        Primitive::Circle(self.clone())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Points {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub coords: Vec<(f64, f64)>,
    #[serde(default)]
    pub color: Color,
}

impl Points {
    pub fn new<I>(coords: I, color: Color) -> Self
    where
        I: IntoIterator<Item = (f64, f64)>,
    {
        Self {
            coords: coords.into_iter().collect(),
            color,
        }
    }
}

impl Shape for Points {
    fn to_primitive(&self) -> Primitive {
        Primitive::Points(self.clone())
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MapResolution {
    #[default]
    Low,
    High,
}

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Map {
    #[serde(default)]
    pub resolution: MapResolution,
    #[serde(default)]
    pub color: Color,
}

impl Shape for Map {
    fn to_primitive(&self) -> Primitive {
        Primitive::Map(self.clone())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Label {
    pub x: f64,
    pub y: f64,
    pub line: TextLine,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::model::Node;

    #[test]
    fn paint_records_shapes_layers_and_top_labels() {
        let node: Node = Canvas::default()
            .x_bounds([-10.0, 10.0])
            .y_bounds([-10.0, 10.0])
            .paint(|context| {
                context.draw(&Line::new(0.0, 0.0, 1.0, 1.0, Color::White));
                context.layer();
                context.draw(&Circle {
                    x: 2.0,
                    y: 3.0,
                    radius: 1.0,
                    color: Color::Red,
                });
                context.print(2.0, 3.0, "point");
            })
            .into();
        node.validate().unwrap();
        let json = serde_json::to_string(&node).unwrap();
        assert!(json.contains("\"layers\""));
        assert!(json.contains("\"labels\""));
        assert_eq!(serde_json::from_str::<Node>(&json).unwrap(), node);
    }

    #[test]
    fn validation_rejects_unrenderable_coordinates() {
        let node: Node = Canvas::default()
            .x_bounds([0.0, f64::INFINITY])
            .y_bounds([0.0, 1.0])
            .into();
        assert!(node.validate().is_err());
    }

    #[test]
    fn an_empty_default_canvas_is_valid_like_ratatui() {
        let node: Node = Canvas::default()
            .block(Block::bordered().title("Empty canvas"))
            .into();
        node.validate().unwrap();
    }
}

use std::collections::BTreeSet;
use std::io::Cursor;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use unpeel_ui::portable::{
    read_message, Alignment, BorderType, Color, Constraint, Direction, Element, EventKind,
    EventValue, Flex, HighlightSpacing, ListDirection, MapResolution, Marker, Message, Modifier,
    Primitive, ProtocolError, RenderMode, Spacing, PROTOCOL_NAME, PROTOCOL_VERSION,
};

const SCHEMA_JSON: &str = include_str!("../../../protocol/unpeel-ui-v1.schema.json");
const FIXTURES_JSON: &str = include_str!("../../../protocol/unpeel-ui-fixtures-v1.json");
const STREAM_NDJSON: &[u8] = include_bytes!("../../../protocol/unpeel-ui-stream-v1.ndjson");

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixtureDocument {
    protocol: String,
    protocol_version: u32,
    description: String,
    valid_messages: Vec<Value>,
    wire_value_matrix: WireValueMatrix,
    compatibility_cases: Vec<CompatibilityCase>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct WireValueMatrix {
    event_kinds: Vec<EventKind>,
    event_values: Vec<EventValue>,
    constraints: Vec<Constraint>,
    flex: Vec<Flex>,
    directions: Vec<Direction>,
    list_directions: Vec<ListDirection>,
    highlight_spacings: Vec<HighlightSpacing>,
    spacing: Vec<Spacing>,
    alignments: Vec<Alignment>,
    colors: Vec<Color>,
    modifiers: Vec<Modifier>,
    border_types: Vec<BorderType>,
    markers: Vec<Marker>,
    map_resolutions: Vec<MapResolution>,
    render_modes: Vec<RenderMode>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompatibilityCase {
    name: String,
    expected_error: String,
    message: Value,
}

fn fixtures() -> FixtureDocument {
    serde_json::from_str(FIXTURES_JSON).expect("canonical fixture document must be valid JSON")
}

fn decode_valid_messages(document: &FixtureDocument) -> Vec<Message> {
    document
        .valid_messages
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let message: Message = serde_json::from_value(value.clone())
                .unwrap_or_else(|error| panic!("validMessages[{index}] did not decode: {error}"));
            message
                .validate()
                .unwrap_or_else(|error| panic!("validMessages[{index}] did not validate: {error}"));
            if let Message::Snapshot(snapshot) = &message {
                snapshot.root.validate().unwrap_or_else(|error| {
                    panic!("validMessages[{index}].root did not validate: {error}")
                });
            }
            assert_eq!(
                serde_json::to_value(&message).expect("message must serialize"),
                *value,
                "validMessages[{index}] is not in canonical serialized form"
            );
            message
        })
        .collect()
}

#[test]
fn canonical_messages_round_trip_and_cover_the_v1_surface() {
    let document = fixtures();
    assert_eq!(document.protocol, PROTOCOL_NAME);
    assert_eq!(document.protocol_version, PROTOCOL_VERSION);
    assert!(document.description.contains("not one executable dialogue"));

    let messages = decode_valid_messages(&document);
    assert_eq!(messages.len(), 10);
    assert!(matches!(messages[0], Message::ClientHello(_)));
    assert!(matches!(messages[1], Message::HostHello(_)));

    let Message::Snapshot(first) = &messages[2] else {
        panic!("third canonical message must be the initial snapshot");
    };
    assert_eq!(first.revision, 1);
    let Element::Layout(root_layout) = &first.root.element else {
        panic!("initial snapshot must have a layout root");
    };
    assert_eq!(root_layout.children.len(), 2);
    assert_eq!(
        root_layout.on_reorder.as_ref().unwrap().0,
        "reorder-sections"
    );

    let Element::Tabs(tabs) = &root_layout.children[0].element else {
        panic!("the first root child must be tabs");
    };
    assert_eq!(root_layout.children[0].id.as_ref().unwrap().0, "main-tabs");
    assert_eq!(tabs.on_select.as_ref().unwrap().0, "select-tab");
    assert_eq!(tabs.on_reorder.as_ref().unwrap().0, "reorder-tabs");
    assert_eq!(
        tabs.tab_ids
            .iter()
            .map(|id| id.as_str())
            .collect::<Vec<_>>(),
        ["overview", "details"]
    );
    assert_eq!(tabs.selected, Some(0));

    let Element::Layout(content_layout) = &root_layout.children[1].element else {
        panic!("the second root child must be the nested layout");
    };
    assert_eq!(content_layout.children.len(), 2);
    assert!(matches!(
        content_layout.children[0].element,
        Element::Paragraph(_)
    ));
    let Element::Canvas(canvas) = &content_layout.children[1].element else {
        panic!("the nested layout must include a canvas");
    };
    assert_eq!(canvas.layers.len(), 3, "explicit empty layers are portable");
    assert_eq!(canvas.labels.len(), 2);

    let primitive_types = canvas
        .layers
        .iter()
        .flat_map(|layer| layer.primitives.iter())
        .map(|primitive| match primitive {
            Primitive::Line(_) => "line",
            Primitive::Rectangle(_) => "rectangle",
            Primitive::Circle(_) => "circle",
            Primitive::Points(_) => "points",
            Primitive::Map(_) => "map",
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        primitive_types,
        BTreeSet::from(["circle", "line", "map", "points", "rectangle"])
    );

    let Message::Event(event) = &messages[3] else {
        panic!("fourth canonical message must be the semantic event");
    };
    assert_eq!(event.revision, 1);
    assert_eq!(event.node_id.0, "main-tabs");
    assert_eq!(event.action.0, "select-tab");
    assert_eq!(event.kind, EventKind::Select);
    assert_eq!(event.value, EventValue::Index(1));

    let Message::Snapshot(second) = &messages[4] else {
        panic!("fifth canonical message must be the updated snapshot");
    };
    assert_eq!(second.revision, 2);
    let Element::Layout(second_layout) = &second.root.element else {
        panic!("revision 2 must remain a complete layout snapshot");
    };
    let Element::Tabs(second_tabs) = &second_layout.children[0].element else {
        panic!("revision 2 must retain the interactive tabs");
    };
    assert_eq!(second_tabs.selected, Some(1));
    assert_eq!(second_layout.children.len(), 4);

    let Element::List(list) = &second_layout.children[2].element else {
        panic!("revision 2 must include the reorderable list");
    };
    assert_eq!(list.on_reorder.as_ref().unwrap().0, "reorder-tasks");
    assert_eq!(
        list.items
            .iter()
            .map(|item| item.id.as_ref().unwrap().as_str())
            .collect::<Vec<_>>(),
        ["task-a", "task-b"]
    );

    let Element::Table(table) = &second_layout.children[3].element else {
        panic!("revision 2 must include the reorderable table");
    };
    assert_eq!(table.on_reorder_rows.as_ref().unwrap().0, "reorder-rows");
    assert_eq!(
        table.on_reorder_columns.as_ref().unwrap().0,
        "reorder-columns"
    );
    assert_eq!(
        table
            .rows
            .iter()
            .map(|row| row.id.as_ref().unwrap().as_str())
            .collect::<Vec<_>>(),
        ["row-a", "row-b"]
    );
    assert_eq!(
        table
            .column_ids
            .iter()
            .map(|id| id.as_str())
            .collect::<Vec<_>>(),
        ["task", "owner"]
    );

    let expected_reorders = [
        (
            "root",
            "reorder-sections",
            &["task-list", "main-tabs", "selection-result", "task-table"][..],
        ),
        ("main-tabs", "reorder-tabs", &["details", "overview"][..]),
        ("task-list", "reorder-tasks", &["task-b", "task-a"][..]),
        ("task-table", "reorder-rows", &["row-b", "row-a"][..]),
        ("task-table", "reorder-columns", &["owner", "task"][..]),
    ];
    for (message, (node_id, action, order)) in messages[5..].iter().zip(expected_reorders) {
        let Message::Event(event) = message else {
            panic!("canonical reorder messages must be events");
        };
        assert_eq!(event.revision, 2);
        assert_eq!(event.node_id.0, node_id);
        assert_eq!(event.action.0, action);
        assert_eq!(event.kind, EventKind::Change);
        assert_eq!(
            event.value,
            EventValue::TextList(order.iter().map(|id| (*id).to_owned()).collect())
        );
    }
}

#[test]
fn closed_wire_values_round_trip_in_declaration_order() {
    let raw_document: Value =
        serde_json::from_str(FIXTURES_JSON).expect("fixture document must be valid JSON");
    let document = fixtures();
    let matrix = &document.wire_value_matrix;

    assert_eq!(
        serde_json::to_value(matrix).expect("wire value matrix must serialize"),
        raw_document["wireValueMatrix"],
        "the matrix must remain in canonical cross-language wire form"
    );

    assert_eq!(
        matrix.event_kinds,
        vec![
            EventKind::Activate,
            EventKind::Select,
            EventKind::Change,
            EventKind::Submit,
            EventKind::Cancel,
        ]
    );
    assert_eq!(
        matrix.event_values,
        vec![
            EventValue::None,
            EventValue::Bool(true),
            EventValue::Index(2),
            EventValue::Integer(-7),
            EventValue::Number(3.25),
            EventValue::Text("héllo 👋".to_owned()),
            EventValue::TextList(vec!["alpha".to_owned(), "βeta".to_owned()]),
        ]
    );
    assert_eq!(
        matrix.constraints,
        vec![
            Constraint::Min(1),
            Constraint::Max(99),
            Constraint::Length(7),
            Constraint::Percentage(42),
            Constraint::Ratio(16, 9),
            Constraint::Fill(2),
        ]
    );
    assert_eq!(
        matrix.flex,
        vec![
            Flex::Legacy,
            Flex::Start,
            Flex::End,
            Flex::Center,
            Flex::SpaceBetween,
            Flex::SpaceAround,
        ]
    );
    assert_eq!(
        matrix.directions,
        vec![Direction::Horizontal, Direction::Vertical]
    );
    assert_eq!(
        matrix.list_directions,
        vec![ListDirection::TopToBottom, ListDirection::BottomToTop]
    );
    assert_eq!(
        matrix.highlight_spacings,
        vec![
            HighlightSpacing::Always,
            HighlightSpacing::WhenSelected,
            HighlightSpacing::Never,
        ]
    );
    assert_eq!(matrix.spacing, vec![Spacing::Space(3), Spacing::Overlap(2)]);
    assert_eq!(
        matrix.alignments,
        vec![Alignment::Left, Alignment::Center, Alignment::Right]
    );
    assert_eq!(
        matrix.colors,
        vec![
            Color::Reset,
            Color::Black,
            Color::Red,
            Color::Green,
            Color::Yellow,
            Color::Blue,
            Color::Magenta,
            Color::Cyan,
            Color::Gray,
            Color::DarkGray,
            Color::LightRed,
            Color::LightGreen,
            Color::LightYellow,
            Color::LightBlue,
            Color::LightMagenta,
            Color::LightCyan,
            Color::White,
            Color::Rgb {
                red: 12,
                green: 34,
                blue: 56,
            },
            Color::Indexed { index: 201 },
        ]
    );
    assert_eq!(
        matrix.modifiers,
        vec![
            Modifier::Bold,
            Modifier::Dim,
            Modifier::Italic,
            Modifier::Underlined,
            Modifier::SlowBlink,
            Modifier::RapidBlink,
            Modifier::Reversed,
            Modifier::Hidden,
            Modifier::CrossedOut,
        ]
    );
    assert_eq!(
        matrix.border_types,
        vec![
            BorderType::Plain,
            BorderType::Rounded,
            BorderType::Double,
            BorderType::Thick,
            BorderType::QuadrantInside,
            BorderType::QuadrantOutside,
        ]
    );
    assert_eq!(
        matrix.markers,
        vec![
            Marker::Dot,
            Marker::Block,
            Marker::Bar,
            Marker::Braille,
            Marker::HalfBlock,
        ]
    );
    assert_eq!(
        matrix.map_resolutions,
        vec![MapResolution::Low, MapResolution::High]
    );
    assert_eq!(
        matrix.render_modes,
        vec![RenderMode::Terminal, RenderMode::Structured]
    );

    assert_eq!(matrix.event_kinds.len(), 5);
    assert_eq!(matrix.event_values.len(), 7);
    assert_eq!(matrix.constraints.len(), 6);
    assert_eq!(matrix.flex.len(), 6);
    assert_eq!(matrix.directions.len(), 2);
    assert_eq!(matrix.list_directions.len(), 2);
    assert_eq!(matrix.highlight_spacings.len(), 3);
    assert_eq!(matrix.spacing.len(), 2);
    assert_eq!(matrix.alignments.len(), 3);
    assert_eq!(matrix.colors.len(), 19);
    assert_eq!(matrix.modifiers.len(), 9);
    assert_eq!(matrix.border_types.len(), 6);
    assert_eq!(matrix.markers.len(), 5);
    assert_eq!(matrix.map_resolutions.len(), 2);
    assert_eq!(matrix.render_modes.len(), 2);
}

#[test]
fn compatibility_cases_fail_at_the_intended_boundary() {
    let document = fixtures();
    assert_eq!(document.compatibility_cases.len(), 2);
    assert!(document
        .valid_messages
        .iter()
        .all(|message| message["protocolVersion"] == PROTOCOL_VERSION));

    for case in document.compatibility_cases {
        match (case.name.as_str(), case.expected_error.as_str()) {
            ("too-new-version", "unsupportedVersion") => {
                // The v1 body shape can still be decoded, but negotiation must
                // reject it before it is processed.
                let message: Message = serde_json::from_value(case.message.clone())
                    .expect("the too-new fixture deliberately uses a known body shape");
                assert!(matches!(
                    message.validate(),
                    Err(ProtocolError::UnsupportedVersion {
                        supported: PROTOCOL_VERSION,
                        received: 2
                    })
                ));

                let frame = serde_json::to_vec(&case.message).unwrap();
                let mut reader = Cursor::new(frame);
                assert!(matches!(
                    read_message(&mut reader),
                    Err(ProtocolError::UnsupportedVersion {
                        supported: PROTOCOL_VERSION,
                        received: 2
                    })
                ));
            }
            ("unknown-node", "unknownDiscriminator") => {
                assert!(
                    serde_json::from_value::<Message>(case.message.clone()).is_err(),
                    "v1 must not silently decode an unknown node discriminator"
                );

                let frame = serde_json::to_vec(&case.message).unwrap();
                let mut reader = Cursor::new(frame);
                assert!(matches!(
                    read_message(&mut reader),
                    Err(ProtocolError::Json(_))
                ));
            }
            (name, expected) => panic!("unhandled compatibility fixture {name:?}: {expected:?}"),
        }
    }

    let unknown_message = json!({
        "type": "futureMessage",
        "protocol": PROTOCOL_NAME,
        "protocolVersion": PROTOCOL_VERSION
    });
    assert!(serde_json::from_value::<Message>(unknown_message).is_err());
}

#[test]
fn canonical_ndjson_stream_decodes_one_validated_message_per_line() {
    let document = fixtures();
    let expected = decode_valid_messages(&document);
    let mut reader = Cursor::new(STREAM_NDJSON);
    let mut actual = Vec::new();

    while let Some(message) = read_message(&mut reader).expect("canonical stream must decode") {
        actual.push(message);
    }

    assert_eq!(actual, expected);
}

#[test]
fn schema_has_closed_discriminators_and_additive_objects() {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect("schema must be valid JSON");
    assert_eq!(
        schema["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(schema["$ref"], "#/$defs/message");

    assert_discriminator_union(
        &schema,
        "message",
        &["clientHello", "event", "hostHello", "snapshot"],
    );
    assert_discriminator_union(
        &schema,
        "node",
        &["canvas", "layout", "list", "paragraph", "table", "tabs"],
    );
    assert_discriminator_union(
        &schema,
        "primitive",
        &["circle", "line", "map", "points", "rectangle"],
    );

    for definition in [
        "clientHello",
        "hostHello",
        "snapshot",
        "event",
        "layoutNode",
        "paragraphNode",
        "tabsNode",
        "listNode",
        "listItem",
        "tableNode",
        "tableRow",
        "tableCell",
        "canvasNode",
        "canvasLine",
        "rectangle",
        "circle",
        "points",
        "map",
    ] {
        assert_eq!(
            schema["$defs"][definition]["additionalProperties"],
            Value::Bool(true),
            "{definition} must permit additive unknown properties"
        );
    }

    assert_eq!(
        schema["$defs"]["protocolHeader"]["properties"]["protocolVersion"]["const"],
        PROTOCOL_VERSION
    );
    assert_eq!(
        schema["$defs"]["listDirection"]["enum"],
        json!(["topToBottom", "bottomToTop"])
    );
    assert_eq!(
        schema["$defs"]["highlightSpacing"]["enum"],
        json!(["always", "whenSelected", "never"])
    );
    for (definition, field) in [
        ("layoutNode", "onReorder"),
        ("tabsNode", "onReorder"),
        ("listNode", "onReorder"),
        ("tableNode", "onReorderRows"),
        ("tableNode", "onReorderColumns"),
    ] {
        assert!(
            schema["$defs"][definition]["properties"][field].is_object(),
            "{definition}.{field} must declare its direct semantic action"
        );
    }
    assert!(schema["$defs"]["protocolHeader"]["properties"]
        .get("version")
        .is_none());
}

fn assert_discriminator_union(schema: &Value, union: &str, expected: &[&str]) {
    let branches = schema["$defs"][union]["oneOf"]
        .as_array()
        .unwrap_or_else(|| panic!("$defs.{union}.oneOf must be an array"));
    let mut actual = branches
        .iter()
        .map(|branch| {
            let reference = branch["$ref"]
                .as_str()
                .unwrap_or_else(|| panic!("$defs.{union} branches must use $ref"));
            let definition = reference
                .strip_prefix("#/$defs/")
                .unwrap_or_else(|| panic!("unexpected local reference {reference:?}"));
            schema["$defs"][definition]["properties"]["type"]["const"]
                .as_str()
                .unwrap_or_else(|| panic!("{definition} must have a constant type discriminator"))
                .to_owned()
        })
        .collect::<Vec<_>>();
    actual.sort();
    assert_eq!(actual, expected);
}

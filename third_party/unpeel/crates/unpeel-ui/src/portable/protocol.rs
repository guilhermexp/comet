//! Versioned, newline-delimited transport for portable Unpeel App views.
//!
//! Structured mode reserves stdout for this protocol: every frame is one
//! compact JSON object followed by `\n`. App diagnostics belong on stderr.

use std::ffi::OsStr;
use std::fmt;
use std::io::{self, BufRead, Read, Write};

use serde::{Deserialize, Serialize};

use super::model::{ActionId, Node, NodeId, ValidationError};
use super::reorder::ItemId;

/// Stable protocol name sent in every independently replayable message.
pub const NAME: &str = "unpeel.ui";

/// The only protocol version this crate currently reads and writes.
pub const VERSION: u32 = 1;

/// Maximum size of one encoded JSON payload, excluding its NDJSON newline.
///
/// This is deliberately generous for canvas scenes while preventing an App
/// or renderer from growing memory without bound before sending a delimiter.
pub const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

/// Largest integer represented exactly by both 64-bit native clients and
/// JavaScript-based web renderers.
pub const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

/// Environment variable through which the Host requests structured mode.
pub const UI_MODE_ENV: &str = "UNPEEL_UI_MODE";

/// How an Unpeel App should present its view for this process invocation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RenderMode {
    /// Render through Ratatui in the process's terminal.
    #[default]
    Terminal,
    /// Exchange portable view snapshots and semantic events over stdio.
    Structured,
}

impl RenderMode {
    /// Detect the requested renderer. Unknown and missing values deliberately
    /// fall back to the terminal so an App remains standalone-first.
    pub fn detect() -> Self {
        Self::detect_from(std::env::var_os(UI_MODE_ENV).as_deref())
    }

    fn detect_from(value: Option<&OsStr>) -> Self {
        match value.and_then(OsStr::to_str).map(str::trim) {
            Some(value) if value.eq_ignore_ascii_case("structured") => Self::Structured,
            _ => Self::Terminal,
        }
    }
}

/// Human-facing identity advertised by the App process during negotiation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppMetadata {
    /// Stable reverse-domain (or otherwise globally unique) App identifier.
    pub id: String,
    /// Display name chosen by the App.
    pub name: String,
    /// App binary/package version, independent of the UI protocol version.
    pub version: String,
    /// Optional short description for clients that have room to show it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl AppMetadata {
    pub fn new(id: impl Into<String>, name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            version: version.into(),
            description: None,
        }
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// The App process introduces itself before emitting view snapshots.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientHello {
    pub protocol: String,
    pub protocol_version: u32,
    pub app: AppMetadata,
}

impl ClientHello {
    pub fn new(app: AppMetadata) -> Self {
        Self {
            protocol: NAME.to_string(),
            protocol_version: VERSION,
            app,
        }
    }
}

/// The Unpeel Host acknowledges the negotiated renderer and version.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostHello {
    pub protocol: String,
    pub protocol_version: u32,
    pub render_mode: RenderMode,
}

impl HostHello {
    pub fn new(render_mode: RenderMode) -> Self {
        Self {
            protocol: NAME.to_string(),
            protocol_version: VERSION,
            render_mode,
        }
    }
}

/// A complete portable view at a monotonically increasing revision.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub protocol: String,
    pub protocol_version: u32,
    pub revision: u64,
    pub root: Node,
}

impl Snapshot {
    pub fn new(revision: u64, root: Node) -> Self {
        Self {
            protocol: NAME.to_string(),
            protocol_version: VERSION,
            revision,
            root,
        }
    }
}

/// Semantic intent from a native or terminal renderer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EventKind {
    Activate,
    Select,
    Change,
    Submit,
    Cancel,
}

/// Typed event data. The adjacent tag keeps values unambiguous for Swift and
/// web decoders without exposing arbitrary JSON as an App-facing API.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum EventValue {
    #[default]
    None,
    Bool(bool),
    Index(u64),
    Integer(i64),
    Number(f64),
    Text(String),
    TextList(Vec<String>),
}

/// One action addressed to the node and action id declared by the App.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionEvent {
    pub protocol: String,
    pub protocol_version: u32,
    /// Snapshot revision against which the user performed the action.
    pub revision: u64,
    pub node_id: NodeId,
    pub action: ActionId,
    pub kind: EventKind,
    pub value: EventValue,
}

impl ActionEvent {
    pub fn new(
        revision: u64,
        node_id: impl Into<NodeId>,
        action: impl Into<ActionId>,
        kind: EventKind,
        value: EventValue,
    ) -> Self {
        Self {
            protocol: NAME.to_string(),
            protocol_version: VERSION,
            revision,
            node_id: node_id.into(),
            action: action.into(),
            kind,
            value,
        }
    }

    /// Build the canonical semantic event for reordering a collection.
    ///
    /// The value is the complete logical order of stable item ids. Renderers
    /// never send pointer coordinates or terminal key codes across the wire.
    pub fn reorder<I, T>(
        revision: u64,
        node_id: impl Into<NodeId>,
        action: impl Into<ActionId>,
        order: I,
    ) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<ItemId>,
    {
        Self::new(
            revision,
            node_id,
            action,
            EventKind::Change,
            EventValue::TextList(
                order
                    .into_iter()
                    .map(|item_id| item_id.into().into_string())
                    .collect(),
            ),
        )
    }

    /// Decode the complete stable-ID order carried by a reorder event.
    ///
    /// Callers must still match `node_id` and `action` against the action they
    /// declared in the current snapshot before applying the result.
    pub fn reorder_ids(&self) -> Option<Vec<ItemId>> {
        if self.kind != EventKind::Change {
            return None;
        }
        let EventValue::TextList(order) = &self.value else {
            return None;
        };
        Some(order.iter().cloned().map(ItemId::from).collect())
    }
}

/// Every NDJSON frame has a visible camelCase `type` discriminator.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Message {
    ClientHello(ClientHello),
    HostHello(HostHello),
    Snapshot(Snapshot),
    Event(ActionEvent),
}

impl Message {
    /// Reject a message from another protocol or an unsupported revision even
    /// if its remaining JSON happens to match one of our message shapes.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        let (protocol, protocol_version) = match self {
            Self::ClientHello(message) => (&message.protocol, message.protocol_version),
            Self::HostHello(message) => (&message.protocol, message.protocol_version),
            Self::Snapshot(message) => (&message.protocol, message.protocol_version),
            Self::Event(message) => (&message.protocol, message.protocol_version),
        };
        if protocol != NAME {
            return Err(ProtocolError::UnexpectedProtocol {
                expected: NAME,
                received: protocol.clone(),
            });
        }
        if protocol_version != VERSION {
            return Err(ProtocolError::UnsupportedVersion {
                supported: VERSION,
                received: protocol_version,
            });
        }
        match self {
            Self::ClientHello(message) => {
                for (field, value) in [
                    ("app.id", message.app.id.as_str()),
                    ("app.name", message.app.name.as_str()),
                    ("app.version", message.app.version.as_str()),
                ] {
                    if value.trim().is_empty() {
                        return Err(ProtocolError::InvalidMessage(format!(
                            "clientHello {field} must not be empty"
                        )));
                    }
                }
            }
            Self::Snapshot(message) => {
                validate_revision(message.revision)?;
                message
                    .root
                    .validate()
                    .map_err(ProtocolError::InvalidView)?;
            }
            Self::Event(message) => {
                validate_revision(message.revision)?;
                if message.node_id.0.trim().is_empty() {
                    return Err(ProtocolError::InvalidMessage(
                        "event nodeId must not be empty".to_string(),
                    ));
                }
                if message.action.0.trim().is_empty() {
                    return Err(ProtocolError::InvalidMessage(
                        "event action must not be empty".to_string(),
                    ));
                }
                if matches!(message.value, EventValue::Number(value) if !value.is_finite()) {
                    return Err(ProtocolError::InvalidMessage(
                        "event number must be finite".to_string(),
                    ));
                }
                if matches!(message.value, EventValue::Index(value) if value > MAX_SAFE_INTEGER) {
                    return Err(ProtocolError::InvalidMessage(format!(
                        "event index exceeds the cross-platform safe integer {MAX_SAFE_INTEGER}"
                    )));
                }
                if matches!(
                    message.value,
                    EventValue::Integer(value)
                        if value < -(MAX_SAFE_INTEGER as i64)
                            || value > MAX_SAFE_INTEGER as i64
                ) {
                    return Err(ProtocolError::InvalidMessage(format!(
                        "event integer exceeds the cross-platform safe range ±{MAX_SAFE_INTEGER}"
                    )));
                }
            }
            Self::HostHello(_) => {}
        }
        Ok(())
    }
}

fn validate_revision(revision: u64) -> Result<(), ProtocolError> {
    if revision > MAX_SAFE_INTEGER {
        Err(ProtocolError::InvalidMessage(format!(
            "revision exceeds the cross-platform safe integer {MAX_SAFE_INTEGER}"
        )))
    } else {
        Ok(())
    }
}

impl From<ClientHello> for Message {
    fn from(value: ClientHello) -> Self {
        Self::ClientHello(value)
    }
}

impl From<HostHello> for Message {
    fn from(value: HostHello) -> Self {
        Self::HostHello(value)
    }
}

impl From<Snapshot> for Message {
    fn from(value: Snapshot) -> Self {
        Self::Snapshot(value)
    }
}

impl From<ActionEvent> for Message {
    fn from(value: ActionEvent) -> Self {
        Self::Event(value)
    }
}

/// Errors raised while decoding, validating, or writing protocol frames.
#[derive(Debug)]
pub enum ProtocolError {
    Io(io::Error),
    Json(serde_json::Error),
    EmptyFrame,
    FrameTooLarge {
        max_bytes: usize,
    },
    InvalidView(ValidationError),
    InvalidMessage(String),
    UnexpectedProtocol {
        expected: &'static str,
        received: String,
    },
    UnsupportedVersion {
        supported: u32,
        received: u32,
    },
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "UI protocol I/O error: {error}"),
            Self::Json(error) => write!(formatter, "invalid UI protocol JSON: {error}"),
            Self::EmptyFrame => formatter.write_str("UI protocol frame is empty"),
            Self::FrameTooLarge { max_bytes } => write!(
                formatter,
                "UI protocol frame exceeds the {max_bytes}-byte limit"
            ),
            Self::InvalidView(error) => write!(formatter, "invalid portable view: {error}"),
            Self::InvalidMessage(message) => {
                write!(formatter, "invalid UI protocol message: {message}")
            }
            Self::UnexpectedProtocol { expected, received } => write!(
                formatter,
                "unexpected UI protocol {received:?}; expected {expected:?}"
            ),
            Self::UnsupportedVersion {
                supported,
                received,
            } => write!(
                formatter,
                "unsupported UI protocol version {received}; this client supports {supported}"
            ),
        }
    }
}

impl std::error::Error for ProtocolError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::InvalidView(error) => Some(error),
            Self::EmptyFrame
            | Self::FrameTooLarge { .. }
            | Self::InvalidMessage(_)
            | Self::UnexpectedProtocol { .. }
            | Self::UnsupportedVersion { .. } => None,
        }
    }
}

impl From<io::Error> for ProtocolError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for ProtocolError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

/// Read and validate one newline-delimited message. `None` is a clean EOF.
pub fn read_message<R: BufRead>(reader: &mut R) -> Result<Option<Message>, ProtocolError> {
    let mut frame = Vec::new();
    // Two extra bytes allow a maximum-sized payload followed by CRLF.
    let mut limited = reader.take(MAX_FRAME_BYTES as u64 + 2);
    if limited.read_until(b'\n', &mut frame)? == 0 {
        return Ok(None);
    }
    if frame.ends_with(b"\n") {
        frame.pop();
        if frame.ends_with(b"\r") {
            frame.pop();
        }
    }
    if frame.len() > MAX_FRAME_BYTES {
        return Err(ProtocolError::FrameTooLarge {
            max_bytes: MAX_FRAME_BYTES,
        });
    }
    if frame.iter().all(u8::is_ascii_whitespace) {
        return Err(ProtocolError::EmptyFrame);
    }

    // Inspect the stable envelope before decoding the tagged body. A future
    // version can add message or node variants unknown to this crate and
    // still produce UnsupportedVersion instead of a misleading JSON error.
    let header: MessageHeader = serde_json::from_slice(&frame)?;
    validate_header(&header.protocol, header.protocol_version)?;

    let message: Message = serde_json::from_slice(&frame)?;
    message.validate()?;
    Ok(Some(message))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MessageHeader {
    protocol: String,
    protocol_version: u32,
}

fn validate_header(protocol: &str, protocol_version: u32) -> Result<(), ProtocolError> {
    if protocol != NAME {
        return Err(ProtocolError::UnexpectedProtocol {
            expected: NAME,
            received: protocol.to_string(),
        });
    }
    if protocol_version != VERSION {
        return Err(ProtocolError::UnsupportedVersion {
            supported: VERSION,
            received: protocol_version,
        });
    }
    Ok(())
}

/// Write one compact JSON frame and flush it for interactive stdio use.
pub fn write_message<W: Write>(writer: &mut W, message: &Message) -> Result<(), ProtocolError> {
    message.validate()?;
    let mut frame = serde_json::to_vec(message)?;
    if frame.len() > MAX_FRAME_BYTES {
        return Err(ProtocolError::FrameTooLarge {
            max_bytes: MAX_FRAME_BYTES,
        });
    }
    frame.push(b'\n');
    writer.write_all(&frame)?;
    writer.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::portable::widgets::Tabs;

    fn hello() -> Message {
        ClientHello::new(
            AppMetadata::new("com.unpeel.todos", "Todos", "0.1.0")
                .description("A portable todo list"),
        )
        .into()
    }

    #[test]
    fn render_mode_is_standalone_first() {
        assert_eq!(RenderMode::detect_from(None), RenderMode::Terminal);
        assert_eq!(
            RenderMode::detect_from(Some(OsStr::new("terminal"))),
            RenderMode::Terminal
        );
        assert_eq!(
            RenderMode::detect_from(Some(OsStr::new("structured"))),
            RenderMode::Structured
        );
        assert_eq!(
            RenderMode::detect_from(Some(OsStr::new(" STRUCTURED "))),
            RenderMode::Structured
        );
        assert_eq!(
            RenderMode::detect_from(Some(OsStr::new("future-mode"))),
            RenderMode::Terminal
        );
    }

    #[test]
    fn message_round_trips_with_camel_case_type_and_fields() {
        let message = hello();
        let encoded = serde_json::to_string(&message).unwrap();
        assert!(encoded.contains(r#""type":"clientHello""#), "{encoded}");
        assert!(encoded.contains(r#""protocol":"unpeel.ui""#), "{encoded}");
        assert!(encoded.contains(r#""protocolVersion":1"#), "{encoded}");
        assert!(encoded.contains(r#""description":"A portable todo list""#));

        let decoded: Message = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, message);
    }

    #[test]
    fn snapshots_and_typed_actions_round_trip() {
        let root: Node = Tabs::new(["Overview", "Activity"])
            .id("main-tabs")
            .select(1)
            .on_select("select-tab")
            .into();
        let snapshot = Message::Snapshot(Snapshot::new(7, root));
        let snapshot_json = serde_json::to_string(&snapshot).unwrap();
        assert!(snapshot_json.contains(r#""type":"snapshot""#));
        assert!(snapshot_json.contains(r#""id":"main-tabs""#));
        assert_eq!(
            serde_json::from_str::<Message>(&snapshot_json).unwrap(),
            snapshot
        );

        let event = Message::Event(ActionEvent::new(
            7,
            "main-tabs",
            "select-tab",
            EventKind::Select,
            EventValue::Index(0),
        ));
        let event_json = serde_json::to_string(&event).unwrap();
        assert!(event_json.contains(r#""nodeId":"main-tabs""#));
        assert!(event_json.contains(r#""kind":"select""#));
        assert!(event_json.contains(r#""value":{"type":"index","value":0}"#));
        assert_eq!(serde_json::from_str::<Message>(&event_json).unwrap(), event);
    }

    #[test]
    fn reorder_event_uses_the_change_text_list_contract() {
        let event = ActionEvent::reorder(7, "tasks", "reorder-tasks", ["c", "a", "b"]);
        assert_eq!(event.revision, 7);
        assert_eq!(event.node_id.as_str(), "tasks");
        assert_eq!(event.action.as_str(), "reorder-tasks");
        assert_eq!(event.kind, EventKind::Change);
        assert_eq!(
            event.value,
            EventValue::TextList(vec!["c".into(), "a".into(), "b".into()])
        );
        assert_eq!(
            event.reorder_ids(),
            Some(vec!["c".into(), "a".into(), "b".into()])
        );
    }

    #[test]
    fn helpers_frame_multiple_messages_one_per_line() {
        let messages = [
            hello(),
            Message::HostHello(HostHello::new(RenderMode::Structured)),
        ];
        let mut bytes = Vec::new();
        for message in &messages {
            write_message(&mut bytes, message).unwrap();
        }
        assert_eq!(bytes.iter().filter(|byte| **byte == b'\n').count(), 2);

        let mut reader = Cursor::new(bytes);
        assert_eq!(
            read_message(&mut reader).unwrap(),
            Some(messages[0].clone())
        );
        assert_eq!(
            read_message(&mut reader).unwrap(),
            Some(messages[1].clone())
        );
        assert_eq!(read_message(&mut reader).unwrap(), None);
    }

    #[test]
    fn reader_reports_empty_invalid_and_incompatible_frames() {
        let mut empty = Cursor::new(b"\n");
        assert!(matches!(
            read_message(&mut empty),
            Err(ProtocolError::EmptyFrame)
        ));

        let mut invalid = Cursor::new(b"not-json\n");
        assert!(matches!(
            read_message(&mut invalid),
            Err(ProtocolError::Json(_))
        ));

        let frame = serde_json::to_string(&hello())
            .unwrap()
            .replace(r#""protocolVersion":1"#, r#""protocolVersion":2"#)
            + "\n";
        let mut incompatible = Cursor::new(frame.into_bytes());
        assert!(matches!(
            read_message(&mut incompatible),
            Err(ProtocolError::UnsupportedVersion {
                supported: VERSION,
                received: 2
            })
        ));

        let mut future =
            Cursor::new(br#"{"type":"futureMessage","protocol":"unpeel.ui","protocolVersion":2}"#);
        assert!(matches!(
            read_message(&mut future),
            Err(ProtocolError::UnsupportedVersion {
                supported: VERSION,
                received: 2
            })
        ));

        let mut oversized = Cursor::new(vec![b'x'; MAX_FRAME_BYTES + 1]);
        assert!(matches!(
            read_message(&mut oversized),
            Err(ProtocolError::FrameTooLarge {
                max_bytes: MAX_FRAME_BYTES
            })
        ));
    }

    #[test]
    fn writer_rejects_invalid_views_and_events_before_emitting_bytes() {
        let root: Node = Tabs::new(["Overview"]).on_select("select-tab").into();
        let mut output = Vec::new();
        assert!(matches!(
            write_message(&mut output, &Message::Snapshot(Snapshot::new(1, root))),
            Err(ProtocolError::InvalidView(_))
        ));
        assert!(output.is_empty());

        let event = Message::Event(ActionEvent::new(
            1,
            " ",
            "select-tab",
            EventKind::Select,
            EventValue::Index(0),
        ));
        assert!(matches!(
            write_message(&mut output, &event),
            Err(ProtocolError::InvalidMessage(_))
        ));
        assert!(output.is_empty());

        let unsafe_integer = Message::Event(ActionEvent::new(
            1,
            "field",
            "change",
            EventKind::Change,
            EventValue::Integer(MAX_SAFE_INTEGER as i64 + 1),
        ));
        assert!(matches!(
            write_message(&mut output, &unsafe_integer),
            Err(ProtocolError::InvalidMessage(_))
        ));
        assert!(output.is_empty());

        let oversized = Message::Event(ActionEvent::new(
            1,
            "field",
            "change",
            EventKind::Change,
            EventValue::Text("x".repeat(MAX_FRAME_BYTES)),
        ));
        assert!(matches!(
            write_message(&mut output, &oversized),
            Err(ProtocolError::FrameTooLarge {
                max_bytes: MAX_FRAME_BYTES
            })
        ));
        assert!(output.is_empty());
    }

    #[test]
    fn explicit_null_optionals_decode_and_canonical_output_omits_them() {
        let frame = br#"{"type":"snapshot","protocol":"unpeel.ui","protocolVersion":1,"revision":1,"root":{"id":null,"type":"tabs","selected":null,"divider":{"content":"|"},"paddingLeft":{"spans":[]},"paddingRight":{"spans":[]},"onSelect":null}}"#;
        let mut reader = Cursor::new(frame);
        let message = read_message(&mut reader).unwrap().unwrap();
        let encoded = serde_json::to_string(&message).unwrap();
        assert!(!encoded.contains(r#""id""#), "{encoded}");
        assert!(!encoded.contains(r#""selected""#), "{encoded}");
        assert!(!encoded.contains(r#""onSelect""#), "{encoded}");
    }
}

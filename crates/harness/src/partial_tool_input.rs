use zeron_proto::ToolCall;

pub(crate) const PARTIAL_PREVIEW_BODY_MAX_BYTES: usize = 8 * 1024;
pub(crate) const PARTIAL_REFRESH_BYTES: usize = 16 * 1024;
const PARTIAL_PATH_MAX_BYTES: usize = 4 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileToolKind {
    Write,
    Edit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileField {
    Path,
    Content,
    Old,
    New,
}

#[derive(Debug, Clone)]
enum ObjectPhase {
    Key,
    Colon(Option<FileField>),
    Value(Option<FileField>),
    AfterValue,
}

#[derive(Debug, Clone, Copy)]
enum StringRole {
    Key,
    Value(Option<FileField>),
    Ignore,
}

#[derive(Debug, Clone, Copy)]
enum EscapeState {
    Normal,
    Escaped,
    Unicode { value: u32, digits: u8 },
}

#[derive(Debug, Clone)]
struct ActiveString {
    role: StringRole,
    escape: EscapeState,
    key: String,
    pending_high_surrogate: Option<u16>,
}

#[derive(Debug, Clone)]
struct IncrementalFileFields {
    depth: usize,
    phase: ObjectPhase,
    active: Option<ActiveString>,
    path: Option<String>,
    content: Option<String>,
    old_string: Option<String>,
    new_string: Option<String>,
}

impl Default for IncrementalFileFields {
    fn default() -> Self {
        Self {
            depth: 0,
            phase: ObjectPhase::Key,
            active: None,
            path: None,
            content: None,
            old_string: None,
            new_string: None,
        }
    }
}

/// Incremental, bounded decoder for progressive Write/Edit JSON input.
/// Every incoming character is consumed once; only the path and bounded body
/// tails survive between chunks.
#[derive(Debug, Clone)]
pub(crate) struct PartialFileToolInput {
    kind: FileToolKind,
    fields: IncrementalFileFields,
    bytes_since_emit: usize,
    emission_count: u32,
    last_emitted: Option<ToolCall>,
}

impl PartialFileToolInput {
    pub(crate) fn new(tool_name: &str) -> Option<Self> {
        let kind = match tool_name.to_ascii_lowercase().as_str() {
            "write" => FileToolKind::Write,
            "edit" => FileToolKind::Edit,
            _ => return None,
        };
        Some(Self {
            kind,
            fields: IncrementalFileFields::default(),
            bytes_since_emit: 0,
            emission_count: 0,
            last_emitted: None,
        })
    }

    pub(crate) fn push(&mut self, delta: &str) -> Option<ToolCall> {
        self.bytes_since_emit = self.bytes_since_emit.saturating_add(delta.len());
        let had_body = self.has_body();
        self.fields.push(delta);
        let first = self.last_emitted.is_none();
        let body_started = !had_body && self.has_body();
        let first_semantic_followup =
            self.emission_count == 1 && (body_started || has_line_boundary(delta));
        if !first && !first_semantic_followup && self.bytes_since_emit < PARTIAL_REFRESH_BYTES {
            return None;
        }
        let call = self.preview_call()?;
        if self.last_emitted.as_ref() == Some(&call) {
            return None;
        }
        self.last_emitted = Some(call.clone());
        self.bytes_since_emit = 0;
        self.emission_count = self.emission_count.saturating_add(1);
        Some(call)
    }

    pub(crate) fn force_preview(&mut self) -> Option<ToolCall> {
        let call = self.preview_call()?;
        if self.last_emitted.as_ref() == Some(&call) {
            return None;
        }
        self.last_emitted = Some(call.clone());
        self.bytes_since_emit = 0;
        self.emission_count = self.emission_count.saturating_add(1);
        Some(call)
    }

    pub(crate) fn preview_call(&self) -> Option<ToolCall> {
        let path = self.fields.path.clone().filter(|path| !path.is_empty())?;
        Some(match self.kind {
            FileToolKind::Write => ToolCall::WriteFile {
                path,
                content: self.fields.content.clone(),
            },
            FileToolKind::Edit => ToolCall::EditFile {
                path,
                old_string: self.fields.old_string.clone(),
                new_string: self.fields.new_string.clone(),
            },
        })
    }

    fn has_body(&self) -> bool {
        match self.kind {
            FileToolKind::Write => self.fields.content.is_some(),
            FileToolKind::Edit => {
                self.fields.old_string.is_some() || self.fields.new_string.is_some()
            }
        }
    }
}

impl IncrementalFileFields {
    fn push(&mut self, delta: &str) {
        for character in delta.chars() {
            if self.active.is_some() {
                self.push_string_character(character);
                continue;
            }

            match character {
                '"' => {
                    let role = if self.depth == 1 {
                        match self.phase {
                            ObjectPhase::Key => StringRole::Key,
                            ObjectPhase::Value(field) => {
                                self.start_field(field);
                                StringRole::Value(field)
                            }
                            _ => StringRole::Ignore,
                        }
                    } else {
                        StringRole::Ignore
                    };
                    self.active = Some(ActiveString {
                        role,
                        escape: EscapeState::Normal,
                        key: String::new(),
                        pending_high_surrogate: None,
                    });
                }
                '{' | '[' => {
                    self.depth = self.depth.saturating_add(1);
                    if self.depth == 1 {
                        self.phase = ObjectPhase::Key;
                    }
                }
                '}' | ']' => {
                    let before = self.depth;
                    self.depth = self.depth.saturating_sub(1);
                    if before > 1 && self.depth == 1 {
                        self.phase = ObjectPhase::AfterValue;
                    }
                }
                ':' if self.depth == 1 => {
                    if let ObjectPhase::Colon(field) = self.phase {
                        self.phase = ObjectPhase::Value(field);
                    }
                }
                ',' if self.depth == 1 => self.phase = ObjectPhase::Key,
                c if self.depth == 1 && !c.is_whitespace() => {
                    if matches!(self.phase, ObjectPhase::Value(_)) {
                        self.phase = ObjectPhase::AfterValue;
                    }
                }
                _ => {}
            }
        }
        self.bound_body_tails();
    }

    fn push_string_character(&mut self, character: char) {
        let Some(escape) = self.active.as_ref().map(|active| active.escape) else {
            return;
        };
        match escape {
            EscapeState::Normal => match character {
                '"' => {
                    let active = self.active.take().expect("active string");
                    self.finish_string(active);
                }
                '\\' => {
                    if let Some(active) = self.active.as_mut() {
                        active.escape = EscapeState::Escaped;
                    }
                }
                other => {
                    if let Some(active) = self.active.as_mut() {
                        active.pending_high_surrogate = None;
                    }
                    self.append_active(other);
                }
            },
            EscapeState::Escaped => {
                let (decoded, next) = match character {
                    '"' => (Some('"'), EscapeState::Normal),
                    '\\' => (Some('\\'), EscapeState::Normal),
                    '/' => (Some('/'), EscapeState::Normal),
                    'b' => (Some('\u{0008}'), EscapeState::Normal),
                    'f' => (Some('\u{000c}'), EscapeState::Normal),
                    'n' => (Some('\n'), EscapeState::Normal),
                    'r' => (Some('\r'), EscapeState::Normal),
                    't' => (Some('\t'), EscapeState::Normal),
                    'u' => (
                        None,
                        EscapeState::Unicode {
                            value: 0,
                            digits: 0,
                        },
                    ),
                    _ => (None, EscapeState::Normal),
                };
                if let Some(decoded) = decoded {
                    if let Some(active) = self.active.as_mut() {
                        active.pending_high_surrogate = None;
                    }
                    self.append_active(decoded);
                }
                if !matches!(next, EscapeState::Unicode { .. })
                    && let Some(active) = self.active.as_mut()
                {
                    active.pending_high_surrogate = None;
                }
                if let Some(active) = self.active.as_mut() {
                    active.escape = next;
                }
            }
            EscapeState::Unicode { value, digits } => {
                let Some(hex) = character.to_digit(16) else {
                    if let Some(active) = self.active.as_mut() {
                        active.escape = EscapeState::Normal;
                    }
                    return;
                };
                let value = (value << 4) | hex;
                let digits = digits + 1;
                if digits == 4 {
                    let code = value as u16;
                    let decoded = if (0xD800..=0xDBFF).contains(&code) {
                        if let Some(active) = self.active.as_mut() {
                            active.pending_high_surrogate = Some(code);
                        }
                        None
                    } else if (0xDC00..=0xDFFF).contains(&code) {
                        self.active
                            .as_mut()
                            .and_then(|active| active.pending_high_surrogate.take())
                            .and_then(|high| {
                                let scalar = 0x10000
                                    + ((u32::from(high) - 0xD800) << 10)
                                    + (u32::from(code) - 0xDC00);
                                char::from_u32(scalar)
                            })
                    } else {
                        if let Some(active) = self.active.as_mut() {
                            active.pending_high_surrogate = None;
                        }
                        char::from_u32(value)
                    };
                    if let Some(decoded) = decoded {
                        self.append_active(decoded);
                    }
                    if let Some(active) = self.active.as_mut() {
                        active.escape = EscapeState::Normal;
                    }
                } else if let Some(active) = self.active.as_mut() {
                    active.escape = EscapeState::Unicode { value, digits };
                }
            }
        }
    }

    fn append_active(&mut self, character: char) {
        let Some(active) = self.active.as_mut() else {
            return;
        };
        match active.role {
            StringRole::Key => {
                if active.key.len() + character.len_utf8() <= 64 {
                    active.key.push(character);
                }
            }
            StringRole::Value(Some(field)) => self.append_field(field, character),
            StringRole::Value(None) | StringRole::Ignore => {}
        }
    }

    fn finish_string(&mut self, active: ActiveString) {
        self.phase = match active.role {
            StringRole::Key => ObjectPhase::Colon(field_alias(&active.key)),
            StringRole::Value(_) => ObjectPhase::AfterValue,
            StringRole::Ignore => self.phase.clone(),
        };
    }

    fn start_field(&mut self, field: Option<FileField>) {
        let slot = match field {
            Some(FileField::Path) => &mut self.path,
            Some(FileField::Content) => &mut self.content,
            Some(FileField::Old) => &mut self.old_string,
            Some(FileField::New) => &mut self.new_string,
            None => return,
        };
        slot.get_or_insert_with(String::new);
    }

    fn append_field(&mut self, field: FileField, character: char) {
        match field {
            FileField::Path => {
                let path = self.path.get_or_insert_with(String::new);
                if path.len() + character.len_utf8() <= PARTIAL_PATH_MAX_BYTES {
                    path.push(character);
                }
            }
            FileField::Content => self.content.get_or_insert_with(String::new).push(character),
            FileField::Old => self
                .old_string
                .get_or_insert_with(String::new)
                .push(character),
            FileField::New => self
                .new_string
                .get_or_insert_with(String::new)
                .push(character),
        }
    }

    fn bound_body_tails(&mut self) {
        for body in [
            &mut self.content,
            &mut self.old_string,
            &mut self.new_string,
        ]
        .into_iter()
        .flatten()
        {
            truncate_to_tail(body, PARTIAL_PREVIEW_BODY_MAX_BYTES);
        }
    }
}

fn field_alias(key: &str) -> Option<FileField> {
    match key {
        "path" | "file_path" | "filePath" => Some(FileField::Path),
        "content" => Some(FileField::Content),
        "old_string" | "oldText" => Some(FileField::Old),
        "new_string" | "newText" => Some(FileField::New),
        _ => None,
    }
}

fn truncate_to_tail(value: &mut String, max_bytes: usize) {
    if value.len() <= max_bytes {
        return;
    }
    let mut start = value.len() - max_bytes;
    while !value.is_char_boundary(start) {
        start += 1;
    }
    value.drain(..start);
}

fn has_line_boundary(delta: &str) -> bool {
    delta.contains('\n')
        || delta.contains('\r')
        || delta
            .as_bytes()
            .windows(2)
            .any(|pair| matches!(pair, b"\\n" | b"\\r"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_complete_and_unterminated_file_strings_without_general_json_repair() {
        let raw = r#"{"file_path":"src/a.rs","content":"line 1\nline 2"#;
        let mut parser = PartialFileToolInput::new("Write").unwrap();
        parser.push(raw);
        assert_eq!(
            parser.preview_call(),
            Some(ToolCall::WriteFile {
                path: "src/a.rs".into(),
                content: Some("line 1\nline 2".into()),
            })
        );
    }

    #[test]
    fn incomplete_escape_is_not_invented() {
        let mut parser = PartialFileToolInput::new("Write").unwrap();
        parser.push(r#"{"file_path":"a","content":"line\"#);
        assert!(matches!(
            parser.preview_call(),
            Some(ToolCall::WriteFile { content: Some(content), .. }) if content == "line"
        ));
    }

    #[test]
    fn ignores_alias_text_inside_another_json_string() {
        let raw = r#"{"note":"\"content\":\"wrong\"","file_path":"a","content":"right"#;

        let mut parser = PartialFileToolInput::new("Write").unwrap();
        parser.push(raw);
        assert!(matches!(
            parser.preview_call(),
            Some(ToolCall::WriteFile { content: Some(content), .. }) if content == "right"
        ));
    }

    #[test]
    fn decodes_complete_unicode_escapes_and_stops_before_incomplete_ones() {
        let mut parser = PartialFileToolInput::new("Write").unwrap();
        parser.push(r#"{"file_path":"a","content":"caf\u00e9 \u12"#);
        assert!(matches!(
            parser.preview_call(),
            Some(ToolCall::WriteFile { content: Some(content), .. }) if content == "café "
        ));
    }

    #[test]
    fn builds_only_file_calls_from_supported_names() {
        let mut parser = PartialFileToolInput::new("Write").unwrap();
        assert!(matches!(
            parser.push(r#"{"file_path":"a.txt","content":"hi"#),
            Some(ToolCall::WriteFile { path, content: Some(content) })
                if path == "a.txt" && content == "hi"
        ));
        assert!(PartialFileToolInput::new("Bash").is_none());
    }

    #[test]
    fn maps_edit_aliases_without_inventing_missing_fields() {
        let mut parser = PartialFileToolInput::new("edit").unwrap();
        parser.push(r#"{"path":"a.txt","oldText":"before","newText":"after"#);
        assert_eq!(
            parser.preview_call(),
            Some(ToolCall::EditFile {
                path: "a.txt".into(),
                old_string: Some("before".into()),
                new_string: Some("after".into()),
            })
        );
        let mut parser = PartialFileToolInput::new("Write").unwrap();
        parser.push(r#"{"content":"hi"#);
        assert_eq!(parser.preview_call(), None);
    }

    #[test]
    fn incremental_parser_decodes_each_chunk_and_bounds_live_body() {
        let mut parser = PartialFileToolInput::new("Write").expect("file tool");
        assert_eq!(
            parser.push(r#"{"file_path":"live.txt","content":"first"#),
            Some(ToolCall::WriteFile {
                path: "live.txt".into(),
                content: Some("first".into()),
            })
        );
        let tail = "€".repeat(PARTIAL_PREVIEW_BODY_MAX_BYTES);
        let _ = parser.push(&tail);

        let ToolCall::WriteFile {
            content: Some(content),
            ..
        } = parser.preview_call().expect("preview")
        else {
            panic!("write preview")
        };
        assert!(content.len() <= PARTIAL_PREVIEW_BODY_MAX_BYTES);
    }

    #[test]
    fn incremental_parser_combines_surrogate_pair_across_chunks() {
        let mut parser = PartialFileToolInput::new("Write").unwrap();
        parser.push(r#"{"file_path":"emoji.txt","content":"smile \uD83D"#);
        parser.push(r#"\uDE00"#);

        assert!(matches!(
            parser.preview_call(),
            Some(ToolCall::WriteFile { content: Some(content), .. }) if content == "smile 😀"
        ));
    }

    #[test]
    fn body_start_refreshes_a_path_only_preview_before_size_threshold() {
        let mut parser = PartialFileToolInput::new("Write").unwrap();
        assert!(matches!(
            parser.push("{\"file_path\":\"small.txt\""),
            Some(ToolCall::WriteFile { content: None, .. })
        ));
        assert_eq!(
            parser.push(r#", "content":"small body"#),
            Some(ToolCall::WriteFile {
                path: "small.txt".into(),
                content: Some("small body".into()),
            })
        );
    }
}

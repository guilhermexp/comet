use zeron_proto::ToolCall;

const MAX_PARTIAL_TOOL_INPUT_BYTES: usize = 1024 * 1024;

pub(crate) fn cap_partial_json(mut raw: String) -> String {
    if raw.len() <= MAX_PARTIAL_TOOL_INPUT_BYTES {
        return raw;
    }
    let mut end = MAX_PARTIAL_TOOL_INPUT_BYTES;
    while !raw.is_char_boundary(end) {
        end -= 1;
    }
    raw.truncate(end);
    raw
}

/// Decode a named JSON string from a possibly incomplete tool-input object.
///
/// This deliberately recognizes only string keys followed by string values;
/// it is not JSON repair. A trailing, incomplete escape is omitted rather
/// than guessed, which keeps every returned character grounded in bytes that
/// have already arrived from the runtime.
pub(crate) fn partial_json_string_field(raw: &str, aliases: &[&str]) -> Option<String> {
    let raw = capped_input(raw);
    let bytes = raw.as_bytes();
    let mut cursor = 0;

    while cursor < bytes.len() {
        if bytes[cursor] != b'"' {
            cursor += 1;
            continue;
        }

        let (key, key_end, key_terminated) = decode_json_string(bytes, cursor + 1);
        if !key_terminated {
            return None;
        }
        cursor = key_end;
        if !aliases.iter().any(|alias| *alias == key) {
            continue;
        }

        let mut value_start = skip_json_whitespace(bytes, cursor);
        if bytes.get(value_start) != Some(&b':') {
            continue;
        }
        value_start = skip_json_whitespace(bytes, value_start + 1);
        if bytes.get(value_start) != Some(&b'"') {
            continue;
        }

        let (value, _, _) = decode_json_string(bytes, value_start + 1);
        return Some(value);
    }

    None
}

/// Build only typed file calls whose safely decoded fields are already
/// present. Other tool names intentionally return `None`.
pub(crate) fn partial_file_tool_call(tool_name: &str, raw: &str) -> Option<ToolCall> {
    let path = || partial_json_string_field(raw, &["path", "file_path", "filePath"]);
    match tool_name.to_ascii_lowercase().as_str() {
        "write" => Some(ToolCall::WriteFile {
            path: path().filter(|path| !path.is_empty())?,
            content: partial_json_string_field(raw, &["content"]),
        }),
        "edit" => Some(ToolCall::EditFile {
            path: path().filter(|path| !path.is_empty())?,
            old_string: partial_json_string_field(raw, &["old_string", "oldText"]),
            new_string: partial_json_string_field(raw, &["new_string", "newText"]),
        }),
        _ => None,
    }
}

fn capped_input(raw: &str) -> &str {
    if raw.len() <= MAX_PARTIAL_TOOL_INPUT_BYTES {
        return raw;
    }
    let mut end = MAX_PARTIAL_TOOL_INPUT_BYTES;
    while !raw.is_char_boundary(end) {
        end -= 1;
    }
    &raw[..end]
}

fn skip_json_whitespace(bytes: &[u8], mut cursor: usize) -> usize {
    while bytes
        .get(cursor)
        .is_some_and(|byte| matches!(byte, b' ' | b'\n' | b'\r' | b'\t'))
    {
        cursor += 1;
    }
    cursor
}

/// Decode the string whose opening quote precedes `cursor`. Returns the
/// decoded prefix, the byte after the closing quote (or input end), and
/// whether a closing quote was observed.
fn decode_json_string(bytes: &[u8], mut cursor: usize) -> (String, usize, bool) {
    let mut decoded = String::new();
    let mut literal_start = cursor;

    while cursor < bytes.len() {
        match bytes[cursor] {
            b'"' => {
                push_utf8_slice(&mut decoded, &bytes[literal_start..cursor]);
                return (decoded, cursor + 1, true);
            }
            b'\\' => {
                push_utf8_slice(&mut decoded, &bytes[literal_start..cursor]);
                let Some(escape) = bytes.get(cursor + 1).copied() else {
                    return (decoded, bytes.len(), false);
                };
                match escape {
                    b'"' => decoded.push('"'),
                    b'\\' => decoded.push('\\'),
                    b'/' => decoded.push('/'),
                    b'b' => decoded.push('\u{0008}'),
                    b'f' => decoded.push('\u{000c}'),
                    b'n' => decoded.push('\n'),
                    b'r' => decoded.push('\r'),
                    b't' => decoded.push('\t'),
                    b'u' => {
                        let Some((character, consumed)) = decode_unicode_escape(bytes, cursor + 2)
                        else {
                            return (decoded, bytes.len(), false);
                        };
                        decoded.push(character);
                        cursor = consumed;
                        literal_start = cursor;
                        continue;
                    }
                    _ => return (decoded, cursor + 2, false),
                }
                cursor += 2;
                literal_start = cursor;
                continue;
            }
            _ => cursor += 1,
        }
    }

    push_utf8_slice(&mut decoded, &bytes[literal_start..]);
    (decoded, bytes.len(), false)
}

fn decode_unicode_escape(bytes: &[u8], hex_start: usize) -> Option<(char, usize)> {
    let first_end = hex_start.checked_add(4)?;
    let first = decode_hex_quad(bytes.get(hex_start..first_end)?)?;
    if !(0xD800..=0xDBFF).contains(&first) {
        return char::from_u32(u32::from(first)).map(|character| (character, first_end));
    }

    let second_hex_start = first_end.checked_add(2)?;
    if bytes.get(first_end..second_hex_start)? != b"\\u" {
        return None;
    }
    let second_end = second_hex_start.checked_add(4)?;
    let second = decode_hex_quad(bytes.get(second_hex_start..second_end)?)?;
    if !(0xDC00..=0xDFFF).contains(&second) {
        return None;
    }
    let scalar = 0x10000 + ((u32::from(first) - 0xD800) << 10) + (u32::from(second) - 0xDC00);
    char::from_u32(scalar).map(|character| (character, second_end))
}

fn decode_hex_quad(hex: &[u8]) -> Option<u16> {
    (hex.len() == 4).then_some(())?;
    hex.iter().try_fold(0_u16, |value, byte| {
        let digit = match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            b'A'..=b'F' => byte - b'A' + 10,
            _ => return None,
        };
        Some((value << 4) | u16::from(digit))
    })
}

fn push_utf8_slice(output: &mut String, bytes: &[u8]) {
    match std::str::from_utf8(bytes) {
        Ok(text) => output.push_str(text),
        Err(error) => output.push_str(std::str::from_utf8(&bytes[..error.valid_up_to()]).unwrap()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_complete_and_unterminated_file_strings_without_general_json_repair() {
        let raw = r#"{"file_path":"src/a.rs","content":"line 1\nline 2"#;

        assert_eq!(
            partial_json_string_field(raw, &["file_path"]),
            Some("src/a.rs".into())
        );
        assert_eq!(
            partial_json_string_field(raw, &["content"]),
            Some("line 1\nline 2".into())
        );
    }

    #[test]
    fn incomplete_escape_is_not_invented() {
        assert_eq!(
            partial_json_string_field(r#"{"content":"line\"#, &["content"]),
            Some("line".into()),
        );
    }

    #[test]
    fn ignores_alias_text_inside_another_json_string() {
        let raw = r#"{"note":"\"content\":\"wrong\"","content":"right"#;

        assert_eq!(
            partial_json_string_field(raw, &["content"]),
            Some("right".into())
        );
    }

    #[test]
    fn decodes_complete_unicode_escapes_and_stops_before_incomplete_ones() {
        assert_eq!(
            partial_json_string_field(r#"{"content":"caf\u00e9 \u12"#, &["content"]),
            Some("café ".into())
        );
    }

    #[test]
    fn builds_only_file_calls_from_supported_names() {
        assert!(matches!(
            partial_file_tool_call(
                "Write",
                r#"{"file_path":"a.txt","content":"hi"#,
            ),
            Some(ToolCall::WriteFile { path, content: Some(content) })
                if path == "a.txt" && content == "hi"
        ));
        assert_eq!(
            partial_file_tool_call("Bash", r#"{"command":"echo hi"#),
            None
        );
    }

    #[test]
    fn maps_edit_aliases_without_inventing_missing_fields() {
        assert_eq!(
            partial_file_tool_call(
                "edit",
                r#"{"path":"a.txt","oldText":"before","newText":"after"#,
            ),
            Some(ToolCall::EditFile {
                path: "a.txt".into(),
                old_string: Some("before".into()),
                new_string: Some("after".into()),
            })
        );
        assert_eq!(partial_file_tool_call("Write", r#"{"content":"hi"#), None);
    }
}

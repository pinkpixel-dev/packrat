//! Turns GPUI keystrokes into the bytes a terminal program expects.

use gpui_kit::Keystroke;

/// The bytes to send to the PTY for a key press, or `None` when the key
/// means nothing to a terminal (a bare modifier, for example).
///
/// `app_cursor` is the terminal's application cursor mode (DECCKM), which
/// changes what the arrow keys send.
pub fn to_bytes(keystroke: &Keystroke, app_cursor: bool) -> Option<Vec<u8>> {
    let mods = &keystroke.modifiers;
    let key = keystroke.key.as_str();

    let named: Option<&[u8]> = match key {
        "enter" => Some(b"\r"),
        "backspace" if mods.control => Some(b"\x08"),
        "backspace" => Some(b"\x7f"),
        "tab" if mods.shift => Some(b"\x1b[Z"),
        "tab" => Some(b"\t"),
        "escape" => Some(b"\x1b"),
        "up" if app_cursor => Some(b"\x1bOA"),
        "down" if app_cursor => Some(b"\x1bOB"),
        "right" if app_cursor => Some(b"\x1bOC"),
        "left" if app_cursor => Some(b"\x1bOD"),
        "up" => Some(b"\x1b[A"),
        "down" => Some(b"\x1b[B"),
        "right" => Some(b"\x1b[C"),
        "left" => Some(b"\x1b[D"),
        "home" => Some(b"\x1b[H"),
        "end" => Some(b"\x1b[F"),
        "pageup" => Some(b"\x1b[5~"),
        "pagedown" => Some(b"\x1b[6~"),
        "insert" => Some(b"\x1b[2~"),
        "delete" => Some(b"\x1b[3~"),
        _ => None,
    };
    if let Some(bytes) = named {
        return Some(bytes.to_vec());
    }

    // Ctrl+letter sends the matching control byte: Ctrl+C is 0x03.
    if mods.control && !mods.alt {
        let mut chars = key.chars();
        if let (Some(ch), None) = (chars.next(), chars.next()) {
            let byte = match ch.to_ascii_lowercase() {
                ch @ 'a'..='z' => Some(ch as u8 - b'a' + 1),
                '@' | ' ' | '2' => Some(0),
                '[' | '3' => Some(0x1b),
                '\\' | '4' => Some(0x1c),
                ']' | '5' => Some(0x1d),
                '6' => Some(0x1e),
                '/' | '7' => Some(0x1f),
                _ => None,
            };
            return byte.map(|byte| vec![byte]);
        }
        return None;
    }

    let text = keystroke.key_char.as_deref().or_else(|| (key == "space").then_some(" "))?;
    let mut bytes = Vec::with_capacity(text.len() + 1);
    // Alt sends ESC before the character, the way most terminals do.
    if mods.alt {
        bytes.push(0x1b);
    }
    bytes.extend_from_slice(text.as_bytes());
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(text: &str) -> Keystroke {
        Keystroke::parse(text).unwrap()
    }

    fn typed(text: &str, ch: &str) -> Keystroke {
        let mut keystroke = key(text);
        keystroke.key_char = Some(ch.into());
        keystroke
    }

    #[test]
    fn named_keys() {
        assert_eq!(to_bytes(&key("enter"), false).unwrap(), b"\r");
        assert_eq!(to_bytes(&key("backspace"), false).unwrap(), b"\x7f");
        assert_eq!(to_bytes(&key("up"), false).unwrap(), b"\x1b[A");
        assert_eq!(to_bytes(&key("up"), true).unwrap(), b"\x1bOA");
        assert_eq!(to_bytes(&key("shift-tab"), false).unwrap(), b"\x1b[Z");
    }

    #[test]
    fn control_bytes() {
        assert_eq!(to_bytes(&key("ctrl-c"), false).unwrap(), [3]);
        assert_eq!(to_bytes(&key("ctrl-d"), false).unwrap(), [4]);
        assert_eq!(to_bytes(&key("ctrl-["), false).unwrap(), [0x1b]);
    }

    #[test]
    fn typed_text_and_alt() {
        assert_eq!(to_bytes(&typed("y", "y"), false).unwrap(), b"y");
        assert_eq!(to_bytes(&typed("shift-y", "Y"), false).unwrap(), b"Y");
        assert_eq!(to_bytes(&typed("alt-b", "b"), false).unwrap(), b"\x1bb");
        assert!(to_bytes(&key("shift"), false).is_none());
    }
}

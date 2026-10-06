//! Terminal key and paste encoding, independent of any GUI framework.

use crate::TerminalMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Enter,
    Backspace,
    Tab,
    Escape,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    Insert,
    Delete,
    PageUp,
    PageDown,
    Function(u8),
}

pub fn encode_key(key: Key, ctrl: bool, alt: bool, mode: &TerminalMode) -> Vec<u8> {
    let sequence: Vec<u8> = match key {
        Key::Char(c) if ctrl && c.is_ascii_alphabetic() => {
            vec![(c.to_ascii_uppercase() as u8) - b'@']
        },
        Key::Char(c) => c.to_string().into_bytes(),
        Key::Enter => b"\r".to_vec(),
        Key::Backspace => vec![0x7f],
        Key::Tab => b"\t".to_vec(),
        Key::Escape => vec![0x1b],
        Key::Up => cursor(b'A', mode),
        Key::Down => cursor(b'B', mode),
        Key::Right => cursor(b'C', mode),
        Key::Left => cursor(b'D', mode),
        Key::Home => b"\x1b[H".to_vec(),
        Key::End => b"\x1b[F".to_vec(),
        Key::Insert => b"\x1b[2~".to_vec(),
        Key::Delete => b"\x1b[3~".to_vec(),
        Key::PageUp => b"\x1b[5~".to_vec(),
        Key::PageDown => b"\x1b[6~".to_vec(),
        Key::Function(n @ 1..=4) => vec![0x1b, b'O', b'P' + n - 1],
        Key::Function(n @ 5..=12) => format!(
            "\x1b[{}~",
            [15, 17, 18, 19, 20, 21, 23, 24][(n - 5) as usize]
        )
        .into_bytes(),
        Key::Function(_) => Vec::new(),
    };
    if alt && !matches!(key, Key::Escape) {
        let mut out = vec![0x1b];
        out.extend(sequence);
        out
    } else {
        sequence
    }
}

fn cursor(final_byte: u8, mode: &TerminalMode) -> Vec<u8> {
    vec![
        0x1b,
        if mode.application_cursor { b'O' } else { b'[' },
        final_byte,
    ]
}

pub fn encode_paste(text: &str, mode: &TerminalMode) -> Vec<u8> {
    if mode.bracketed_paste {
        [b"\x1b[200~".as_slice(), text.as_bytes(), b"\x1b[201~"].concat()
    } else {
        text.as_bytes().to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_keys_follow_application_mode() {
        let mut mode = TerminalMode::default();
        assert_eq!(encode_key(Key::Up, false, false, &mode), b"\x1b[A");
        mode.application_cursor = true;
        assert_eq!(encode_key(Key::Up, false, false, &mode), b"\x1bOA");
    }

    #[test]
    fn bracketed_paste_wraps_payload() {
        let mode = TerminalMode {
            bracketed_paste: true,
            ..Default::default()
        };
        assert_eq!(encode_paste("echo hi", &mode), b"\x1b[200~echo hi\x1b[201~");
    }
}

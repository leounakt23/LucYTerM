//! Terminal key, mouse, and paste encoding, independent of any GUI framework.

use crate::{MouseMode, TerminalMode};

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

/// Mouse button for reporting (xterm 1000/1006).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    WheelUp,
    WheelDown,
}

/// Encode one mouse event for the remote side (xterm mouse reporting).
///
/// `col`/`row` are zero-based cell coordinates; both encodings are 1-based.
/// Returns empty when reporting is off (`MouseMode::None`), so callers can
/// forward the result unconditionally. X10 single-byte coordinates cap at
/// cell 222 (col/row + 33 must fit in a byte); out-of-range events encode
/// as empty rather than corrupt — SGR (1006) has no such limit.
pub fn encode_mouse(
    button: MouseButton,
    pressed: bool,
    col: u16,
    row: u16,
    mode: &TerminalMode,
) -> Vec<u8> {
    if mode.mouse_mode == MouseMode::None {
        return Vec::new();
    }
    let base: u8 = match button {
        MouseButton::Left => 0,
        MouseButton::Middle => 1,
        MouseButton::Right => 2,
        MouseButton::WheelUp => 64,
        MouseButton::WheelDown => 65,
    };
    // Releases report code 3 (wheels are press-only and never release).
    let code = match button {
        MouseButton::WheelUp | MouseButton::WheelDown => base,
        _ if pressed => base,
        _ => 3,
    };
    if mode.mouse_sgr {
        let terminator = if pressed { b'M' } else { b'm' };
        format!(
            "\x1b[<{0};{1};{2}{3}",
            code,
            col + 1,
            row + 1,
            terminator as char
        )
        .into_bytes()
    } else {
        let (col, row) = (col + 1, row + 1);
        if col > 223 || row > 223 {
            return Vec::new();
        }
        vec![
            0x1b,
            b'[',
            b'M',
            32 + code,
            (32 + col) as u8,
            (32 + row) as u8,
        ]
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

    #[test]
    fn mouse_off_encodes_empty() {
        let mode = TerminalMode::default();
        assert!(encode_mouse(MouseButton::Left, true, 0, 0, &mode).is_empty());
    }

    #[test]
    fn mouse_x10_press_and_release() {
        let mode = TerminalMode {
            mouse_mode: MouseMode::X10,
            ..Default::default()
        };
        // Left press at cell (0,0): Cb=32, Cx=33, Cy=33.
        assert_eq!(
            encode_mouse(MouseButton::Left, true, 0, 0, &mode),
            vec![0x1b, b'[', b'M', 32, 33, 33]
        );
        // Release reports code 3.
        assert_eq!(
            encode_mouse(MouseButton::Left, false, 4, 2, &mode),
            vec![0x1b, b'[', b'M', 35, 37, 35]
        );
    }

    #[test]
    fn mouse_x10_clamps_out_of_range() {
        let mode = TerminalMode {
            mouse_mode: MouseMode::Button,
            ..Default::default()
        };
        assert!(encode_mouse(MouseButton::Left, true, 300, 0, &mode).is_empty());
        assert!(encode_mouse(MouseButton::Left, true, 0, 300, &mode).is_empty());
    }

    #[test]
    fn mouse_sgr_press_release_and_wheel() {
        let mode = TerminalMode {
            mouse_mode: MouseMode::Any,
            mouse_sgr: true,
            ..Default::default()
        };
        assert_eq!(
            encode_mouse(MouseButton::Right, true, 9, 4, &mode),
            b"\x1b[<2;10;5M"
        );
        assert_eq!(
            encode_mouse(MouseButton::Right, false, 9, 4, &mode),
            b"\x1b[<3;10;5m"
        );
        // Wheels are press-only; large coordinates are fine in SGR.
        assert_eq!(
            encode_mouse(MouseButton::WheelUp, true, 500, 200, &mode),
            b"\x1b[<64;501;201M"
        );
    }
}

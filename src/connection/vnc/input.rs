//! VNC input mapping (Prompt 4.3): keysyms, pointer events, clipboard.
//!
//! Pure builders over RFB conventions — no I/O, fully unit-tested. The
//! session layer frames these into `KeyEvent`/`PointerEvent`/`ClientCutText`
//! messages; the viewer widget maps local pointer coordinates through
//! [`scale_point`] before sending.

/// X11 keysym type (RFB reuses the X keysym space).
pub type KeySym = u32;

/// ASCII keysyms are their codepoints; special keys live at `0xFF00+`.
pub mod keysym {
    pub const BACKSPACE: u32 = 0xFF08;
    pub const TAB: u32 = 0xFF09;
    pub const RETURN: u32 = 0xFF0D;
    pub const ESCAPE: u32 = 0xFF1B;
    pub const INSERT: u32 = 0xFF63;
    pub const DELETE: u32 = 0xFFFF;
    pub const HOME: u32 = 0xFF50;
    pub const END: u32 = 0xFF57;
    pub const PAGE_UP: u32 = 0xFF55;
    pub const PAGE_DOWN: u32 = 0xFF56;
    pub const LEFT: u32 = 0xFF51;
    pub const UP: u32 = 0xFF52;
    pub const RIGHT: u32 = 0xFF53;
    pub const DOWN: u32 = 0xFF54;
    pub const SHIFT_LEFT: u32 = 0xFFE1;
    pub const CONTROL_LEFT: u32 = 0xFFE3;
    pub const ALT_LEFT: u32 = 0xFFE9;
    pub const SUPER_LEFT: u32 = 0xFFEB;
}

/// Function keys F1..F12 (`0xFFBE + n - 1`).
pub fn keysym_f(n: u8) -> Option<KeySym> {
    (1..=12).contains(&n).then_some(0xFFBE + u32::from(n) - 1)
}

/// Printable ASCII char → keysym (`None` for control characters).
pub fn keysym_for_char(c: char) -> Option<KeySym> {
    if c.is_ascii_graphic() || c == ' ' {
        Some(c as KeySym)
    } else {
        None
    }
}

/// Pointer button mask bits (RFB §7.5.5).
pub mod button {
    pub const LEFT: u8 = 0x01;
    pub const MIDDLE: u8 = 0x02;
    pub const RIGHT: u8 = 0x04;
    pub const WHEEL_UP: u8 = 0x08;
    pub const WHEEL_DOWN: u8 = 0x10;
}

/// One key press/release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyPress {
    pub keysym: KeySym,
    pub down: bool,
}

impl KeyPress {
    pub fn down(keysym: KeySym) -> Self {
        Self { keysym, down: true }
    }

    pub fn up(keysym: KeySym) -> Self {
        Self {
            keysym,
            down: false,
        }
    }

    /// Full tap (down + up) for a printable character.
    pub fn tap_char(c: char) -> Option<[Self; 2]> {
        keysym_for_char(c).map(|keysym| [Self::down(keysym), Self::up(keysym)])
    }
}

/// Ctrl+Alt+Del chord (remote login screens / task manager).
pub fn ctrl_alt_del() -> [KeyPress; 6] {
    use keysym::{ALT_LEFT, CONTROL_LEFT, DELETE};
    [
        KeyPress::down(CONTROL_LEFT),
        KeyPress::down(ALT_LEFT),
        KeyPress::down(DELETE),
        KeyPress::up(DELETE),
        KeyPress::up(ALT_LEFT),
        KeyPress::up(CONTROL_LEFT),
    ]
}

/// Pointer event in remote coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PointerEvent {
    pub buttons: u8,
    pub x: u16,
    pub y: u16,
}

impl PointerEvent {
    pub fn new(buttons: u8, x: u16, y: u16) -> Self {
        Self { buttons, x, y }
    }
}

/// Scaling between the local widget and the remote desktop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScalingMode {
    /// Stretch to fill (aspect may distort).
    #[default]
    Fit,
    /// 1:1 pixels (scroll when larger — widget clips).
    OneToOne,
}

/// Map a local point to remote coordinates.
///
/// - `Fit`: scale by `remote/local` per axis, clamped into the desktop.
/// - `OneToOne`: truncate (widget shows the top-left crop).
pub fn scale_point(
    local: (f32, f32),
    local_size: (f32, f32),
    remote_size: (u16, u16),
    mode: ScalingMode,
) -> (u16, u16) {
    let (rw, rh) = (f32::from(remote_size.0), f32::from(remote_size.1));
    let (x, y) = match mode {
        ScalingMode::OneToOne => (local.0, local.1),
        ScalingMode::Fit => {
            if local_size.0 <= 0.0 || local_size.1 <= 0.0 {
                (0.0, 0.0)
            } else {
                (local.0 * rw / local_size.0, local.1 * rh / local_size.1)
            }
        },
    };
    (
        x.clamp(0.0, rw - 1.0).max(0.0) as u16,
        y.clamp(0.0, rh - 1.0).max(0.0) as u16,
    )
}

/// Encode clipboard text as latin-1 (`ClientCutText` body; non-latin-1
/// chars degrade to `?` rather than failing the sync).
pub fn encode_cut_text(text: &str) -> Vec<u8> {
    text.chars()
        .map(|c| if c.is_ascii() { c as u8 } else { b'?' })
        .collect()
}

/// Decode a `ServerCutText` body (latin-1 → UTF-8 losslessly for ASCII).
pub fn decode_cut_text(bytes: &[u8]) -> String {
    bytes.iter().map(|b| *b as char).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keysym_tables_cover_printables_and_specials() {
        assert_eq!(keysym_for_char('a'), Some(0x61));
        assert_eq!(keysym_for_char(' '), Some(0x20));
        assert_eq!(keysym_for_char('\n'), None);
        assert_eq!(keysym_f(1), Some(0xFFBE));
        assert_eq!(keysym_f(12), Some(0xFFC9));
        assert_eq!(keysym_f(0), None);
        assert_eq!(keysym_f(13), None);
    }

    #[test]
    fn tap_and_cad_chords() {
        let tap = KeyPress::tap_char('Z').unwrap();
        assert_eq!(tap[0], KeyPress::down(0x5A));
        assert_eq!(tap[1], KeyPress::up(0x5A));
        assert!(KeyPress::tap_char('\t').is_none());
        let chord = ctrl_alt_del();
        assert_eq!(chord.len(), 6);
        assert!(chord[0].down && !chord[5].down);
    }

    #[test]
    fn scaling_maps_and_clamps() {
        // Fit: 800x600 widget over 1920x1080 desktop.
        assert_eq!(
            scale_point(
                (400.0, 300.0),
                (800.0, 600.0),
                (1920, 1080),
                ScalingMode::Fit
            ),
            (960, 540)
        );
        // Out-of-bounds clamps into the desktop.
        assert_eq!(
            scale_point(
                (900.0, 700.0),
                (800.0, 600.0),
                (1920, 1080),
                ScalingMode::Fit
            ),
            (1919, 1079)
        );
        // 1:1 passes through (clamped).
        assert_eq!(
            scale_point(
                (100.0, 50.0),
                (800.0, 600.0),
                (1920, 1080),
                ScalingMode::OneToOne
            ),
            (100, 50)
        );
        // Degenerate widget size never divides.
        assert_eq!(
            scale_point((5.0, 5.0), (0.0, 0.0), (1920, 1080), ScalingMode::Fit),
            (0, 0)
        );
    }

    #[test]
    fn clipboard_round_trip_is_lossy_only_outside_ascii() {
        assert_eq!(decode_cut_text(&encode_cut_text("hello")), "hello");
        assert_eq!(encode_cut_text("héllo")[1], b'?');
    }
}

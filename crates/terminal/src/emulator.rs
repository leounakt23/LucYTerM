//! `Terminal`: the emulator core — grid + cursor + modes + charsets, driven
//! by the vte parser (prompt 2.3). Public API:
//! `write_bytes` (SSH output), `resize`, `clear`, `scroll_up/down`,
//! `get_cell`, selection, bell/title/cwd side channels.

use crate::grid::{
    char_width, Attributes, Cell, Color, Cursor, Grid, Pos, Selection, SelectionMode,
};

// ---------------------------------------------------------------------------
// Modes & charsets
// ---------------------------------------------------------------------------

/// Mouse reporting mode (xterm extensions).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MouseMode {
    #[default]
    None,
    /// 1000: press/release only.
    X10,
    /// 1002: button-event tracking (press/release/drag).
    Button,
    /// 1003: any motion.
    Any,
}

/// Terminal modes toggled via DEC private set/reset (CSI ? n h/l).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TerminalMode {
    /// DECCKM: cursor keys send SS3 instead of CSI.
    pub application_cursor: bool,
    /// DECOM: CUP/VPA relative to scroll region.
    pub origin_mode: bool,
    /// DECAWM: wrap at the right margin (default on).
    pub auto_wrap: bool,
    /// IRM: inserted characters shift existing cells right.
    pub insert_mode: bool,
    /// 47/1047/1049 alternate screen.
    pub alternate_screen: bool,
    /// 2004: paste wrapped in CSI 200~/201~.
    pub bracketed_paste: bool,
    pub mouse_mode: MouseMode,
    /// 1006: SGR-style mouse encoding.
    pub mouse_sgr: bool,
    /// DECKPAM/DECPAM application keypad (ESC = / ESC >).
    pub application_keypad: bool,
}

/// G0/G1 designations (SCS) + locking shifts (SI/SO).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Charset {
    #[default]
    Ascii,
    Uk,
    DecSpecialGraphics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum CharsetShift {
    #[default]
    G0,
    G1,
}

/// G0/G1 charset state.
#[derive(Debug, Clone, Default)]
pub struct CharsetState {
    g0: Charset,
    g1: Charset,
    active: CharsetShift,
}

impl CharsetState {
    fn translate(&self, c: char) -> char {
        let charset = match self.active {
            CharsetShift::G0 => self.g0,
            CharsetShift::G1 => self.g1,
        };
        match charset {
            Charset::Ascii => c,
            Charset::Uk => {
                if c == '#' {
                    '£'
                } else {
                    c
                }
            },
            Charset::DecSpecialGraphics => translate_dec_special(c),
        }
    }

    fn designate(&mut self, which: u8, charset: Charset) {
        match which {
            b'(' => self.g0 = charset,
            b')' => self.g1 = charset,
            _ => {},
        }
    }
}

/// DEC Special Graphics subset (line drawing) — the widely implemented table.
fn translate_dec_special(c: char) -> char {
    match c {
        '`' => '◆',
        'a' => '▒',
        'f' => '°',
        'g' => '±',
        'j' => '┘',
        'k' => '┐',
        'l' => '┌',
        'm' => '└',
        'n' => '┼',
        'o' => '⎺',
        'p' => '⎻',
        'q' => '─',
        'r' => '⎼',
        's' => '⎽',
        't' => '├',
        'u' => '┤',
        'v' => '┴',
        'w' => '┬',
        'x' => '│',
        other => other,
    }
}

/// Current print style (SGR state applied to new cells).
#[derive(Debug, Clone, Default)]
pub struct Style {
    pub fg: Color,
    pub bg: Color,
    pub attrs: Attributes,
}

// ---------------------------------------------------------------------------
// Terminal
// ---------------------------------------------------------------------------

/// The emulator: grid + cursor + selection + mode + charset, implementing
/// `vte::Perform` (see `parser.rs`).
pub struct Terminal {
    pub grid: Grid,
    pub cursor: Cursor,
    pub selection: Option<Selection>,
    pub mode: TerminalMode,
    pub charset: CharsetState,
    /// Window title (OSC 0/2).
    pub title: String,
    /// Working directory hint (OSC 7).
    pub cwd: Option<String>,
    /// BEL seen since last `take_bell()`.
    bell_pending: bool,
    /// Current SGR style for printed cells.
    style: Style,
    /// DECSC-saved cursor (restored by DECRC / 1049 exit).
    saved_cursor: Option<Cursor>,
    parser: vte::Parser,
}

impl std::fmt::Debug for Terminal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Terminal")
            .field("grid", &self.grid)
            .field("cursor", &self.cursor)
            .field("mode", &self.mode)
            .field("title", &self.title)
            .finish()
    }
}

impl Terminal {
    pub fn new(cols: u16, rows: u16, scrollback_lines: usize) -> Self {
        Self {
            grid: Grid::new(cols, rows, scrollback_lines),
            cursor: Cursor::default(),
            selection: None,
            mode: TerminalMode {
                auto_wrap: true,
                ..TerminalMode::default()
            },
            charset: CharsetState::default(),
            title: String::new(),
            cwd: None,
            bell_pending: false,
            style: Style::default(),
            saved_cursor: None,
            parser: vte::Parser::new(),
        }
    }

    /// Feed SSH output: parse and update grid/cursor/modes (prompt 2.3).
    pub fn write_bytes(&mut self, bytes: &[u8]) {
        let mut parser = std::mem::replace(&mut self.parser, vte::Parser::new());
        for byte in bytes {
            parser.advance(self, *byte);
        }
        self.parser = parser;
    }

    /// Resize (prompt 2.3 signature: rows, cols); cursor is clamped.
    pub fn resize(&mut self, rows: u16, cols: u16) {
        self.grid.resize(cols, rows);
        self.cursor.row = self.cursor.row.min(rows.saturating_sub(1));
        self.cursor.col = self.cursor.col.min(cols.saturating_sub(1));
    }

    /// Clear the screen (CSI 2 J / RIS); cursor unchanged (xterm behavior).
    pub fn clear(&mut self) {
        self.grid.clear();
    }

    pub fn scroll_up(&mut self, n: u16) {
        self.grid.scroll_up(n);
    }

    pub fn scroll_down(&mut self, n: u16) {
        self.grid.scroll_down(n);
    }

    pub fn get_cell(&self, row: u16, col: u16) -> Option<&Cell> {
        self.grid.get_cell(row, col)
    }

    /// Consume the pending BEL flag (UI notification, feature #46).
    pub fn take_bell(&mut self) -> bool {
        std::mem::take(&mut self.bell_pending)
    }

    // -- selection ---------------------------------------------------------

    pub fn start_selection(&mut self, pos: Pos, mode: SelectionMode) {
        self.selection = Some(Selection {
            start: pos,
            end: pos,
            mode,
        });
    }

    pub fn update_selection(&mut self, pos: Pos) {
        if let Some(selection) = self.selection.as_mut() {
            selection.end = pos;
        }
    }

    pub fn clear_selection(&mut self) {
        self.selection = None;
    }

    /// Selected text (simple linear sweep between normalized bounds).
    pub fn selection_text(&self) -> Option<String> {
        let selection = self.selection.as_ref()?;
        let (start, end) = selection.bounds();
        let mut out = String::new();
        for row in start.row..=end.row {
            for col in start.col..=end.col {
                if let Some(cell) = self.grid.get_cell(row, col) {
                    if cell.width > 0 {
                        out.push(cell.ch);
                    }
                }
            }
            if row < end.row {
                out.push('\n');
            }
        }
        Some(out.trim_end_matches('\n').to_string())
    }

    // -- internal helpers shared with the Perform impl (pub(crate)) --------

    /// Move down one line, scrolling the region at its bottom edge.
    pub(crate) fn index_linefeed(&mut self) {
        if self.cursor.row == self.grid.scroll_region().1 {
            self.grid.scroll_up(1);
        } else if self.cursor.row + 1 < self.grid.rows() {
            self.cursor.row += 1;
        }
    }

    /// Reverse index (ESC M): move up, scrolling the region at its top edge.
    pub(crate) fn reverse_index(&mut self) {
        if self.cursor.row == self.grid.scroll_region().0 {
            self.grid.scroll_down(1);
        } else {
            self.cursor.row = self.cursor.row.saturating_sub(1);
        }
    }

    /// Absolute (1-based) or region-relative (DECOM) target row for CUP/VPA.
    pub(crate) fn target_row(&self, requested: u16) -> u16 {
        let (top, bottom) = self.grid.scroll_region();
        if self.mode.origin_mode {
            (top + requested.saturating_sub(1)).min(bottom)
        } else {
            requested
                .saturating_sub(1)
                .min(self.grid.rows().saturating_sub(1))
        }
    }

    pub(crate) fn clamp_col(&mut self) {
        self.cursor.col = self.cursor.col.min(self.grid.cols().saturating_sub(1));
    }

    pub(crate) fn style(&self) -> Style {
        self.style.clone()
    }

    pub(crate) fn set_style(&mut self, style: Style) {
        self.style = style;
    }

    pub(crate) fn set_bell(&mut self) {
        self.bell_pending = true;
    }

    pub(crate) fn designate_charset(&mut self, which: u8, charset: Charset) {
        self.charset.designate(which, charset);
    }

    pub(crate) fn shift_charset(&mut self, g1: bool) {
        self.charset.active = if g1 {
            CharsetShift::G1
        } else {
            CharsetShift::G0
        };
    }

    pub(crate) fn save_cursor(&mut self) {
        self.saved_cursor = Some(self.cursor.clone());
    }

    pub(crate) fn restore_cursor(&mut self) {
        if let Some(saved) = self.saved_cursor.take() {
            self.cursor = saved;
            self.clamp_col();
        } else {
            self.cursor = Cursor::default();
        }
    }

    pub(crate) fn print_char(&mut self, c: char) {
        let c = self.charset.translate(c);
        let width = char_width(c);
        if width == 0 {
            // Combining marks: merge into the previous cell later (prompt 2.x
            // renderer handles clusters); skip for now.
            return;
        }

        // Auto-wrap (DECAWM).
        if self.cursor.col >= self.grid.cols() {
            if self.mode.auto_wrap {
                self.cursor.col = 0;
                self.index_linefeed();
            } else {
                self.cursor.col = self.grid.cols().saturating_sub(1);
            }
        }

        // Insert mode (IRM): shift the rest of the line right.
        if self.mode.insert_mode {
            if let Some(row) = self.grid.row_mut(self.cursor.row) {
                let col = self.cursor.col as usize;
                for i in (col..row.len().saturating_sub(width as usize)).rev() {
                    row[i + width as usize] = row[i].clone();
                }
            }
        }

        let mut cell = Cell {
            ch: c,
            width,
            ..self.style.to_cell()
        };
        if let Some(slot) = self.grid.get_cell_mut(self.cursor.row, self.cursor.col) {
            *slot = cell.clone();
        }
        if width == 2 {
            // Spacer for the trailing half of a wide glyph.
            if let Some(slot) = self.grid.get_cell_mut(self.cursor.row, self.cursor.col + 1) {
                cell.ch = '\0';
                cell.width = 0;
                *slot = cell;
            }
        }
        self.cursor.col += u16::from(width);
    }
}

impl Style {
    fn to_cell(&self) -> Cell {
        Cell {
            ch: ' ',
            fg: self.fg,
            bg: self.bg,
            attrs: self.attrs,
            width: 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::Color;

    #[test]
    fn basic_shell_output_lands_in_cells() {
        let mut term = Terminal::new(20, 5, 10);
        term.write_bytes(b"user@host:~$ ls\r\n");
        assert_eq!(term.grid.row_text(0).trim_end(), "user@host:~$ ls");
        assert_eq!(term.cursor.row, 1);
        assert_eq!(term.cursor.col, 0);
    }

    #[test]
    fn cjk_glyphs_occupy_two_cells() {
        let mut term = Terminal::new(10, 2, 0);
        term.write_bytes("漢字".as_bytes());
        let cell = term.get_cell(0, 0).unwrap();
        assert_eq!(cell.ch, '漢');
        assert_eq!(cell.width, 2);
        assert_eq!(term.get_cell(0, 1).unwrap().width, 0, "spacer");
        assert_eq!(term.cursor.col, 4);
    }

    #[test]
    fn cursor_visible_toggle_and_shape() {
        let mut term = Terminal::new(10, 2, 0);
        term.write_bytes(b"\x1b[?25l"); // hide
        assert!(!term.cursor.visible);
        term.write_bytes(b"\x1b[?25h"); // show
        assert!(term.cursor.visible);
        term.write_bytes(b"\x1b[4 q"); // DECSCUSR 4 = underline
        assert_eq!(term.cursor.shape, crate::grid::CursorShape::Underline);
    }

    #[test]
    fn colors_apply_to_cells() {
        let mut term = Terminal::new(10, 2, 0);
        term.write_bytes(b"\x1b[1;31mERR\x1b[0m");
        let cell = term.get_cell(0, 0).unwrap();
        assert_eq!(cell.ch, 'E');
        assert_eq!(cell.fg, Color::Indexed(1));
        assert!(cell.attrs.bold);
        // After reset, following text is default.
        term.write_bytes(b"ok");
        let cell = term.get_cell(0, 3).unwrap();
        assert_eq!(cell.fg, Color::Default);
        assert!(!cell.attrs.bold);
    }

    #[test]
    fn truecolor_and_256() {
        let mut term = Terminal::new(10, 2, 0);
        term.write_bytes(b"\x1b[38;5;196mX\x1b[0m");
        assert_eq!(term.get_cell(0, 0).unwrap().fg, Color::Indexed(196));
        term.write_bytes(b"\x1b[48;2;10;20;30mY\x1b[0m");
        assert_eq!(term.get_cell(0, 1).unwrap().bg, Color::Rgb(10, 20, 30));
    }

    #[test]
    fn resize_clamps_cursor() {
        let mut term = Terminal::new(80, 24, 100);
        term.write_bytes(b"\x1b[23;79H"); // 1-based CUP → row 22, col 78
        assert_eq!(term.cursor.row, 22);
        term.resize(10, 20);
        assert_eq!(term.cursor.row, 9);
        assert_eq!(term.cursor.col, 19);
    }

    #[test]
    fn bell_flag_is_set_by_bel() {
        let mut term = Terminal::new(10, 2, 0);
        term.write_bytes(b"\x07");
        assert!(term.take_bell());
        assert!(!term.take_bell());
    }

    #[test]
    fn osc_title_and_cwd() {
        let mut term = Terminal::new(10, 2, 0);
        term.write_bytes(b"\x1b]0;my title\x07");
        assert_eq!(term.title, "my title");
        term.write_bytes(b"\x1b]7;file://host/home/user\x1b\\");
        assert_eq!(term.cwd.as_deref(), Some("/home/user"));
    }

    #[test]
    fn selection_text() {
        let mut term = Terminal::new(10, 2, 0);
        term.write_bytes(b"hello");
        term.start_selection(Pos { row: 0, col: 0 }, SelectionMode::Mouse);
        term.update_selection(Pos { row: 0, col: 4 });
        assert_eq!(term.selection_text().as_deref(), Some("hello"));
    }
}

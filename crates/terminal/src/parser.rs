use crate::emulator::{Charset, MouseMode, Style, Terminal};
use crate::grid::{Color, Cursor, CursorShape};

fn param(params: &vte::Params, index: usize, default: u16) -> u16 {
    params
        .iter()
        .nth(index)
        .and_then(|values| values.first())
        .copied()
        .unwrap_or(default)
}

impl vte::Perform for Terminal {
    fn print(&mut self, c: char) {
        self.print_char(c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            0x07 => self.set_bell(),
            0x08 => self.cursor.col = self.cursor.col.saturating_sub(1),
            0x09 => self.cursor.col = self.grid.next_tabstop(self.cursor.col),
            0x0a..=0x0c => self.index_linefeed(),
            0x0d => self.cursor.col = 0,
            0x0e => self.shift_charset(true),
            0x0f => self.shift_charset(false),
            _ => {},
        }
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        let Some(command) = params.first().and_then(|p| std::str::from_utf8(p).ok()) else {
            return;
        };
        let value = params
            .get(1)
            .and_then(|p| std::str::from_utf8(p).ok())
            .unwrap_or_default();
        match command {
            "0" | "2" => self.title = value.to_string(),
            "7" => {
                let path = value
                    .strip_prefix("file://")
                    .and_then(|v| v.find('/').map(|i| &v[i..]))
                    .unwrap_or(value);
                self.cwd = Some(path.to_string());
            },
            _ => {},
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        intermediates: &[u8],
        ignore: bool,
        action: char,
    ) {
        if ignore {
            return;
        }
        let n = param(params, 0, 1).max(1);
        let private = intermediates.contains(&b'?');
        match action {
            'A' => self.cursor.row = self.cursor.row.saturating_sub(n),
            'B' => self.cursor.row = (self.cursor.row + n).min(self.grid.rows().saturating_sub(1)),
            'C' | 'a' => {
                self.cursor.col = (self.cursor.col + n).min(self.grid.cols().saturating_sub(1))
            },
            'D' => self.cursor.col = self.cursor.col.saturating_sub(n),
            'E' => {
                self.cursor.row = (self.cursor.row + n).min(self.grid.rows().saturating_sub(1));
                self.cursor.col = 0;
            },
            'F' => {
                self.cursor.row = self.cursor.row.saturating_sub(n);
                self.cursor.col = 0;
            },
            'G' | '`' => {
                self.cursor.col = n.saturating_sub(1).min(self.grid.cols().saturating_sub(1))
            },
            'H' | 'f' => {
                self.cursor.row = self.target_row(param(params, 0, 1));
                self.cursor.col = param(params, 1, 1)
                    .saturating_sub(1)
                    .min(self.grid.cols().saturating_sub(1));
            },
            'd' => self.cursor.row = self.target_row(n),
            'J' if param(params, 0, 0) == 2 || param(params, 0, 0) == 3 => self.clear(),
            'K' => {
                let mode = param(params, 0, 0);
                if let Some(row) = self.grid.row_mut(self.cursor.row) {
                    let col = self.cursor.col as usize;
                    let range = match mode {
                        1 => 0..col.saturating_add(1).min(row.len()),
                        2 => 0..row.len(),
                        _ => col.min(row.len())..row.len(),
                    };
                    for cell in &mut row[range] {
                        *cell = Default::default();
                    }
                }
            },
            'm' => apply_sgr(self, params),
            'h' | 'l' => {
                let enabled = action == 'h';
                for values in params.iter() {
                    let code = values.first().copied().unwrap_or(0);
                    if private {
                        match code {
                            1 => self.mode.application_cursor = enabled,
                            7 => self.mode.auto_wrap = enabled,
                            25 => self.cursor.visible = enabled,
                            47 | 1047 | 1049 => self.mode.alternate_screen = enabled,
                            1000 => {
                                self.mode.mouse_mode = if enabled {
                                    MouseMode::X10
                                } else {
                                    MouseMode::None
                                }
                            },
                            1002 => {
                                self.mode.mouse_mode = if enabled {
                                    MouseMode::Button
                                } else {
                                    MouseMode::None
                                }
                            },
                            1003 => {
                                self.mode.mouse_mode = if enabled {
                                    MouseMode::Any
                                } else {
                                    MouseMode::None
                                }
                            },
                            1006 => self.mode.mouse_sgr = enabled,
                            2004 => self.mode.bracketed_paste = enabled,
                            _ => {},
                        }
                    } else if code == 4 {
                        self.mode.insert_mode = enabled;
                    }
                }
            },
            'r' => {
                let top = param(params, 0, 1).saturating_sub(1);
                let bottom = param(params, 1, self.grid.rows()).saturating_sub(1);
                self.grid.set_scroll_region(top, bottom);
                self.cursor = Cursor::default();
            },
            'S' => self.grid.scroll_up(n),
            'T' => self.grid.scroll_down(n),
            's' => self.save_cursor(),
            'u' => self.restore_cursor(),
            'q' if intermediates.contains(&b' ') => {
                self.cursor.shape = match n {
                    3 | 4 => CursorShape::Underline,
                    5 | 6 => CursorShape::Bar,
                    _ => CursorShape::Block,
                };
            },
            _ => {},
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        if ignore {
            return;
        }
        if let Some(which @ (b'(' | b')')) = intermediates.first().copied() {
            let charset = match byte {
                b'0' => Charset::DecSpecialGraphics,
                b'A' => Charset::Uk,
                _ => Charset::Ascii,
            };
            self.designate_charset(which, charset);
            return;
        }
        match byte {
            b'7' => self.save_cursor(),
            b'8' => self.restore_cursor(),
            b'D' => self.index_linefeed(),
            b'E' => {
                self.index_linefeed();
                self.cursor.col = 0;
            },
            b'M' => self.reverse_index(),
            b'H' => self.grid.set_tabstop(self.cursor.col),
            b'=' => self.mode.application_keypad = true,
            b'>' => self.mode.application_keypad = false,
            b'c' => {
                self.clear();
                self.cursor = Cursor::default();
                self.mode = Default::default();
                self.mode.auto_wrap = true;
            },
            _ => {},
        }
    }
}

fn apply_sgr(terminal: &mut Terminal, params: &vte::Params) {
    let values: Vec<u16> = params.iter().flat_map(|p| p.iter().copied()).collect();
    let values = if values.is_empty() { vec![0] } else { values };
    let mut style = terminal.style();
    let mut i = 0;
    while i < values.len() {
        match values[i] {
            0 => style = Style::default(),
            1 => style.attrs.bold = true,
            2 => style.attrs.dim = true,
            3 => style.attrs.italic = true,
            4 => style.attrs.underline = true,
            7 => style.attrs.reverse = true,
            9 => style.attrs.strikethrough = true,
            22 => {
                style.attrs.bold = false;
                style.attrs.dim = false;
            },
            23 => style.attrs.italic = false,
            24 => style.attrs.underline = false,
            27 => style.attrs.reverse = false,
            29 => style.attrs.strikethrough = false,
            30..=37 => style.fg = Color::Indexed((values[i] - 30) as u8),
            40..=47 => style.bg = Color::Indexed((values[i] - 40) as u8),
            90..=97 => style.fg = Color::Indexed((values[i] - 90 + 8) as u8),
            100..=107 => style.bg = Color::Indexed((values[i] - 100 + 8) as u8),
            39 => style.fg = Color::Default,
            49 => style.bg = Color::Default,
            38 | 48 if i + 2 < values.len() && values[i + 1] == 5 => {
                let color = Color::Indexed(values[i + 2] as u8);
                if values[i] == 38 {
                    style.fg = color
                } else {
                    style.bg = color
                };
                i += 2;
            },
            38 | 48 if i + 4 < values.len() && values[i + 1] == 2 => {
                let color = Color::Rgb(
                    values[i + 2] as u8,
                    values[i + 3] as u8,
                    values[i + 4] as u8,
                );
                if values[i] == 38 {
                    style.fg = color
                } else {
                    style.bg = color
                };
                i += 4;
            },
            _ => {},
        }
        i += 1;
    }
    terminal.set_style(style);
}

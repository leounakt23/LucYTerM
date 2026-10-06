//! Grid model: cells, cursor, selection, scrollback ring buffer, tab stops,
//! and scroll regions (prompt 2.3).

use std::collections::VecDeque;

// ---------------------------------------------------------------------------
// Cells & styling
// ---------------------------------------------------------------------------

/// Terminal color: default palette entry, 256-color index, or truecolor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Color {
    #[default]
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

/// Character attributes (SGR subset).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Attributes {
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub reverse: bool,
    pub strikethrough: bool,
}

/// One grid cell: glyph, colors, attributes, and display width (CJK = 2;
/// the trailing half of a wide char is a `width == 0` spacer cell).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub fg: Color,
    pub bg: Color,
    pub attrs: Attributes,
    pub width: u8,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: Color::Default,
            bg: Color::Default,
            attrs: Attributes::default(),
            width: 1,
        }
    }
}

/// Display width of a character (East Asian Wide/Fullwidth ranges → 2,
/// combining marks → 0, everything else → 1).
pub fn char_width(c: char) -> u8 {
    let cp = c as u32;
    let wide = matches!(cp,
        0x1100..=0x115F
        | 0x2E80..=0x303E
        | 0x3041..=0x33FF
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xA000..=0xA4CF
        | 0xAC00..=0xD7A3
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE6F
        | 0xFF00..=0xFF60
        | 0xFFE0..=0xFFE6
        | 0x1F300..=0x1F64F
        | 0x1F900..=0x1F9FF
        | 0x20000..=0x3FFFD
    );
    if wide {
        2
    } else if matches!(cp, 0x0300..=0x036F | 0x200B..=0x200F) {
        0 // combining / zero-width
    } else {
        1
    }
}

// ---------------------------------------------------------------------------
// Cursor & selection
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorShape {
    #[default]
    Block,
    Underline,
    Bar,
}

/// Grid position (row, col), both zero-based.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Pos {
    pub row: u16,
    pub col: u16,
}

/// Selection start/end + how it was made (#23 copy/paste support).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionMode {
    Mouse,
    Keyboard,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    pub start: Pos,
    pub end: Pos,
    pub mode: SelectionMode,
}

impl Selection {
    /// Normalized (top-left, bottom-right) bounds.
    pub fn bounds(&self) -> (Pos, Pos) {
        let (a, b) = (self.start, self.end);
        let (top, bottom) = if a.row <= b.row { (a, b) } else { (b, a) };
        (
            Pos {
                row: top.row,
                col: top.col.min(bottom.col),
            },
            Pos {
                row: bottom.row,
                col: top.col.max(bottom.col),
            },
        )
    }
}

/// Terminal cursor: position + shape + visibility (DECTCEM).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor {
    pub row: u16,
    pub col: u16,
    pub shape: CursorShape,
    pub visible: bool,
}

impl Default for Cursor {
    fn default() -> Self {
        Self {
            row: 0,
            col: 0,
            shape: CursorShape::Block,
            visible: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Scrollback
// ---------------------------------------------------------------------------

/// Ring buffer of scrolled-off lines (feature matrix #21).
#[derive(Debug, Clone)]
pub struct Scrollback {
    lines: VecDeque<Vec<Cell>>,
    capacity: usize,
}

impl Scrollback {
    pub fn new(capacity: usize) -> Self {
        Self {
            lines: VecDeque::with_capacity(capacity.min(1024)),
            capacity,
        }
    }

    pub fn push(&mut self, line: Vec<Cell>) {
        if self.capacity == 0 {
            return;
        }
        if self.lines.len() == self.capacity {
            self.lines.pop_front();
        }
        self.lines.push_back(line);
    }

    /// Restore the most recently scrolled-off line (scroll-down path).
    pub fn pop(&mut self) -> Option<Vec<Cell>> {
        self.lines.pop_back()
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Vec<Cell>> {
        self.lines.iter()
    }
}

// ---------------------------------------------------------------------------
// Grid
// ---------------------------------------------------------------------------

/// Active screen (`rows × cols` of cells) with scrollback and tab stops.
pub struct Grid {
    cols: u16,
    rows: u16,
    cells: Vec<Vec<Cell>>,
    scrollback: Scrollback,
    /// Column tab stops (HTS/TBC).
    tabstops: Vec<bool>,
    /// Inclusive scroll region (DECSTBM); full screen by default.
    scroll_top: u16,
    scroll_bottom: u16,
}

impl std::fmt::Debug for Grid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Grid")
            .field("cols", &self.cols)
            .field("rows", &self.rows)
            .field("scrollback", &self.scrollback.len())
            .finish()
    }
}

impl Grid {
    pub fn new(cols: u16, rows: u16, scrollback_lines: usize) -> Self {
        let mut grid = Self {
            cols,
            rows,
            cells: Vec::new(),
            scrollback: Scrollback::new(scrollback_lines),
            tabstops: vec![],
            scroll_top: 0,
            scroll_bottom: rows.saturating_sub(1),
        };
        for _ in 0..rows {
            grid.cells.push(grid.blank_row());
        }
        grid.reset_tabstops();
        grid
    }

    pub fn cols(&self) -> u16 {
        self.cols
    }

    pub fn rows(&self) -> u16 {
        self.rows
    }

    pub fn scrollback_len(&self) -> usize {
        self.scrollback.len()
    }

    pub fn scroll_region(&self) -> (u16, u16) {
        (self.scroll_top, self.scroll_bottom)
    }

    fn blank_row(&self) -> Vec<Cell> {
        vec![Cell::default(); self.cols as usize]
    }

    /// Resize, preserving content. New columns are blank; shrinking rows
    /// spills bottom rows into scrollback; the region is clamped.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.cols = cols;
        self.reset_tabstops();
        for row in &mut self.cells {
            row.resize(cols as usize, Cell::default());
        }
        while self.cells.len() > rows as usize {
            let line = self.cells.remove(self.cells.len() - 1);
            self.scrollback.push(line);
        }
        while self.cells.len() < rows as usize {
            self.cells.push(self.blank_row());
        }
        self.rows = rows;
        self.scroll_bottom = rows.saturating_sub(1);
        if self.scroll_top >= rows {
            self.scroll_top = 0;
        }
    }

    pub fn get_cell(&self, row: u16, col: u16) -> Option<&Cell> {
        self.cells
            .get(row as usize)
            .and_then(|r| r.get(col as usize))
    }

    pub fn get_cell_mut(&mut self, row: u16, col: u16) -> Option<&mut Cell> {
        self.cells
            .get_mut(row as usize)
            .and_then(|r| r.get_mut(col as usize))
    }

    pub fn row_mut(&mut self, row: u16) -> Option<&mut Vec<Cell>> {
        self.cells.get_mut(row as usize)
    }

    /// Text of one screen row (wide chars included, spacers skipped).
    pub fn row_text(&self, row: u16) -> String {
        let mut out = String::new();
        if let Some(cells) = self.cells.get(row as usize) {
            for cell in cells {
                if cell.width > 0 {
                    out.push(cell.ch);
                }
            }
        }
        out.trim_end().to_string()
    }

    /// Visible screen as text lines (renderer seed / tests).
    pub fn visible_text(&self) -> Vec<String> {
        (0..self.rows).map(|r| self.row_text(r)).collect()
    }

    /// Search scrollback + screen; returns absolute line indices
    /// (scrollback first, then screen rows).
    pub fn search(&self, needle: &str) -> Vec<usize> {
        let mut hits = Vec::new();
        for (i, line) in self.scrollback.iter().enumerate() {
            let text: String = line.iter().filter(|c| c.width > 0).map(|c| c.ch).collect();
            if text.contains(needle) {
                hits.push(i);
            }
        }
        let offset = self.scrollback.len();
        for (i, row) in self.cells.iter().enumerate() {
            let text: String = row.iter().filter(|c| c.width > 0).map(|c| c.ch).collect();
            if text.contains(needle) {
                hits.push(offset + i);
            }
        }
        hits
    }

    /// Render one scrollback line as text (absolute index ≥ scrollback_len
    /// addresses screen rows).
    pub fn line_text_at(&self, absolute: usize) -> Option<String> {
        if absolute < self.scrollback.len() {
            return self
                .scrollback
                .iter()
                .nth(absolute)
                .map(|line| line.iter().filter(|c| c.width > 0).map(|c| c.ch).collect());
        }
        let row = absolute - self.scrollback.len();
        (row < self.rows as usize).then(|| self.row_text(row as u16))
    }

    // -- scrolling ---------------------------------------------------------

    pub fn set_scroll_region(&mut self, top: u16, bottom: u16) {
        if top < bottom && bottom < self.rows {
            self.scroll_top = top;
            self.scroll_bottom = bottom;
        }
    }

    pub fn reset_scroll_region(&mut self) {
        self.scroll_top = 0;
        self.scroll_bottom = self.rows.saturating_sub(1);
    }

    /// Scroll the active (full-screen) region up: top lines → scrollback.
    pub fn scroll_up(&mut self, n: u16) {
        let (top, bottom) = (self.scroll_top, self.scroll_bottom);
        self.scroll_region_up(top, bottom, n);
    }

    /// Scroll down: history is restored first when the region touches the
    /// top of the screen (prompt 2.3: "on scroll down, restore from history").
    pub fn scroll_down(&mut self, n: u16) {
        let (top, bottom) = (self.scroll_top, self.scroll_bottom);
        self.scroll_region_down(top, bottom, n);
    }

    pub fn scroll_region_up(&mut self, top: u16, bottom: u16, n: u16) {
        let (top, bottom) = (
            top.min(self.rows.saturating_sub(1)),
            bottom.min(self.rows.saturating_sub(1)),
        );
        if top > bottom {
            return;
        }
        let region_len = (bottom - top + 1) as usize;
        let n = (n as usize).min(region_len);
        if n == 0 {
            return;
        }
        if top == 0 {
            for index in top as usize..top as usize + n {
                self.scrollback.push(self.cells[index].clone());
            }
        }
        // Rotate rows in place instead of cloning the entire scroll region.
        // This is the common terminal path and keeps scroll allocations
        // proportional to the newly exposed blank rows only.
        let range = top as usize..=bottom as usize;
        self.cells[range].rotate_left(n);
        let blank = self.blank_row();
        for row in bottom as usize + 1 - n..=bottom as usize {
            self.cells[row] = blank.clone();
        }
    }

    pub fn scroll_region_down(&mut self, top: u16, bottom: u16, n: u16) {
        let (top, bottom) = (
            top.min(self.rows.saturating_sub(1)),
            bottom.min(self.rows.saturating_sub(1)),
        );
        if top > bottom {
            return;
        }
        let region_len = (bottom - top + 1) as usize;
        let n = (n as usize).min(region_len);
        if n == 0 {
            return;
        }
        let mut restored: Vec<Vec<Cell>> = Vec::new();
        if top == 0 {
            for _ in 0..n {
                match self.scrollback.pop() {
                    Some(line) => restored.push(line),
                    None => break,
                }
            }
        }
        let range = top as usize..=bottom as usize;
        self.cells[range].rotate_right(n);
        let blank = self.blank_row();
        for index in 0..n {
            self.cells[top as usize + index] = restored
                .get(index)
                .cloned()
                .unwrap_or_else(|| blank.clone());
        }
    }

    /// Erase the active screen (CSI 2 J) and reset the scroll region.
    pub fn clear(&mut self) {
        let blank = vec![Cell::default(); self.cols as usize];
        for row in &mut self.cells {
            *row = blank.clone();
        }
        self.reset_scroll_region();
    }

    // -- tab stops ---------------------------------------------------------

    fn reset_tabstops(&mut self) {
        // Default: every 8 columns (CTDEC).
        self.tabstops = (0..self.cols).map(|c| c % 8 == 0).collect();
    }

    pub fn set_tabstop(&mut self, col: u16) {
        if (col as usize) < self.tabstops.len() {
            self.tabstops[col as usize] = true;
        }
    }

    pub fn clear_tabstop(&mut self, col: u16) {
        if (col as usize) < self.tabstops.len() {
            self.tabstops[col as usize] = false;
        }
    }

    pub fn clear_all_tabstops(&mut self) {
        self.tabstops.fill(false);
    }

    /// Next tab stop strictly after `from`, else the last column.
    pub fn next_tabstop(&self, from: u16) -> u16 {
        for col in (from + 1)..self.cols {
            if self.tabstops.get(col as usize).copied().unwrap_or(false) {
                return col;
            }
        }
        self.cols.saturating_sub(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_up_moves_lines_into_history() {
        let mut grid = Grid::new(10, 3, 10);
        for i in 0..3 {
            for (col, ch) in format!("line{i}").chars().enumerate() {
                if let Some(cell) = grid.get_cell_mut(i, col as u16) {
                    cell.ch = ch;
                }
            }
            grid.scroll_up(1);
        }
        assert_eq!(grid.scrollback_len(), 3);
        assert_eq!(grid.row_text(0), "");
    }

    #[test]
    fn scroll_down_restores_from_history() {
        let mut grid = Grid::new(10, 3, 10);
        for i in 0..3 {
            if let Some(cell) = grid.get_cell_mut(i, 0) {
                cell.ch = char::from(b'a' + i as u8);
            }
            grid.scroll_up(1);
        }
        grid.scroll_down(1);
        assert_eq!(
            grid.row_text(0),
            "b",
            "latest scrolled line restored at top"
        );
    }

    #[test]
    fn resize_preserves_and_pads() {
        let mut grid = Grid::new(4, 2, 10);
        if let Some(cell) = grid.get_cell_mut(0, 0) {
            cell.ch = 'x';
        }
        grid.resize(6, 4);
        assert_eq!(grid.cols(), 6);
        assert_eq!(grid.rows(), 4);
        assert_eq!(grid.get_cell(0, 0).unwrap().ch, 'x');
        assert_eq!(grid.get_cell(0, 5).unwrap().ch, ' ');

        // Shrink spills bottom rows to history.
        grid.resize(6, 1);
        assert!(grid.scrollback_len() >= 1);
    }

    #[test]
    fn search_spans_scrollback_and_screen() {
        let mut grid = Grid::new(20, 2, 10);
        if let Some(cell) = grid.get_cell_mut(1, 0) {
            cell.ch = 'e';
            grid.get_cell_mut(1, 1).unwrap().ch = 'r';
        }
        assert_eq!(
            grid.search("er"),
            vec![1],
            "screen row 1 follows scrollback"
        );
    }

    #[test]
    fn tabstops_advance_by_eight() {
        let grid = Grid::new(80, 24, 0);
        assert_eq!(grid.next_tabstop(0), 8);
        assert_eq!(grid.next_tabstop(7), 8);
        assert_eq!(grid.next_tabstop(79), 79, "clamped to last column");
    }

    #[test]
    fn selection_bounds_normalize() {
        let selection = Selection {
            start: Pos { row: 5, col: 9 },
            end: Pos { row: 2, col: 3 },
            mode: SelectionMode::Mouse,
        };
        let (a, b) = selection.bounds();
        assert_eq!(a, Pos { row: 2, col: 3 });
        assert_eq!(b, Pos { row: 5, col: 9 });
    }
}

//! Fixture-driven terminal rendering checks (verification checklist:
//! VT100 output, 256-color, and true color, using the repo's fixture
//! scripts — never production output).
//!
//! Each fixture is a deterministic shell script (no dates, no network).
//! The test runs it twice to prove determinism, feeds the bytes into a
//! real [`Terminal`](mbxt_terminal::Terminal), and asserts anchor cells.
//! Pixel-level proof stays a display-only MISS (see final_report.md).

use mbxt_terminal::grid::Color;
use mbxt_terminal::Terminal;
use std::path::{Path, PathBuf};

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn run_fixture(name: &str) -> Vec<u8> {
    let output = std::process::Command::new("sh")
        .arg(fixture_path(name))
        .output()
        .unwrap_or_else(|err| panic!("cannot run fixture {name}: {err}"));
    assert!(
        output.status.success(),
        "fixture {name} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

fn cell_char(term: &Terminal, row: u16, col: u16) -> char {
    term.get_cell(row, col)
        .unwrap_or_else(|| panic!("missing cell ({row},{col})"))
        .ch
}

#[test]
fn vt100_cursor_addressing_and_erase() {
    let bytes = run_fixture("terminal_vt100.sh");
    // Deterministic: two runs produce identical output.
    assert_eq!(bytes, run_fixture("terminal_vt100.sh"));

    let mut term = Terminal::new(10, 4, 0);
    term.write_bytes(&bytes);

    // CUP writes landed 1-based → 0-based.
    assert_eq!(cell_char(&term, 0, 0), 'T');
    assert_eq!(cell_char(&term, 0, 1), 'O');
    assert_eq!(cell_char(&term, 0, 2), 'P');
    assert_eq!(cell_char(&term, 3, 0), 'B');
    assert_eq!(cell_char(&term, 3, 1), 'O');
    assert_eq!(cell_char(&term, 3, 2), 'T');
    // EL (mode 0) cleared row 1 from column 1; column 0 survived.
    assert_eq!(cell_char(&term, 1, 0), 'X');
    assert_eq!(cell_char(&term, 1, 1), ' ');
    assert_eq!(cell_char(&term, 1, 2), ' ');
    // Untouched cells are blank.
    assert_eq!(cell_char(&term, 2, 0), ' ');
}

#[test]
fn vt100_scroll_overflow_discards_top() {
    let bytes = run_fixture("terminal_vt100_scroll.sh");
    assert_eq!(bytes, run_fixture("terminal_vt100_scroll.sh"));

    let mut term = Terminal::new(10, 4, 0);
    term.write_bytes(&bytes);

    assert_eq!(cell_char(&term, 0, 0), 'S');
    assert_eq!(cell_char(&term, 0, 1), '3');
    assert_eq!(cell_char(&term, 1, 1), '4');
    assert_eq!(cell_char(&term, 2, 1), '5');
    assert_eq!(cell_char(&term, 3, 0), ' ');
}

#[test]
fn sgr_colors_and_attributes() {
    let bytes = run_fixture("terminal_colors.sh");
    assert_eq!(bytes, run_fixture("terminal_colors.sh"));

    let mut term = Terminal::new(10, 2, 0);
    term.write_bytes(&bytes);

    let cell = |row: u16, col: u16| {
        term.get_cell(row, col)
            .unwrap_or_else(|| panic!("missing cell ({row},{col})"))
            .clone()
    };

    // Bold 16-color red, then reset on the next cell is covered by A.
    let red = cell(0, 0);
    assert_eq!((red.ch, red.fg), ('R', Color::Indexed(1)));
    assert!(red.attrs.bold);
    // 256-color foreground / background.
    assert_eq!(cell(0, 1).fg, Color::Indexed(196));
    assert_eq!(cell(0, 2).bg, Color::Indexed(21));
    // Truecolor foreground / background.
    assert_eq!(cell(0, 3).fg, Color::Rgb(10, 20, 30));
    assert_eq!(cell(0, 4).bg, Color::Rgb(200, 150, 100));
    // Italic + underline survive together.
    let italic = cell(0, 5);
    assert_eq!(italic.ch, 'E');
    assert!(italic.attrs.italic && italic.attrs.underline);
    // Reverse video flag is stored on the cell.
    assert!(cell(0, 6).attrs.reverse);
    // Green from the 16-color range resolves as indexed, not default
    // (SGR 32 is the third base color: 30 -> 0, 31 -> 1, 32 -> 2).
    assert_eq!(cell(0, 7).fg, Color::Indexed(2));
}

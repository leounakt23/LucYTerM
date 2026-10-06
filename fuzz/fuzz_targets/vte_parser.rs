#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    let mut terminal = mbxt_terminal::Terminal::new(80, 24, 256);
    terminal.write_bytes(input);
});

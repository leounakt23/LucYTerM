//! Telnet IAC parser (Prompt 4.4 output path).
//!
//! Canonical implementation lives in `mbxt-connections::telnet` (shared by
//! the actor transport); this module re-exports it plus the terminal-feeding
//! helper used by [`super::session::TelnetSession`].

pub use mbxt_connections::telnet::{
    escape_iac, TelnetParser, DEFAULT_TELNET_PORT, TERMINAL_TYPE_NAME,
};

/// Feed socket bytes: strip negotiation, push clean data into the terminal,
/// and return any negotiation replies to transmit.
pub fn feed_terminal(
    parser: &mut TelnetParser,
    terminal: &mut mbxt_terminal::Terminal,
    bytes: &[u8],
) -> Vec<u8> {
    let (clean, replies) = parser.feed(bytes);
    if !clean.is_empty() {
        terminal.write_bytes(&clean);
    }
    replies
}

#[cfg(test)]
mod tests {
    use super::*;
    use mbxt_connections::telnet::{DO, IAC, OPT_ECHO, WILL};

    #[test]
    fn parser_feeds_terminal_and_answers() {
        let mut parser = TelnetParser::new();
        let mut terminal = mbxt_terminal::Terminal::new(20, 5, 0);
        // WILL ECHO → DO ECHO reply, no terminal output.
        let replies = feed_terminal(&mut parser, &mut terminal, &[IAC, WILL, OPT_ECHO]);
        assert_eq!(replies, vec![IAC, DO, OPT_ECHO]);
        // Plain text lands in the grid.
        let replies = feed_terminal(&mut parser, &mut terminal, b"hi");
        assert!(replies.is_empty());
        assert_eq!(terminal.grid.row_text(0).trim_end(), "hi");
    }
}

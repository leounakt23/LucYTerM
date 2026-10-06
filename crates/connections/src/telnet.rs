//! Telnet transport (Prompt 4.4, feature matrix #2): IAC negotiation,
//! NAWS window-size sync, and a clean byte stream for the terminal.
//!
//! Client stance: `WILL NAWS`, `WILL TERMINAL-TYPE`, `DO/DONT ECHO` accepted
//! as offered, `DO SUPPRESS-GO-AHEAD`; everything else refused (`WONT` /
//! `DONT`). `TERMINAL-TYPE SEND` is answered with `xterm-256color` (matches
//! the SSH PTY request). Outbound `0xFF` bytes are IAC-doubled.

use async_trait::async_trait;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::{ConnError, Connection, ConnectionAuth, ConnectionEvent, SessionSpec, TerminalSize};

// ---------------------------------------------------------------------------
// Protocol constants (RFC 854/855/857/858/1073/1091)
// ---------------------------------------------------------------------------

pub const IAC: u8 = 255;
pub const DONT: u8 = 254;
pub const DO: u8 = 253;
pub const WONT: u8 = 252;
pub const WILL: u8 = 251;
pub const SB: u8 = 250;
pub const SE: u8 = 240;

pub const OPT_TRANSMIT_BINARY: u8 = 0;
pub const OPT_ECHO: u8 = 1;
pub const OPT_SUPPRESS_GO_AHEAD: u8 = 3;
pub const OPT_TERMINAL_TYPE: u8 = 24;
pub const OPT_NAWS: u8 = 31;

pub const TTYPE_SEND: u8 = 1;
pub const TTYPE_IS: u8 = 0;

pub const TERMINAL_TYPE_NAME: &[u8] = b"xterm-256color";
pub const DEFAULT_TELNET_PORT: u16 = 23;

// ---------------------------------------------------------------------------
// Incremental IAC parser
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum ParseState {
    #[default]
    Data,
    Iac,
    Command(u8),
    SubnegotiationOption,
    Subnegotiation(u8),
    SubnegotiationIac(u8),
}

/// Streaming Telnet parser: `feed` consumes bytes, returning clean terminal
/// data and any negotiation replies to transmit. Split sequences across
/// `feed` calls are handled (subnegotiation buffers until `IAC SE`).
#[derive(Debug, Default)]
pub struct TelnetParser {
    state: ParseState,
    clean: Vec<u8>,
    replies: Vec<u8>,
    subnegotiation: Vec<u8>,
}

impl TelnetParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Initial client stance (`WILL NAWS/TTYPE`, `DO SGA`).
    pub fn opening_negotiation() -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&[IAC, WILL, OPT_NAWS]);
        out.extend_from_slice(&[IAC, WILL, OPT_TERMINAL_TYPE]);
        out.extend_from_slice(&[IAC, DO, OPT_SUPPRESS_GO_AHEAD]);
        out
    }

    /// NAWS subnegotiation for `cols` × `rows`.
    pub fn naws_message(cols: u16, rows: u16) -> Vec<u8> {
        let (cols, rows) = (cols.to_be_bytes(), rows.to_be_bytes());
        vec![
            IAC, SB, OPT_NAWS, cols[0], cols[1], rows[0], rows[1], IAC, SE,
        ]
    }

    /// Feed bytes; returns `(clean_data, replies)`.
    pub fn feed(&mut self, bytes: &[u8]) -> (Vec<u8>, Vec<u8>) {
        for byte in bytes {
            self.step(*byte);
        }
        (
            std::mem::take(&mut self.clean),
            std::mem::take(&mut self.replies),
        )
    }

    fn step(&mut self, byte: u8) {
        match self.state {
            ParseState::Data => {
                if byte == IAC {
                    self.state = ParseState::Iac;
                } else {
                    self.clean.push(byte);
                }
            },
            ParseState::Iac => match byte {
                IAC => {
                    self.clean.push(IAC);
                    self.state = ParseState::Data;
                },
                DO | DONT | WILL | WONT => {
                    self.state = ParseState::Command(byte);
                },
                SB => {
                    self.subnegotiation.clear();
                    self.state = ParseState::SubnegotiationOption;
                },
                _ => {
                    // NOP/DM/BRK/IP/AO/AYT/EC/EL/GA and friends: strip.
                    self.state = ParseState::Data;
                },
            },
            ParseState::Command(command) => {
                self.answer_negotiation(command, byte);
                self.state = ParseState::Data;
            },
            ParseState::SubnegotiationOption => {
                // First byte after SB is the option number.
                self.state = ParseState::Subnegotiation(byte);
            },
            ParseState::Subnegotiation(option) => {
                if byte == IAC {
                    self.state = ParseState::SubnegotiationIac(option);
                } else {
                    self.subnegotiation.push(byte);
                }
            },
            ParseState::SubnegotiationIac(option) => match byte {
                IAC => {
                    self.subnegotiation.push(IAC);
                    self.state = ParseState::Subnegotiation(option);
                },
                SE => {
                    let payload = std::mem::take(&mut self.subnegotiation);
                    self.answer_subnegotiation(option, payload);
                    self.state = ParseState::Data;
                },
                _ => {
                    // Malformed escape: drop the subnegotiation, resync.
                    self.subnegotiation.clear();
                    self.state = ParseState::Data;
                },
            },
        }
    }

    fn answer_negotiation(&mut self, command: u8, option: u8) {
        let supported = matches!(option, OPT_SUPPRESS_GO_AHEAD | OPT_TERMINAL_TYPE | OPT_NAWS);
        let wanted = matches!(option, OPT_SUPPRESS_GO_AHEAD | OPT_ECHO);
        let reply = match command {
            DO if supported => WILL,
            DO => WONT,
            DONT => WONT,
            WILL if wanted => DO,
            WILL => DONT,
            WONT => DONT,
            _ => return,
        };
        self.replies.extend_from_slice(&[IAC, reply, option]);
    }

    fn answer_subnegotiation(&mut self, option: u8, payload: Vec<u8>) {
        // `TERMINAL-TYPE SEND` → `IS "xterm-256color"`. Anything else
        // (including stray NAWS from the peer) is ignored.
        if option == OPT_TERMINAL_TYPE && payload.first() == Some(&TTYPE_SEND) {
            self.replies
                .extend_from_slice(&[IAC, SB, OPT_TERMINAL_TYPE, TTYPE_IS]);
            self.replies.extend_from_slice(TERMINAL_TYPE_NAME);
            self.replies.extend_from_slice(&[IAC, SE]);
        }
    }
}

/// Double outbound `0xFF` bytes (IAC escaping on send).
pub fn escape_iac(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    for byte in data {
        out.push(*byte);
        if *byte == IAC {
            out.push(IAC);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Transport
// ---------------------------------------------------------------------------

/// Telnet connection over TCP (legacy devices; no authentication — the
/// `auth` argument to [`Connection::start`] is accepted and ignored).
pub struct TelnetConn {
    spec: SessionSpec,
    stream: Option<tokio::net::TcpStream>,
    parser: TelnetParser,
    pending: Vec<u8>,
    size: TerminalSize,
}

impl std::fmt::Debug for TelnetConn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TelnetConn")
            .field("spec", &self.spec)
            .finish_non_exhaustive()
    }
}

impl TelnetConn {
    pub fn new(spec: &SessionSpec) -> Self {
        Self {
            spec: spec.clone(),
            stream: None,
            parser: TelnetParser::new(),
            pending: Vec::new(),
            size: TerminalSize::default(),
        }
    }
}

#[async_trait]
impl Connection for TelnetConn {
    async fn start(&mut self, _auth: ConnectionAuth, size: TerminalSize) -> Result<(), ConnError> {
        let host = self
            .spec
            .host
            .clone()
            .ok_or(ConnError::InvalidSession("telnet host is missing"))?;
        let port = self.spec.port.unwrap_or(DEFAULT_TELNET_PORT);
        let mut stream = tokio::net::TcpStream::connect((host.as_str(), port)).await?;
        stream
            .write_all(&TelnetParser::opening_negotiation())
            .await?;
        // Sync the window before the first prompt (NAWS requirement).
        self.size = size;
        stream
            .write_all(&TelnetParser::naws_message(size.cols, size.rows))
            .await?;
        self.stream = Some(stream);
        tracing::info!(session = %self.spec.name, %host, port, "telnet connected");
        Ok(())
    }

    async fn write(&mut self, data: &[u8]) -> Result<(), ConnError> {
        let stream = self.stream.as_mut().ok_or(ConnError::RemoteClosed)?;
        stream.write_all(&escape_iac(data)).await?;
        Ok(())
    }

    async fn resize(&mut self, size: TerminalSize) -> Result<(), ConnError> {
        self.size = size;
        if let Some(stream) = self.stream.as_mut() {
            stream
                .write_all(&TelnetParser::naws_message(size.cols, size.rows))
                .await?;
        }
        Ok(())
    }

    async fn next_event(&mut self) -> Result<ConnectionEvent, ConnError> {
        loop {
            if !self.pending.is_empty() {
                return Ok(ConnectionEvent::Output(std::mem::take(&mut self.pending)));
            }
            let stream = self.stream.as_mut().ok_or(ConnError::RemoteClosed)?;
            let mut chunk = [0u8; 8192];
            let count = stream.read(&mut chunk).await?;
            if count == 0 {
                return Ok(ConnectionEvent::Eof);
            }
            let (clean, replies) = self.parser.feed(&chunk[..count]);
            if !replies.is_empty() {
                let stream = self.stream.as_mut().ok_or(ConnError::RemoteClosed)?;
                stream.write_all(&replies).await?;
            }
            self.pending.extend_from_slice(&clean);
        }
    }

    async fn shutdown(&mut self) -> Result<(), ConnError> {
        self.stream.take();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negotiation_stance() {
        let mut parser = TelnetParser::new();
        // Server: DO SGA, DO TTYPE, DO BINARY, WILL ECHO, WILL SGA, WILL X.
        let (_, replies) = parser.feed(&[
            IAC,
            DO,
            OPT_SUPPRESS_GO_AHEAD,
            IAC,
            DO,
            OPT_TERMINAL_TYPE,
            IAC,
            DO,
            OPT_TRANSMIT_BINARY,
            IAC,
            WILL,
            OPT_ECHO,
            IAC,
            WILL,
            OPT_SUPPRESS_GO_AHEAD,
            IAC,
            WILL,
            99,
        ]);
        assert_eq!(
            replies,
            vec![
                IAC,
                WILL,
                OPT_SUPPRESS_GO_AHEAD,
                IAC,
                WILL,
                OPT_TERMINAL_TYPE,
                IAC,
                WONT,
                OPT_TRANSMIT_BINARY,
                IAC,
                DO,
                OPT_ECHO,
                IAC,
                DO,
                OPT_SUPPRESS_GO_AHEAD,
                IAC,
                DONT,
                99,
            ]
        );
        // DONT/WONT always refuse back.
        let (_, replies) = parser.feed(&[IAC, DONT, OPT_ECHO, IAC, WONT, OPT_ECHO]);
        assert_eq!(replies, vec![IAC, WONT, OPT_ECHO, IAC, DONT, OPT_ECHO]);
    }

    #[test]
    fn terminal_type_send_answered() {
        let mut parser = TelnetParser::new();
        let (clean, replies) = parser.feed(&[IAC, SB, OPT_TERMINAL_TYPE, TTYPE_SEND, IAC, SE]);
        assert!(clean.is_empty());
        let mut expected = vec![IAC, SB, OPT_TERMINAL_TYPE, TTYPE_IS];
        expected.extend_from_slice(TERMINAL_TYPE_NAME);
        expected.extend_from_slice(&[IAC, SE]);
        assert_eq!(replies, expected);
    }

    #[test]
    fn data_path_strips_commands_and_unescapes() {
        let mut parser = TelnetParser::new();
        // "hi" + IAC IAC (literal FF) + GA (stripped) + "!" split across feeds.
        let (clean, _) = parser.feed(&[b'h', b'i', IAC, IAC, IAC]);
        assert_eq!(clean, vec![b'h', b'i', IAC]);
        let (clean, replies) = parser.feed(&[250 - 1, b'!']);
        assert_eq!(clean, vec![b'!'], "GA terminator stripped");
        assert!(replies.is_empty());
    }

    #[test]
    fn subnegotiation_split_across_feeds() {
        let mut parser = TelnetParser::new();
        let (clean, _) = parser.feed(&[IAC, SB, OPT_TERMINAL_TYPE]);
        assert!(clean.is_empty());
        let (_, replies) = parser.feed(&[TTYPE_SEND, IAC, SE]);
        assert!(replies.starts_with(&[IAC, SB, OPT_TERMINAL_TYPE, TTYPE_IS]));
    }

    #[test]
    fn naws_message_layout() {
        assert_eq!(
            TelnetParser::naws_message(80, 24),
            vec![IAC, SB, OPT_NAWS, 0, 80, 0, 24, IAC, SE]
        );
        assert_eq!(
            TelnetParser::opening_negotiation(),
            vec![
                IAC,
                WILL,
                OPT_NAWS,
                IAC,
                WILL,
                OPT_TERMINAL_TYPE,
                IAC,
                DO,
                OPT_SUPPRESS_GO_AHEAD
            ]
        );
    }

    #[test]
    fn outbound_iac_doubled() {
        assert_eq!(escape_iac(&[1, IAC, 2]), vec![1, IAC, IAC, 2]);
    }

    #[tokio::test]
    async fn transport_echoes_through_mock_server() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            // Server asks for the terminal type; client must answer.
            socket
                .write_all(&[IAC, DO, OPT_TERMINAL_TYPE])
                .await
                .unwrap();
            let mut seen = Vec::new();
            let mut buf = [0u8; 256];
            // Drain the opening stance + NAWS (9 + 9 bytes; the client sends
            // both from `start` before its first read).
            let deadline = tokio::time::sleep(std::time::Duration::from_secs(2));
            tokio::pin!(deadline);
            loop {
                tokio::select! {
                    result = socket.read(&mut buf) => {
                        match result {
                            Ok(0) => break,
                            Ok(count) => {
                                seen.extend_from_slice(&buf[..count]);
                                if seen.len() >= 18 {
                                    break;
                                }
                            },
                            Err(_) => break,
                        }
                    },
                    _ = &mut deadline => break,
                }
            }
            // Now trigger the TTYPE answer; it arrives once the client pumps
            // reads concurrently with this drain.
            socket
                .write_all(&[IAC, SB, OPT_TERMINAL_TYPE, TTYPE_SEND, IAC, SE])
                .await
                .unwrap();
            let deadline = tokio::time::sleep(std::time::Duration::from_secs(2));
            tokio::pin!(deadline);
            loop {
                tokio::select! {
                    result = socket.read(&mut buf) => {
                        match result {
                            Ok(0) => break,
                            Ok(count) => {
                                seen.extend_from_slice(&buf[..count]);
                                if seen
                                    .windows(TERMINAL_TYPE_NAME.len())
                                    .any(|w| w == TERMINAL_TYPE_NAME)
                                {
                                    break;
                                }
                            },
                            Err(_) => break,
                        }
                    },
                    _ = &mut deadline => break,
                }
            }
            assert!(
                seen.windows(TERMINAL_TYPE_NAME.len())
                    .any(|w| w == TERMINAL_TYPE_NAME),
                "client answered TTYPE SEND"
            );
            // NAWS must precede input (cols 80 x rows 24).
            assert!(seen
                .windows(9)
                .any(|w| w == [IAC, SB, OPT_NAWS, 0, 80, 0, 24, IAC, SE]));
            socket.write_all(b"login:").await.unwrap();
            // Echo one client write back, then close.
            let count = socket.read(&mut buf).await.unwrap();
            socket.write_all(&buf[..count]).await.unwrap();
        });

        let spec = SessionSpec {
            name: "legacy".into(),
            protocol: mbxt_core::Protocol::Telnet,
            host: Some("127.0.0.1".into()),
            port: Some(address.port()),
            username: None,
            auth: mbxt_core::AuthMethod::Password,
            tags: vec![],
            notes: String::new(),
            x11_forwarding: false,
            serial: None,
            forwards: Vec::new(),
        };
        let mut conn = TelnetConn::new(&spec);
        conn.start(
            ConnectionAuth::Password(zeroize::Zeroizing::new(String::new())),
            TerminalSize {
                cols: 80,
                rows: 24,
                pixel_width: 0,
                pixel_height: 0,
            },
        )
        .await
        .unwrap();
        // Server greeting arrives as clean output.
        match conn.next_event().await.unwrap() {
            ConnectionEvent::Output(bytes) => assert_eq!(bytes, b"login:"),
            other => panic!("expected greeting, got {other:?}"),
        }
        conn.write(b"root\r\n").await.unwrap();
        match conn.next_event().await.unwrap() {
            ConnectionEvent::Output(bytes) => assert_eq!(bytes, b"root\r\n"),
            other => panic!("expected echo, got {other:?}"),
        }
        conn.shutdown().await.unwrap();
        server.abort();
    }
}

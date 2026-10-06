//! `TelnetSession`: direct TCP Telnet session with an owned terminal
//! (Prompt 4.4 output path).
//!
//! The actor path (`mbxt-connections::telnet::TelnetConn`) is what the UI
//! connects through; this struct is the same protocol over an explicit
//! handle for embedding, scripting, and headless tests (mock-server
//! round trip, no UART hardware needed).

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use super::parser::{feed_terminal, TelnetParser};
use super::TelnetError;

/// Direct Telnet session: socket + parser + terminal.
pub struct TelnetSession {
    stream: TcpStream,
    parser: TelnetParser,
    terminal: mbxt_terminal::Terminal,
}

impl std::fmt::Debug for TelnetSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TelnetSession").finish_non_exhaustive()
    }
}

impl TelnetSession {
    /// Connect and run the opening negotiation (`WILL NAWS/TTYPE`, `DO SGA`,
    /// initial NAWS for `cols` × `rows`).
    pub async fn connect(
        host: &str,
        port: u16,
        cols: u16,
        rows: u16,
        scrollback_lines: usize,
    ) -> Result<Self, TelnetError> {
        let mut stream = TcpStream::connect((host, port))
            .await
            .map_err(TelnetError::io)?;
        stream
            .write_all(&TelnetParser::opening_negotiation())
            .await
            .map_err(TelnetError::io)?;
        stream
            .write_all(&TelnetParser::naws_message(cols, rows))
            .await
            .map_err(TelnetError::io)?;
        Ok(Self {
            stream,
            parser: TelnetParser::new(),
            terminal: mbxt_terminal::Terminal::new(cols, rows, scrollback_lines),
        })
    }

    /// Send terminal input (IAC-doubled on the wire).
    pub async fn send_data(&mut self, data: &[u8]) -> Result<(), TelnetError> {
        self.stream
            .write_all(&super::parser::escape_iac(data))
            .await
            .map_err(TelnetError::io)
    }

    /// One read iteration: socket → terminal, negotiation answered inline.
    /// Returns `true` on remote EOF.
    pub async fn poll_once(&mut self) -> Result<bool, TelnetError> {
        let mut chunk = [0u8; 8192];
        let count = self
            .stream
            .read(&mut chunk)
            .await
            .map_err(TelnetError::io)?;
        if count == 0 {
            return Ok(true);
        }
        let replies = feed_terminal(&mut self.parser, &mut self.terminal, &chunk[..count]);
        if !replies.is_empty() {
            self.stream
                .write_all(&replies)
                .await
                .map_err(TelnetError::io)?;
        }
        Ok(false)
    }

    /// Sync the remote window (NAWS) and the local grid.
    pub async fn resize(&mut self, cols: u16, rows: u16) -> Result<(), TelnetError> {
        self.terminal.resize(rows, cols);
        self.stream
            .write_all(&TelnetParser::naws_message(cols, rows))
            .await
            .map_err(TelnetError::io)
    }

    /// Owned terminal (grid inspection, tests, embedding).
    pub fn terminal(&self) -> &mbxt_terminal::Terminal {
        &self.terminal
    }

    /// Mutable terminal (selection, Bell drain).
    pub fn terminal_mut(&mut self) -> &mut mbxt_terminal::Terminal {
        &mut self.terminal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn session_round_trips_through_mock_server() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            // Drain the opening stance + NAWS (9 + 9 bytes).
            let mut seen = Vec::new();
            let mut buf = [0u8; 256];
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
            assert!(seen.len() >= 18, "opening negotiation + NAWS sent");
            socket.write_all(b"Password:").await.unwrap();
            let count = socket.read(&mut buf).await.unwrap();
            socket.write_all(&buf[..count]).await.unwrap();
        });

        let mut session = TelnetSession::connect("127.0.0.1", address.port(), 80, 24, 0)
            .await
            .unwrap();
        // Greeting lands in the grid.
        assert!(!session.poll_once().await.unwrap());
        assert_eq!(session.terminal().grid.row_text(0).trim_end(), "Password:");
        // Echo round trip.
        session.send_data(b"secret\r\n").await.unwrap();
        assert!(!session.poll_once().await.unwrap());
        assert!(session.terminal().grid.row_text(0).contains("secret"));
        session.resize(100, 30).await.unwrap();
        assert_eq!(session.terminal().grid.cols(), 100);
        server.abort();
    }
}

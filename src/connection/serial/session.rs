//! `SerialSession`: direct serial-port handle with an owned terminal
//! (Prompt 4.4 output path).
//!
//! The actor path (`mbxt-connections::serial::SerialConn`) is what the UI
//! connects through; this struct is the same port over an explicit handle
//! for embedding and headless tests (unix pty pairs stand in for UART
//! hardware — no adapter needed in CI).

use super::config::SerialConfig;
use super::SerialError;

/// Direct serial session: port + terminal. Raw bytes, no negotiation.
pub struct SerialSession {
    port: Box<dyn serialport::SerialPort>,
    terminal: mbxt_terminal::Terminal,
}

impl std::fmt::Debug for SerialSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SerialSession").finish_non_exhaustive()
    }
}

impl SerialSession {
    /// Open `config` on the calling thread (fast: no I/O beyond `open`).
    pub fn open(config: &SerialConfig, scrollback_lines: usize) -> Result<Self, SerialError> {
        let port = mbxt_connections::serial::open_port(&config.params)
            .map_err(|err| SerialError::io(err.to_string()))?;
        Ok(Self::from_port(port, scrollback_lines))
    }

    /// Wrap an already-open port (pty pairs in tests, handoff in embedding).
    pub fn from_port(port: Box<dyn serialport::SerialPort>, scrollback_lines: usize) -> Self {
        Self {
            port,
            terminal: mbxt_terminal::Terminal::new(80, 24, scrollback_lines),
        }
    }

    /// Send terminal input to the device.
    pub fn send_data(&mut self, data: &[u8]) -> Result<(), SerialError> {
        use std::io::Write as _;
        self.port.write_all(data).map_err(SerialError::io)
    }

    /// Drain one read (timeout-bounded by the port settings) into the
    /// terminal. Returns bytes consumed (`0` on timeout tick).
    pub fn poll_once(&mut self) -> Result<usize, SerialError> {
        use std::io::Read as _;
        let mut chunk = [0u8; 4096];
        match self.port.read(&mut chunk) {
            Ok(count) => {
                self.terminal.write_bytes(&chunk[..count]);
                Ok(count)
            },
            Err(err) if err.kind() == std::io::ErrorKind::TimedOut => Ok(0),
            Err(err) => Err(SerialError::io(err)),
        }
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
    use serialport::SerialPort;

    #[cfg(unix)]
    fn config_for(device: &str) -> SerialConfig {
        SerialConfig::new(mbxt_core::SerialParams {
            device: device.to_string(),
            baud_rate: 115200,
            ..mbxt_core::SerialParams::default()
        })
        .unwrap()
    }

    #[test]
    fn open_rejects_invalid_config() {
        let bad = mbxt_core::SerialParams::default();
        assert!(SerialConfig::new(bad).is_err());
    }

    /// Loopback over a unix pty pair (no hardware): device bytes land in the
    /// grid, session bytes land on the master side.
    #[cfg(unix)]
    #[test]
    fn session_loops_back_over_pty() {
        use std::io::{Read, Write};

        let (mut master, slave) = serialport::TTYPort::pair().expect("pty pair");
        let name = slave.name().expect("pty slave path");
        // `from_port` keeps this test on the session layer (open() is
        // covered by the transport loopback test).
        let mut session = SerialSession::from_port(Box::new(slave), 0);
        master.write_all(b"ready\r\n").unwrap();
        master
            .set_timeout(std::time::Duration::from_secs(2))
            .unwrap();

        // Poll until the greeting arrives (timeout-bounded ticks).
        let mut seen = false;
        for _ in 0..50 {
            session.poll_once().unwrap();
            if session.terminal().grid.row_text(0).contains("ready") {
                seen = true;
                break;
            }
        }
        assert!(seen, "device output reached the grid");
        assert!(config_for(&name).params.device == name);

        session.send_data(b"ping").unwrap();
        let mut echo = [0u8; 4];
        master.read_exact(&mut echo).unwrap();
        assert_eq!(&echo, b"ping");
    }
}

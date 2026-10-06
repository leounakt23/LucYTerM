//! Serial transport (Prompt 4.4, feature matrix #8): RS-232 / USB-serial
//! console access with no negotiation — raw bytes straight to the terminal.
//!
//! The blocking `serialport` API is bridged into the async [`Connection`]
//! surface with a reader thread (`spawn_blocking` + 100 ms polls) feeding a
//! bounded channel; writes go through a short blocking section. Shutdown
//! flips a stop flag and joins the reader (≤ one poll period, no leaks).

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

use std::io::Read as _;

use async_trait::async_trait;
use mbxt_core::{SerialFlowControl, SerialParity};
use serialport::{DataBits, FlowControl, Parity, StopBits};
use tokio::sync::mpsc;

use crate::{ConnError, Connection, ConnectionAuth, ConnectionEvent, SessionSpec, TerminalSize};

/// Map core data-bits to the `serialport` vocabulary.
fn map_data_bits(bits: u8) -> Result<DataBits, ConnError> {
    match bits {
        5 => Ok(DataBits::Five),
        6 => Ok(DataBits::Six),
        7 => Ok(DataBits::Seven),
        8 => Ok(DataBits::Eight),
        _ => Err(ConnError::InvalidSession("serial data bits must be 5–8")),
    }
}

/// Map core parity to the `serialport` vocabulary.
fn map_parity(parity: SerialParity) -> Parity {
    match parity {
        SerialParity::None => Parity::None,
        SerialParity::Odd => Parity::Odd,
        SerialParity::Even => Parity::Even,
    }
}

/// Map core stop bits to the `serialport` vocabulary.
fn map_stop_bits(bits: u8) -> Result<StopBits, ConnError> {
    match bits {
        1 => Ok(StopBits::One),
        2 => Ok(StopBits::Two),
        _ => Err(ConnError::InvalidSession("serial stop bits must be 1–2")),
    }
}

/// Map core flow control to the `serialport` vocabulary.
fn map_flow_control(flow: SerialFlowControl) -> FlowControl {
    match flow {
        SerialFlowControl::None => FlowControl::None,
        SerialFlowControl::Software => FlowControl::Software,
        SerialFlowControl::Hardware => FlowControl::Hardware,
    }
}

/// Open a serial device from validated [`mbxt_core::SerialParams`].
pub fn open_port(
    params: &mbxt_core::SerialParams,
) -> Result<Box<dyn serialport::SerialPort>, ConnError> {
    params.validate().map_err(ConnError::InvalidSession)?;
    serialport::new(&params.device, params.baud_rate)
        .data_bits(map_data_bits(params.data_bits)?)
        .parity(map_parity(params.parity))
        .stop_bits(map_stop_bits(params.stop_bits)?)
        .flow_control(map_flow_control(params.flow_control))
        .timeout(Duration::from_millis(100))
        .open()
        .map_err(|err| ConnError::Io(std::io::Error::other(err.to_string())))
}

/// Human-readable port list for the device picker (`name — type`).
pub fn list_ports() -> Vec<String> {
    serialport::available_ports()
        .map(|ports| {
            ports
                .into_iter()
                .map(|port| format!("{} — {:?}", port.port_name, port.port_type))
                .collect()
        })
        .unwrap_or_default()
}

/// Serial connection (console sessions; `auth` is accepted and ignored —
/// serial lines carry no authentication).
pub struct SerialConn {
    spec: SessionSpec,
    port: Option<Box<dyn serialport::SerialPort>>,
    reader: Option<tokio::task::JoinHandle<()>>,
    stop: Arc<AtomicBool>,
    events: Option<mpsc::Receiver<Vec<u8>>>,
}

impl std::fmt::Debug for SerialConn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SerialConn")
            .field("spec", &self.spec)
            .finish_non_exhaustive()
    }
}

impl SerialConn {
    pub fn new(spec: &SessionSpec) -> Self {
        Self {
            spec: spec.clone(),
            port: None,
            reader: None,
            stop: Arc::new(AtomicBool::new(false)),
            events: None,
        }
    }
}

#[async_trait]
impl Connection for SerialConn {
    async fn start(&mut self, _auth: ConnectionAuth, _size: TerminalSize) -> Result<(), ConnError> {
        let params = self
            .spec
            .serial
            .clone()
            .ok_or(ConnError::InvalidSession("serial settings are missing"))?;
        let port = open_port(&params)?;
        // Clear stale boot output before the UI attaches.
        port.clear(serialport::ClearBuffer::All)
            .map_err(|err| ConnError::Io(std::io::Error::other(err.to_string())))?;
        let mut reader = port
            .try_clone()
            .map_err(|err| ConnError::Io(std::io::Error::other(err.to_string())))?;
        let (tx, rx) = mpsc::channel(256);
        let stop = Arc::clone(&self.stop);
        stop.store(false, Ordering::SeqCst);
        let task = tokio::task::spawn_blocking(move || {
            let mut chunk = [0u8; 4096];
            while !stop.load(Ordering::SeqCst) {
                match reader.read(&mut chunk) {
                    Ok(0) => {
                        // Timeout tick. The driver paces these at the
                        // configured poll period; the yield below only
                        // matters for a zero-timeout driver, which must
                        // still honor the documented poll cadence
                        // instead of busy-spinning.
                        std::thread::sleep(Duration::from_millis(5));
                    },
                    Ok(count) => {
                        if tx.blocking_send(chunk[..count].to_vec()).is_err() {
                            break; // owner gone
                        }
                    },
                    Err(err) if err.kind() == std::io::ErrorKind::BrokenPipe => {
                        // Remote end hung up (pty master closed, device
                        // unplugged for good). Exit so the dropped
                        // sender surfaces as `ConnectionEvent::Eof` in
                        // `next_event`; a persistent hangup must never
                        // look like a transient poll tick.
                        break;
                    },
                    Err(_) => {
                        // Timeouts are the poll tick; other errors are
                        // transient on USB unplug/replug — keep polling
                        // until shutdown flips the flag. Pace the loop
                        // so an instantly-failing driver cannot
                        // busy-spin.
                        std::thread::sleep(Duration::from_millis(10));
                    },
                }
            }
        });
        self.port = Some(port);
        self.reader = Some(task);
        self.events = Some(rx);
        tracing::info!(session = %self.spec.name, device = %params.device, baud = params.baud_rate, "serial opened");
        Ok(())
    }

    async fn write(&mut self, data: &[u8]) -> Result<(), ConnError> {
        use std::io::Write as _;
        // `block_in_place` is multi-thread-only and panics on
        // current-thread runtimes, so writes go through a cloned
        // handle on the blocking pool instead (the documented way to
        // read and write one port simultaneously). This keeps the
        // blocking section short on every runtime flavor.
        let port = self.port.as_ref().ok_or(ConnError::RemoteClosed)?;
        let mut writer = port
            .try_clone()
            .map_err(|err| ConnError::Io(std::io::Error::other(err.to_string())))?;
        let bytes = data.to_vec();
        tokio::task::spawn_blocking(move || writer.write_all(&bytes))
            .await
            .map_err(|err| ConnError::Io(std::io::Error::other(err.to_string())))?
            .map_err(|err| ConnError::Io(std::io::Error::other(err.to_string())))?;
        Ok(())
    }

    async fn resize(&mut self, _size: TerminalSize) -> Result<(), ConnError> {
        // No negotiation on serial lines — window size is terminal-local.
        Ok(())
    }

    async fn next_event(&mut self) -> Result<ConnectionEvent, ConnError> {
        let events = self.events.as_mut().ok_or(ConnError::RemoteClosed)?;
        match events.recv().await {
            Some(bytes) => Ok(ConnectionEvent::Output(bytes)),
            None => Ok(ConnectionEvent::Eof),
        }
    }

    async fn shutdown(&mut self) -> Result<(), ConnError> {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(reader) = self.reader.take() {
            let _ = reader.await;
        }
        self.events.take();
        self.port.take();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serialport::SerialPort;

    #[test]
    fn mappings_cover_the_vocabulary() {
        assert!(map_data_bits(8).is_ok());
        assert!(map_data_bits(4).is_err());
        assert!(map_stop_bits(1).is_ok());
        assert!(map_stop_bits(3).is_err());
        assert_eq!(map_parity(SerialParity::Odd), Parity::Odd);
        assert_eq!(
            map_flow_control(SerialFlowControl::Hardware),
            FlowControl::Hardware
        );
    }

    #[test]
    fn open_rejects_invalid_params_without_touching_hardware() {
        let mut params = mbxt_core::SerialParams::default();
        params.device.clear();
        assert!(open_port(&params).is_err());
        params.device = "/dev/ttyUSB0".into();
        params.data_bits = 4;
        assert!(open_port(&params).is_err());
    }

    #[test]
    fn port_listing_never_fails() {
        // No hardware required: absence of ports is an empty list, not an error.
        let _ = list_ports();
    }

    /// Loopback over a unix pty pair (no hardware): bytes written by the
    /// session arrive on the master side and vice versa.
    #[cfg(unix)]
    #[tokio::test]
    async fn transport_loops_back_over_pty() {
        let (mut master, slave) = serialport::TTYPort::pair().expect("pty pair");
        use std::io::{Read, Write};
        let slave_name = slave.name().unwrap_or_default();
        assert!(!slave_name.is_empty(), "pty slave has a device path");

        let spec = SessionSpec {
            name: "console".into(),
            protocol: mbxt_core::Protocol::Serial,
            host: None,
            port: None,
            username: None,
            auth: mbxt_core::AuthMethod::Password,
            tags: vec![],
            notes: String::new(),
            x11_forwarding: false,
            serial: Some(mbxt_core::SerialParams {
                device: slave_name,
                baud_rate: 115200,
                ..mbxt_core::SerialParams::default()
            }),
            forwards: vec![],
        };
        let mut conn = SerialConn::new(&spec);
        conn.start(
            ConnectionAuth::Password(zeroize::Zeroizing::new(String::new())),
            TerminalSize::default(),
        )
        .await
        .expect("pty open");
        // Device → session.
        master.write_all(b"boot ok\r\n").unwrap();
        match tokio::time::timeout(Duration::from_secs(2), conn.next_event())
            .await
            .expect("event in time")
            .expect("no transport error")
        {
            ConnectionEvent::Output(bytes) => assert!(bytes.windows(7).any(|w| w == b"boot ok")),
            other => panic!("expected output, got {other:?}"),
        }
        // Session → device.
        conn.write(b"ping").await.unwrap();
        master.set_timeout(Duration::from_secs(2)).unwrap();
        let mut echo = [0u8; 4];
        master.read_exact(&mut echo).unwrap();
        assert_eq!(&echo, b"ping");
        conn.shutdown().await.unwrap();
    }
}

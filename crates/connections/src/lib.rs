//! Connection layer: transports, factory (Factory pattern), auth strategies
//! (Strategy pattern). Per `doc/architecture.md` §3.4, new protocols are
//! added by adding one module + one match arm; nothing upstream changes.
//!
//! Wire implementations land in prompt 0.5+; the trait surface and factory
//! are final so upstream code can be written against them now.

use async_trait::async_trait;
use mbxt_core::{AuthMethod, Protocol, SessionSpec};
use std::path::PathBuf;
use zeroize::Zeroizing;

#[derive(Clone)]
pub enum ConnectionAuth {
    Password(Zeroizing<String>),
    KeyFile {
        path: PathBuf,
        passphrase: Option<Zeroizing<String>>,
    },
    Agent,
    KeyboardInteractive(Zeroizing<String>),
}

impl std::fmt::Debug for ConnectionAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Password(_) => f.write_str("Password(<redacted>)"),
            Self::KeyFile { path, .. } => f.debug_tuple("KeyFile").field(path).finish(),
            Self::Agent => f.write_str("Agent"),
            Self::KeyboardInteractive(_) => f.write_str("KeyboardInteractive(<redacted>)"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalSize {
    pub cols: u16,
    pub rows: u16,
    pub pixel_width: u32,
    pub pixel_height: u32,
}

impl Default for TerminalSize {
    fn default() -> Self {
        Self {
            cols: 80,
            rows: 24,
            pixel_width: 0,
            pixel_height: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionEvent {
    Output(Vec<u8>),
    ExitStatus(u32),
    Eof,
}

/// One tunneled TCP stream over the SSH connection (Prompt 5.3).
/// Produced by `direct-tcpip` opens and server `forwarded-tcpip` accepts;
/// both directions implement the async byte-stream traits.
#[cfg(feature = "ssh")]
pub type ForwardStream = russh::ChannelStream<russh::client::Msg>;

/// A server-opened `forwarded-tcpip` channel with its listen endpoint
/// (routes concurrent `-R` forwards sharing one connection).
#[cfg(feature = "ssh")]
pub struct ForwardedTcpIp {
    pub stream: ForwardStream,
    pub connected_host: String,
    pub connected_port: u32,
}

/// A live transport to a remote endpoint.
///
/// Implementations own their byte streams; the session actor wraps this and
/// pumps PTY data onto the event bus.
#[async_trait]
pub trait Connection: Send {
    /// Establish the connection and perform authentication.
    async fn start(&mut self, auth: ConnectionAuth, size: TerminalSize) -> Result<(), ConnError>;
    /// Write raw bytes to the remote side (terminal input).
    async fn write(&mut self, data: &[u8]) -> Result<(), ConnError>;
    async fn resize(&mut self, size: TerminalSize) -> Result<(), ConnError>;
    async fn next_event(&mut self) -> Result<ConnectionEvent, ConnError>;
    /// Graceful shutdown (channel close, PTY release, child reap).
    async fn shutdown(&mut self) -> Result<(), ConnError>;
    /// Run one command and collect its output (Prompt 5.4 remote tools).
    /// Transports without exec channels refuse.
    async fn exec(&mut self, _command: &str) -> Result<String, ConnError> {
        Err(ConnError::Unsupported("exec"))
    }
    /// Open a `direct-tcpip` channel to `host:port` (local/dynamic forward
    /// data path, Prompt 5.3). Transports without tunneling refuse.
    #[cfg(feature = "ssh")]
    async fn open_direct_channel(
        &mut self,
        _host: &str,
        _port: u32,
    ) -> Result<ForwardStream, ConnError> {
        Err(ConnError::Unsupported("port forwarding"))
    }
    /// Ask the server to listen on `bind:port` (`-R`, Prompt 5.3).
    /// Returns the bound port (servers may pick when `port == 0`).
    #[cfg(feature = "ssh")]
    async fn request_remote_forward(&mut self, _bind: &str, _port: u32) -> Result<u32, ConnError> {
        Err(ConnError::Unsupported("port forwarding"))
    }
    /// Release a remote listen (`-R` teardown, best-effort).
    #[cfg(feature = "ssh")]
    async fn cancel_remote_forward(&mut self, _bind: &str, _port: u32) -> Result<(), ConnError> {
        Err(ConnError::Unsupported("port forwarding"))
    }
    /// Sink for server-opened `forwarded-tcpip` channels (`-R` data path,
    /// tagged with the listen endpoint for multi-forward routing).
    /// `None` (default) drops them loudly at the handler.
    #[cfg(feature = "ssh")]
    fn set_forwarded_sink(
        &mut self,
        _sink: Option<tokio::sync::mpsc::UnboundedSender<ForwardedTcpIp>>,
    ) {
    }
}

/// Transport-level errors. Mapped to user-friendly messages in `app`
/// (error handling strategy, architecture §7).
#[derive(Debug, thiserror::Error)]
pub enum ConnError {
    #[error("protocol {0} support not compiled in (enable the corresponding cargo feature)")]
    Unsupported(&'static str),
    #[error("transport not implemented yet (skeleton)")]
    NotYetImplemented,
    #[error("authentication method {0:?} is not valid for this protocol")]
    AuthMismatch(AuthMethod),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid session: {0}")]
    InvalidSession(&'static str),
    #[cfg(feature = "ssh")]
    #[error("SSH error: {0}")]
    Ssh(#[from] russh::Error),
    #[error("SSH key error: {0}")]
    Key(String),
    #[error("server host key is unknown or does not match known_hosts")]
    HostKeyRejected,
    #[error("server rejected authentication")]
    AuthRejected,
    #[error("remote channel closed")]
    RemoteClosed,
}

/// Factory: creates the protocol-specific connection for a session spec.
pub struct ConnectionFactory;

impl ConnectionFactory {
    /// Build the transport matching `spec.protocol` (open/closed principle).
    pub fn create(spec: &SessionSpec) -> Result<Box<dyn Connection>, ConnError> {
        match spec.protocol {
            #[cfg(feature = "ssh")]
            Protocol::Ssh | Protocol::Sftp | Protocol::X11 => Ok(Box::new(ssh::SshConn::new(spec))),
            #[cfg(not(feature = "ssh"))]
            Protocol::Ssh | Protocol::Sftp | Protocol::X11 => Err(ConnError::Unsupported("ssh")),
            #[cfg(feature = "telnet")]
            Protocol::Telnet => Ok(Box::new(telnet::TelnetConn::new(spec))),
            #[cfg(not(feature = "telnet"))]
            Protocol::Telnet => Err(ConnError::Unsupported("telnet")),
            #[cfg(feature = "serial")]
            Protocol::Serial => Ok(Box::new(serial::SerialConn::new(spec))),
            #[cfg(not(feature = "serial"))]
            Protocol::Serial => Err(ConnError::Unsupported("serial")),
            // RDP/VNC/custom: spawn external clients (`freerdp`, VNC viewer,
            // arbitrary binaries) via `fork`/`exec` + PTY capture (#3, #4, #9).
            #[cfg(any(feature = "rdp", feature = "vnc"))]
            Protocol::Rdp | Protocol::Vnc | Protocol::Custom => {
                Ok(Box::new(spawn::SpawnConn::new(spec)))
            },
            #[cfg(not(any(feature = "rdp", feature = "vnc")))]
            Protocol::Rdp | Protocol::Vnc | Protocol::Custom => {
                Err(ConnError::Unsupported("spawn-based protocol"))
            },
            Protocol::Ftp => Err(ConnError::Unsupported("ftp")),
        }
    }
}

/// SSH transport (russh). Wire-up in prompt 0.5: auth strategy chain,
/// shell channel + PTY, forwarding, agent/X11 channels.
#[cfg(feature = "ssh")]
pub mod ssh;

/// Telnet transport (legacy devices; feature matrix #2, prompt 4.4).
#[cfg(feature = "telnet")]
pub mod telnet;

/// Serial transport (feature matrix #8; `serialport` backend, prompt 4.4).
#[cfg(feature = "serial")]
pub mod serial;

/// Spawn-based transport for external clients (RDP/VNC/custom commands).
///
/// Managed child process: the viewer binary is resolved from
/// `MBXT_RDP_CLIENT` / `MBXT_VNC_CLIENT` (or `xfreerdp` / `vncviewer` on
/// `PATH`), spawned with secrets kept off argv (passwords travel over a
/// stdin pipe, never the command line — argv is world-readable via
/// `/proc`), stderr is drained as diagnostic events, and shutdown kills
/// and reaps the child. Window embedding (reparenting the viewer into an
/// app tab) is follow-up work; the `/t` title below exists so a window
/// manager can already identify these sessions.
#[cfg(any(feature = "rdp", feature = "vnc"))]
pub mod spawn {
    use super::*;
    use std::process::Stdio;
    use std::time::Duration;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
    use tokio::process::{Child, ChildStdin, Command};

    const DEFAULT_RDP_CLIENT: &str = "xfreerdp";
    const DEFAULT_VNC_CLIENT: &str = "vncviewer";
    const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

    /// Launch descriptor: program + argv (never containing secrets) plus
    /// an optional password delivered over stdin. Pure constructor, so
    /// tests can assert the argv invariant without spawning anything.
    #[derive(Debug)]
    pub struct SpawnPlan {
        pub program: PathBuf,
        pub args: Vec<String>,
        pub stdin_password: Option<Zeroizing<String>>,
    }

    /// Resolve the viewer binary: an explicit `MBXT_*` override wins,
    /// otherwise search `PATH`. Paths containing a separator must exist;
    /// bare names must be found on `PATH`. Errors name the binary and
    /// the override variable — never fall back silently.
    fn resolve_client(env_var: &str, default: &str, which: &str) -> Result<PathBuf, ConnError> {
        let raw = std::env::var_os(env_var)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(default));
        if raw.components().count() > 1 {
            if raw.is_file() {
                return Ok(raw);
            }
        } else if let Some(found) =
            std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
                .map(|dir| dir.join(&raw))
                .find(|candidate| candidate.is_file())
        {
            return Ok(found);
        }
        Err(ConnError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(
                "{which} client '{}' not found; install it or set {env_var} to its path",
                raw.display()
            ),
        )))
    }

    /// Build the launch plan for one spec. Returns a plan whose argv is
    /// guaranteed secret-free; the password (RDP only) rides stdin.
    pub fn plan_for(spec: &SessionSpec, auth: &ConnectionAuth) -> Result<SpawnPlan, ConnError> {
        match spec.protocol {
            Protocol::Rdp => {
                let host = spec
                    .host
                    .clone()
                    .ok_or(ConnError::InvalidSession("RDP host is missing"))?;
                let port = spec.port.unwrap_or(3389);
                let password = match auth {
                    ConnectionAuth::Password(password) => Some(password.clone()),
                    ConnectionAuth::KeyFile { .. }
                    | ConnectionAuth::Agent
                    | ConnectionAuth::KeyboardInteractive(_) => {
                        return Err(ConnError::AuthMismatch(spec.auth.clone()));
                    },
                };
                let program = resolve_client("MBXT_RDP_CLIENT", DEFAULT_RDP_CLIENT, "RDP")?;
                // xfreerdp 2.x/3.x flags. No /cert-ignore: host trust
                // stays strict. /t pins an identifiable window title.
                let mut args = vec![
                    format!("/v:{host}:{port}"),
                    format!("/t:remote-app RDP {}", spec.name),
                ];
                if let Some(user) = spec.username.as_deref() {
                    args.push(format!("/u:{user}"));
                }
                Ok(SpawnPlan {
                    program,
                    args,
                    stdin_password: password,
                })
            },
            Protocol::Vnc => {
                let host = spec
                    .host
                    .clone()
                    .ok_or(ConnError::InvalidSession("VNC host is missing"))?;
                let port = spec.port.unwrap_or(5900);
                let program = resolve_client("MBXT_VNC_CLIENT", DEFAULT_VNC_CLIENT, "VNC")?;
                Ok(SpawnPlan {
                    program,
                    args: vec![format!("{host}:{port}")],
                    stdin_password: None,
                })
            },
            _ => Err(ConnError::InvalidSession(
                "custom sessions carry no command in this build",
            )),
        }
    }

    #[derive(Debug)]
    pub struct SpawnConn {
        spec: SessionSpec,
        child: Option<Child>,
        stdin: Option<ChildStdin>,
        lines: Option<tokio::sync::mpsc::Receiver<String>>,
        drain: Option<tokio::task::JoinHandle<()>>,
        exited: bool,
    }

    impl SpawnConn {
        pub fn new(spec: &SessionSpec) -> Self {
            Self {
                spec: spec.clone(),
                child: None,
                stdin: None,
                lines: None,
                drain: None,
                exited: false,
            }
        }
    }

    #[async_trait]
    impl Connection for SpawnConn {
        async fn start(
            &mut self,
            auth: ConnectionAuth,
            _size: TerminalSize,
        ) -> Result<(), ConnError> {
            let plan = plan_for(&self.spec, &auth)?;
            let mut command = Command::new(&plan.program);
            command
                .args(&plan.args)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .kill_on_drop(true);
            // Scrub the viewer environment of our own overrides so a
            // nested remote-app never inherits them by accident.
            command.env_remove("MBXT_RDP_CLIENT");
            command.env_remove("MBXT_VNC_CLIENT");
            let mut child = command.spawn().map_err(|error| {
                ConnError::Io(std::io::Error::other(format!(
                    "failed to launch '{}': {error}",
                    plan.program.display()
                )))
            })?;
            // Password over stdin (never argv). Viewers that do not read
            // the password from stdin fall back to their own prompt; the
            // session still works interactively.
            if let (Some(password), Some(stdin)) = (plan.stdin_password, child.stdin.as_mut()) {
                // Short-lived formatted copy; the Zeroizing wrapper
                // scrubs it on drop at scope end.
                let secret = Zeroizing::new(format!("{}\n", password.as_str()));
                stdin
                    .write_all(secret.as_bytes())
                    .await
                    .map_err(ConnError::Io)?;
            }
            let stdin = child.stdin.take();
            let stderr = child.stderr.take();
            let (tx, rx) = tokio::sync::mpsc::channel(128);
            let drain = tokio::spawn(async move {
                if let Some(pipe) = stderr {
                    let mut reader = tokio::io::BufReader::new(pipe).lines();
                    while let Ok(Some(line)) = reader.next_line().await {
                        if tx.send(line).await.is_err() {
                            break;
                        }
                    }
                }
            });
            self.child = Some(child);
            self.stdin = stdin;
            self.lines = Some(rx);
            self.drain = Some(drain);
            self.exited = false;
            tracing::info!(
                session = %self.spec.name,
                program = %plan.program.display(),
                "external viewer launched"
            );
            Ok(())
        }

        async fn write(&mut self, data: &[u8]) -> Result<(), ConnError> {
            // Session input is forwarded to the viewer stdin. Viewers
            // typically ignore it after startup (keyboard goes to the
            // native window); the pipe is the only channel we own.
            let stdin = self.stdin.as_mut().ok_or(ConnError::RemoteClosed)?;
            stdin.write_all(data).await.map_err(ConnError::Io)?;
            Ok(())
        }

        async fn resize(&mut self, _size: TerminalSize) -> Result<(), ConnError> {
            // Viewers manage their own windows; nothing to renegotiate.
            Ok(())
        }

        async fn next_event(&mut self) -> Result<ConnectionEvent, ConnError> {
            if self.exited {
                return Ok(ConnectionEvent::Eof);
            }
            let child = self.child.as_mut().ok_or(ConnError::RemoteClosed)?;
            // Drain buffered diagnostics first.
            if let Some(lines) = self.lines.as_mut() {
                match lines.try_recv() {
                    Ok(line) => return Ok(ConnectionEvent::Output(line.into_bytes())),
                    Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {},
                    Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {},
                }
            }
            match child
                .try_wait()
                .map_err(|error| ConnError::Io(std::io::Error::other(error.to_string())))?
            {
                Some(status) => {
                    self.exited = true;
                    Ok(ConnectionEvent::ExitStatus(
                        status.code().map(|code| code as u32).unwrap_or(128),
                    ))
                },
                None => {
                    // Park until the next diagnostic line or drain end.
                    // A client that closes stderr while running is
                    // abnormal; poll its exit at 10 Hz rather than
                    // spinning or lying about EOF.
                    match self.lines.as_mut() {
                        Some(lines) => match lines.recv().await {
                            Some(line) => Ok(ConnectionEvent::Output(line.into_bytes())),
                            None => loop {
                                tokio::time::sleep(Duration::from_millis(100)).await;
                                match child.try_wait().map_err(|error| {
                                    ConnError::Io(std::io::Error::other(error.to_string()))
                                })? {
                                    Some(status) => {
                                        self.exited = true;
                                        return Ok(ConnectionEvent::ExitStatus(
                                            status.code().map(|code| code as u32).unwrap_or(128),
                                        ));
                                    },
                                    None => continue,
                                }
                            },
                        },
                        None => Ok(ConnectionEvent::Eof),
                    }
                },
            }
        }

        async fn shutdown(&mut self) -> Result<(), ConnError> {
            if let Some(drain) = self.drain.take() {
                drain.abort();
            }
            self.stdin.take();
            self.lines.take();
            if let Some(mut child) = self.child.take() {
                let _ = child.kill().await;
                let _ = tokio::time::timeout(SHUTDOWN_GRACE, child.wait()).await;
            }
            self.exited = true;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mbxt_core::AuthMethod;

    fn spec(protocol: Protocol) -> SessionSpec {
        SessionSpec {
            name: "test".into(),
            protocol,
            host: Some("example.invalid".into()),
            port: None,
            username: None,
            auth: AuthMethod::Password,
            tags: vec![],
            notes: String::new(),
            x11_forwarding: false,
            serial: None,
            forwards: Vec::new(),
        }
    }

    #[test]
    fn factory_respects_features() {
        // With default features (ssh, telnet), ftp is not built in.
        match ConnectionFactory::create(&spec(Protocol::Ftp)) {
            Err(ConnError::Unsupported(_)) => {},
            _ => panic!("expected Unsupported"),
        }
    }

    /// Serializes viewer-env mutation; the spawn transport is the only
    /// reader of `MBXT_RDP_CLIENT` / `MBXT_VNC_CLIENT`.
    #[cfg(any(feature = "rdp", feature = "vnc"))]
    static SPAWN_ENV_GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[cfg(any(feature = "rdp", feature = "vnc"))]
    fn with_spawn_env(rdp: Option<&str>, run: impl FnOnce()) {
        let _guard = SPAWN_ENV_GUARD.lock().unwrap();
        let saved = std::env::var_os("MBXT_RDP_CLIENT");
        match rdp {
            Some(path) => std::env::set_var("MBXT_RDP_CLIENT", path),
            None => std::env::remove_var("MBXT_RDP_CLIENT"),
        }
        run();
        match saved {
            Some(path) => std::env::set_var("MBXT_RDP_CLIENT", path),
            None => std::env::remove_var("MBXT_RDP_CLIENT"),
        }
    }

    #[cfg(any(feature = "rdp", feature = "vnc"))]
    fn block_on<F: std::future::Future>(fut: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime")
            .block_on(fut)
    }

    #[test]
    #[cfg(unix)]
    #[cfg(any(feature = "rdp", feature = "vnc"))]
    fn rdp_argv_carries_no_secrets() {
        with_spawn_env(Some("/bin/true"), || {
            let session = SessionSpec {
                username: Some("ops".into()),
                port: Some(13389),
                ..spec(Protocol::Rdp)
            };
            let auth = ConnectionAuth::Password(Zeroizing::new("s3cret".into()));
            let plan = spawn::plan_for(&session, &auth).expect("plan");
            assert_eq!(plan.program, PathBuf::from("/bin/true"));
            assert!(plan.args.iter().any(|a| a == "/v:example.invalid:13389"));
            assert!(plan.args.iter().any(|a| a == "/u:ops"));
            assert!(
                !plan.args.iter().any(|a| a.contains("s3cret")),
                "password leaked into argv: {:?}",
                plan.args
            );
            assert_eq!(
                plan.stdin_password.as_ref().map(|p| p.as_str()),
                Some("s3cret")
            );
        });
    }

    #[test]
    #[cfg(unix)]
    #[cfg(any(feature = "rdp", feature = "vnc"))]
    fn missing_viewer_binary_is_a_clean_error() {
        // TEST-NET-1 address: even a successful spawn could never
        // reach anything real (and resolution fails first anyway).
        with_spawn_env(Some("/nonexistent/mbxt-test-client"), || {
            let session = SessionSpec {
                host: Some("192.0.2.1".into()),
                ..spec(Protocol::Rdp)
            };
            let auth = ConnectionAuth::Password(Zeroizing::new("pw".into()));
            let mut conn = spawn::SpawnConn::new(&session);
            let err = block_on(conn.start(auth, TerminalSize::default())).unwrap_err();
            let message = format!("{err}");
            assert!(
                message.contains("MBXT_RDP_CLIENT"),
                "diagnostic must name the override: {message}"
            );
        });
    }

    #[test]
    #[cfg(unix)]
    #[cfg(any(feature = "rdp", feature = "vnc"))]
    fn true_client_lifecycle_reports_exit_status() {
        // /bin/true exits(0) instantly: exercises spawn, stdin write,
        // stderr drain, exit event, and shutdown with no network.
        with_spawn_env(Some("/bin/true"), || {
            let auth = ConnectionAuth::Password(Zeroizing::new("pw".into()));
            let mut conn = spawn::SpawnConn::new(&spec(Protocol::Rdp));
            block_on(conn.start(auth, TerminalSize::default())).expect("launch");
            assert_eq!(
                block_on(conn.next_event()).expect("event"),
                ConnectionEvent::ExitStatus(0)
            );
            assert_eq!(
                block_on(conn.next_event()).expect("event"),
                ConnectionEvent::Eof
            );
            block_on(conn.shutdown()).expect("shutdown");
        });
    }

    #[test]
    #[cfg(any(feature = "rdp", feature = "vnc"))]
    fn custom_sessions_are_rejected_without_spawn() {
        let auth = ConnectionAuth::Password(Zeroizing::new("pw".into()));
        let mut conn = spawn::SpawnConn::new(&spec(Protocol::Custom));
        let err = block_on(conn.start(auth, TerminalSize::default())).unwrap_err();
        assert!(
            matches!(err, ConnError::InvalidSession(_)),
            "unexpected: {err:?}"
        );
    }

    #[test]
    #[cfg(any(feature = "rdp", feature = "vnc"))]
    fn wrong_auth_method_rejected_for_rdp() {
        let auth = ConnectionAuth::KeyFile {
            path: PathBuf::from("/nonexistent/key"),
            passphrase: None,
        };
        let mut conn = spawn::SpawnConn::new(&spec(Protocol::Rdp));
        let err = block_on(conn.start(auth, TerminalSize::default())).unwrap_err();
        assert!(
            matches!(err, ConnError::AuthMismatch(_)),
            "unexpected: {err:?}"
        );
    }
}

//! One actor task per open connection. Actors exclusively own transports and
//! expose only typed controls/events to the rest of the application.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use mbxt_core::{SessionId, SessionSpec, SessionState, UiEvent};
use tokio::sync::{broadcast, mpsc, oneshot};

use super::{ConnectionAuth, ConnectionEvent, ConnectionFactory, TerminalSize};

/// Channel stream for port-forwarding data paths. `None`-carrying variants
/// are ssh-gated alongside the transport methods.
#[cfg(feature = "ssh")]
type ForwardStream = mbxt_connections::ForwardStream;

pub enum SessionCtl {
    Write(Vec<u8>),
    Resize(TerminalSize),
    Shutdown,
    /// Run one command, collect output (Prompt 5.4 remote tools).
    Exec {
        command: String,
        reply: oneshot::Sender<Result<String, String>>,
    },
    /// Open a `direct-tcpip` channel (`-L` / SOCKS data path).
    #[cfg(feature = "ssh")]
    OpenDirect {
        host: String,
        port: u32,
        reply: oneshot::Sender<Result<ForwardStream, String>>,
    },
    /// Ask the server to listen (`-R` control path).
    #[cfg(feature = "ssh")]
    RequestRemoteForward {
        bind: String,
        port: u32,
        reply: oneshot::Sender<Result<u32, String>>,
    },
    /// Release a remote listen (`-R` teardown).
    #[cfg(feature = "ssh")]
    CancelRemoteForward {
        bind: String,
        port: u32,
        reply: oneshot::Sender<Result<(), String>>,
    },
    /// Register the sink for server-opened `forwarded-tcpip` channels.
    #[cfg(feature = "ssh")]
    SetForwardedSink(Option<mpsc::UnboundedSender<mbxt_connections::ForwardedTcpIp>>),
}

impl std::fmt::Debug for SessionCtl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Channel streams are not `Debug` — render shapes, not handles.
        match self {
            Self::Write(bytes) => f
                .debug_tuple("Write")
                .field(&format_args!("{} bytes", bytes.len()))
                .finish(),
            Self::Resize(size) => f.debug_tuple("Resize").field(size).finish(),
            Self::Shutdown => f.write_str("Shutdown"),
            Self::Exec { command, .. } => f.debug_struct("Exec").field("command", command).finish(),
            #[cfg(feature = "ssh")]
            Self::OpenDirect { host, port, .. } => f
                .debug_struct("OpenDirect")
                .field("host", host)
                .field("port", port)
                .finish(),
            #[cfg(feature = "ssh")]
            Self::RequestRemoteForward { bind, port, .. } => f
                .debug_struct("RequestRemoteForward")
                .field("bind", bind)
                .field("port", port)
                .finish(),
            #[cfg(feature = "ssh")]
            Self::CancelRemoteForward { bind, port, .. } => f
                .debug_struct("CancelRemoteForward")
                .field("bind", bind)
                .field("port", port)
                .finish(),
            #[cfg(feature = "ssh")]
            Self::SetForwardedSink(_) => f.write_str("SetForwardedSink(..)"),
        }
    }
}

pub struct SessionManager {
    actors: Mutex<HashMap<SessionId, mpsc::Sender<SessionCtl>>>,
    events: broadcast::Sender<UiEvent>,
}

impl std::fmt::Debug for SessionManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionManager").finish_non_exhaustive()
    }
}

impl SessionManager {
    pub fn shared() -> &'static Self {
        static INSTANCE: OnceLock<SessionManager> = OnceLock::new();
        INSTANCE.get_or_init(|| Self {
            actors: Mutex::new(HashMap::new()),
            events: broadcast::channel(1024).0,
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<UiEvent> {
        self.events.subscribe()
    }

    pub fn connect(
        &'static self,
        id: SessionId,
        spec: SessionSpec,
        auth: ConnectionAuth,
        size: TerminalSize,
    ) -> Result<(), String> {
        let connection = ConnectionFactory::create(&spec).map_err(|e| e.to_string())?;
        let (tx, rx) = mpsc::channel(256);
        if let Some(previous) = self
            .actors
            .lock()
            .expect("actor registry poisoned")
            .insert(id, tx)
        {
            let _ = previous.try_send(SessionCtl::Shutdown);
        }
        let events = self.events.clone();
        let _ = events.send(UiEvent::SessionStateChanged {
            session: id,
            state: SessionState::Connecting,
        });
        tokio::spawn(run_actor(id, connection, auth, size, rx, events));
        Ok(())
    }

    pub fn write(&self, id: SessionId, bytes: Vec<u8>) -> Result<(), String> {
        self.send(id, SessionCtl::Write(bytes))
    }

    pub fn resize(&self, id: SessionId, size: TerminalSize) -> Result<(), String> {
        self.send(id, SessionCtl::Resize(size))
    }

    pub fn disconnect(&self, id: SessionId) -> Result<(), String> {
        self.send(id, SessionCtl::Shutdown)
    }

    pub fn multi_exec(&self, targets: &[SessionId], bytes: &[u8]) -> Result<(), String> {
        for id in targets {
            self.write(*id, bytes.to_vec())?;
        }
        Ok(())
    }

    /// Best-effort broadcast (Prompt 5.1): every target is attempted even
    /// when one fails, so a dropped session cannot swallow the others'
    /// commands. Returns `(delivered, [(session, error)])`; callers prune
    /// the failures and notify the user.
    pub fn broadcast(
        &self,
        targets: &[SessionId],
        bytes: &[u8],
    ) -> (Vec<SessionId>, Vec<(SessionId, String)>) {
        let mut delivered = Vec::with_capacity(targets.len());
        let mut failed = Vec::new();
        for id in targets {
            match self.write(*id, bytes.to_vec()) {
                Ok(()) => delivered.push(*id),
                Err(reason) => failed.push((*id, reason)),
            }
        }
        (delivered, failed)
    }

    /// Publish transfer progress onto the event bus (SFTP tasks call this per
    /// chunk; the subscription forwards it as `SftpMsg::TransferProgress`).
    pub fn publish_transfer_progress(&self, session: SessionId, done: u64, total: u64) {
        let _ = self.events.send(UiEvent::TransferProgress {
            session,
            done,
            total,
        });
    }

    /// Open a tunneled TCP channel on a live SSH session (`-L` / SOCKS
    /// data path, Prompt 5.3). Round-trips through the actor so channel
    /// opens serialize with shell traffic on the one connection.
    #[cfg(feature = "ssh")]
    pub async fn open_direct_channel(
        &self,
        session: SessionId,
        host: &str,
        port: u32,
    ) -> Result<ForwardStream, String> {
        let (tx, rx) = oneshot::channel();
        self.send(
            session,
            SessionCtl::OpenDirect {
                host: host.to_string(),
                port,
                reply: tx,
            },
        )?;
        rx.await
            .map_err(|_| format!("session #{session} actor is gone"))?
    }

    /// Ask the server to listen for a remote forward (`-R` control path).
    /// Returns the bound port.
    #[cfg(feature = "ssh")]
    pub async fn request_remote_forward(
        &self,
        session: SessionId,
        bind: &str,
        port: u32,
    ) -> Result<u32, String> {
        let (tx, rx) = oneshot::channel();
        self.send(
            session,
            SessionCtl::RequestRemoteForward {
                bind: bind.to_string(),
                port,
                reply: tx,
            },
        )?;
        rx.await
            .map_err(|_| format!("session #{session} actor is gone"))?
    }

    /// Release a remote listen (best-effort `-R` teardown).
    #[cfg(feature = "ssh")]
    pub async fn cancel_remote_forward(
        &self,
        session: SessionId,
        bind: &str,
        port: u32,
    ) -> Result<(), String> {
        let (tx, rx) = oneshot::channel();
        self.send(
            session,
            SessionCtl::CancelRemoteForward {
                bind: bind.to_string(),
                port,
                reply: tx,
            },
        )?;
        rx.await
            .map_err(|_| format!("session #{session} actor is gone"))?
    }

    /// Register the sink for server-opened `forwarded-tcpip` channels.
    #[cfg(feature = "ssh")]
    pub fn register_forwarded_sink(
        &self,
        session: SessionId,
        sink: Option<mpsc::UnboundedSender<mbxt_connections::ForwardedTcpIp>>,
    ) -> Result<(), String> {
        self.send(session, SessionCtl::SetForwardedSink(sink))
    }

    /// Run one command on a live session and collect its output (Prompt 5.4
    /// remote tools). Round-trips through the actor so exec serializes with
    /// shell traffic on the one connection.
    pub async fn exec(&self, session: SessionId, command: &str) -> Result<String, String> {
        let (tx, rx) = oneshot::channel();
        self.send(
            session,
            SessionCtl::Exec {
                command: command.to_string(),
                reply: tx,
            },
        )?;
        rx.await
            .map_err(|_| format!("session #{session} actor is gone"))?
    }

    /// Register a mock target and take its control receiver (tests only:
    /// asserts exact broadcast bytes per session without any transport).
    #[cfg(test)]
    pub fn inject_test_target(&self, id: SessionId) -> tokio::sync::mpsc::Receiver<SessionCtl> {
        let (tx, rx) = tokio::sync::mpsc::channel(256);
        if let Ok(mut actors) = self.actors.lock() {
            actors.insert(id, tx);
        }
        rx
    }

    fn send(&self, id: SessionId, control: SessionCtl) -> Result<(), String> {
        let actors = self
            .actors
            .lock()
            .map_err(|_| "actor registry poisoned".to_string())?;
        let tx = actors
            .get(&id)
            .ok_or_else(|| format!("session #{id} is not connected"))?;
        tx.try_send(control)
            .map_err(|e| format!("session #{id} control queue: {e}"))
    }

    fn remove(&self, id: SessionId) {
        if let Ok(mut actors) = self.actors.lock() {
            actors.remove(&id);
        }
    }
}

async fn run_actor(
    id: SessionId,
    mut connection: Box<dyn super::Connection>,
    auth: ConnectionAuth,
    size: TerminalSize,
    mut controls: mpsc::Receiver<SessionCtl>,
    events: broadcast::Sender<UiEvent>,
) {
    if let Err(error) = connection.start(auth, size).await {
        let _ = events.send(UiEvent::SessionStateChanged {
            session: id,
            state: SessionState::Failed(error.to_string()),
        });
        SessionManager::shared().remove(id);
        return;
    }
    let _ = events.send(UiEvent::SessionStateChanged {
        session: id,
        state: SessionState::Connected,
    });

    let mut stopping = false;
    while !stopping {
        while let Ok(control) = controls.try_recv() {
            let result = match control {
                SessionCtl::Write(bytes) => connection.write(&bytes).await,
                SessionCtl::Resize(size) => connection.resize(size).await,
                SessionCtl::Shutdown => {
                    stopping = true;
                    break;
                },
                SessionCtl::Exec { command, reply } => {
                    let outcome = connection
                        .exec(&command)
                        .await
                        .map_err(|err| err.to_string());
                    let _ = reply.send(outcome);
                    Ok(())
                },
                #[cfg(feature = "ssh")]
                SessionCtl::OpenDirect { host, port, reply } => {
                    let outcome = connection
                        .open_direct_channel(&host, port)
                        .await
                        .map_err(|err| err.to_string());
                    let _ = reply.send(outcome);
                    Ok(())
                },
                #[cfg(feature = "ssh")]
                SessionCtl::RequestRemoteForward { bind, port, reply } => {
                    let outcome = connection
                        .request_remote_forward(&bind, port)
                        .await
                        .map_err(|err| err.to_string());
                    let _ = reply.send(outcome);
                    Ok(())
                },
                #[cfg(feature = "ssh")]
                SessionCtl::CancelRemoteForward { bind, port, reply } => {
                    let outcome = connection
                        .cancel_remote_forward(&bind, port)
                        .await
                        .map_err(|err| err.to_string());
                    let _ = reply.send(outcome);
                    Ok(())
                },
                #[cfg(feature = "ssh")]
                SessionCtl::SetForwardedSink(sink) => {
                    connection.set_forwarded_sink(sink);
                    Ok(())
                },
            };
            if let Err(error) = result {
                let _ = events.send(UiEvent::Error(error.to_string()));
                stopping = true;
                break;
            }
        }
        if stopping {
            break;
        }
        match tokio::time::timeout(Duration::from_millis(16), connection.next_event()).await {
            Ok(Ok(ConnectionEvent::Output(bytes))) => {
                let _ = events.send(UiEvent::TerminalOutput { session: id, bytes });
            },
            Ok(Ok(ConnectionEvent::ExitStatus(status))) => {
                tracing::info!(session = id, status, "remote shell exited");
            },
            Ok(Ok(ConnectionEvent::Eof)) => break,
            Ok(Err(error)) => {
                let _ = events.send(UiEvent::Error(error.to_string()));
                break;
            },
            Err(_) => {},
        }
    }
    let _ = connection.shutdown().await;
    SessionManager::shared().remove(id);
    let _ = events.send(UiEvent::SessionStateChanged {
        session: id,
        state: SessionState::Disconnected,
    });
}

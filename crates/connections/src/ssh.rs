use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use russh::client;
use russh::{Channel, ChannelMsg, ChannelStream, Disconnect};
use tokio::sync::mpsc;

use crate::{
    ConnError, Connection, ConnectionAuth, ConnectionEvent, ForwardStream, SessionSpec,
    TerminalSize,
};

/// Server-opened `x11` channel stream, handed to the proxy task.
type X11Stream = ChannelStream<client::Msg>;

#[derive(Debug)]
struct ClientHandler {
    host: String,
    port: u16,
    /// Sink for server-opened `x11` channels (present when forwarding).
    x11_tx: Option<mpsc::UnboundedSender<X11Stream>>,
    /// Sink for server-opened `forwarded-tcpip` channels (`-R` data path).
    forwarded_tx: Option<mpsc::UnboundedSender<crate::ForwardedTcpIp>>,
}

#[async_trait]
impl client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &russh::keys::key::PublicKey,
    ) -> Result<bool, Self::Error> {
        match russh::keys::check_known_hosts(&self.host, self.port, server_public_key) {
            Ok(accepted) => Ok(accepted),
            Err(error) => {
                tracing::warn!(%error, host = %self.host, "known_hosts check failed");
                Ok(false)
            },
        }
    }

    async fn server_channel_open_x11(
        &mut self,
        channel: Channel<client::Msg>,
        originator_address: &str,
        originator_port: u32,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        tracing::debug!(%originator_address, originator_port, "server opened x11 channel");
        if let Some(tx) = self.x11_tx.as_ref() {
            // Receiver gone (shutdown) → drop the channel loudly, not silently.
            if tx.send(channel.into_stream()).is_err() {
                tracing::warn!("x11 channel dropped: forwarder is gone");
            }
        } else {
            tracing::warn!("x11 channel opened but forwarding is disabled; dropping");
        }
        Ok(())
    }

    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: Channel<client::Msg>,
        connected_address: &str,
        connected_port: u32,
        originator_address: &str,
        originator_port: u32,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        tracing::debug!(
            %connected_address,
            connected_port,
            %originator_address,
            originator_port,
            "server opened forwarded-tcpip channel"
        );
        if let Some(tx) = self.forwarded_tx.as_ref() {
            let opened = crate::ForwardedTcpIp {
                stream: channel.into_stream(),
                connected_host: connected_address.to_string(),
                connected_port,
            };
            if tx.send(opened).is_err() {
                tracing::warn!("forwarded-tcpip channel dropped: no forward running");
            }
        } else {
            tracing::warn!("forwarded-tcpip channel opened with no forward running; dropping");
        }
        Ok(())
    }
}

pub struct SshConn {
    spec: SessionSpec,
    handle: Option<client::Handle<ClientHandler>>,
    channel: Option<Channel<client::Msg>>,
    x11_task: Option<tokio::task::JoinHandle<()>>,
    /// Buffer for server-opened `forwarded-tcpip` channels. The handler can
    /// only be wired at connect time, so channels always land here first;
    /// [`Connection::set_forwarded_sink`] records the live target and the
    /// `next_event` pump drains to it (no extra task, no ownership churn
    /// across register/unregister cycles).
    forwarded_rx: Option<mpsc::UnboundedReceiver<crate::ForwardedTcpIp>>,
    forwarded_sink: Option<mpsc::UnboundedSender<crate::ForwardedTcpIp>>,
}

impl std::fmt::Debug for SshConn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SshConn")
            .field("spec", &self.spec)
            .finish_non_exhaustive()
    }
}

impl SshConn {
    pub fn new(spec: &SessionSpec) -> Self {
        Self {
            spec: spec.clone(),
            handle: None,
            channel: None,
            x11_task: None,
            forwarded_rx: None,
            forwarded_sink: None,
        }
    }
}

/// Unix socket for `$DISPLAY` (`/tmp/.X11-unix/X<n>`); `None` when the
/// display is remote or unparsable. Pure parsing lives in `mbxt-core::x11`
/// (shared with the app-level forwarder); this transport-local helper keeps
/// the hot path dependency-free. Unix only (used by the unix proxy path).
#[cfg(unix)]
fn x11_socket_path() -> Option<PathBuf> {
    let display = std::env::var("DISPLAY").ok()?;
    let info = mbxt_core::x11::parse_display(&display).ok()?;
    mbxt_core::x11::local_socket_path(&info)
}

/// Hex `MIT-MAGIC-COOKIE-1` for the current display, if `~/.Xauthority`
/// yields one. A missing file is not an error — the server is asked with an
/// empty cookie and warns (mirrors `ssh -X` without `xauth`).
fn x11_cookie_hex() -> String {
    let display_number = std::env::var("DISPLAY")
        .ok()
        .and_then(|display| mbxt_core::x11::parse_display(&display).ok())
        .map(|info| info.display_number)
        .unwrap_or(0);
    let path = std::env::var_os("XAUTHORITY")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".Xauthority")));
    let Some(path) = path else {
        return String::new();
    };
    let Ok(bytes) = std::fs::read(path) else {
        return String::new();
    };
    let entries = mbxt_core::x11::parse_xauthority(&bytes);
    mbxt_core::x11::cookie_for_display(&entries, display_number)
        .map(mbxt_core::x11::hex_cookie)
        .unwrap_or_default()
}

/// Drain server-opened `x11` streams into the local X server, one proxy task
/// per connection. EOF on either side ends that task; the loop survives
/// individual failures so one dead viewer never kills forwarding.
async fn drain_x11_channels(mut rx: mpsc::UnboundedReceiver<X11Stream>) {
    while let Some(stream) = rx.recv().await {
        tokio::spawn(proxy_one_x11(stream));
    }
}

#[cfg(unix)]
async fn proxy_one_x11(mut stream: X11Stream) {
    let Some(socket) = x11_socket_path() else {
        tracing::warn!("x11 channel dropped: no local X socket for $DISPLAY");
        return;
    };
    let mut local = match tokio::net::UnixStream::connect(&socket).await {
        Ok(local) => local,
        Err(error) => {
            tracing::warn!(%error, socket = %socket.display(), "x11 dial failed");
            return;
        },
    };
    match tokio::io::copy_bidirectional(&mut stream, &mut local).await {
        Ok((a_to_b, b_to_a)) => {
            tracing::debug!(a_to_b, b_to_a, "x11 connection closed");
        },
        Err(error) => {
            tracing::warn!(%error, "x11 proxy failed");
        },
    }
}

/// X11 forwarding is unix-only (no X server path on other platforms):
/// accept the channel so the handshake succeeds, then drop it loudly.
#[cfg(not(unix))]
async fn proxy_one_x11(_stream: X11Stream) {
    tracing::warn!("x11 channel dropped: forwarding is only supported on unix");
}

/// ssh-agent authentication (`SSH_AUTH_SOCK`): offer every agent
/// identity in turn; the first the server accepts wins. Shared by the
/// interactive SSH session and the SFTP subsystem (which dials its own
/// transport but runs the same handshake).
pub async fn authenticate_via_agent<H>(
    handle: &mut russh::client::Handle<H>,
    user: &str,
) -> Result<bool, ConnError>
where
    H: russh::client::Handler,
{
    use russh::keys::agent::client::AgentClient;
    let mut agent = AgentClient::connect_env()
        .await
        .map_err(|error| ConnError::Key(format!("ssh-agent unavailable: {error}")))?;
    let identities = agent
        .request_identities()
        .await
        .map_err(|error| ConnError::Key(format!("ssh-agent listed no identities: {error}")))?;
    tracing::debug!(count = identities.len(), "ssh-agent identities offered");
    for key in identities {
        let (back, result) = handle.authenticate_future(user, key, agent).await;
        agent = back;
        match result {
            Ok(true) => return Ok(true),
            Ok(false) => continue, // server declined this identity; try the next
            Err(error) => {
                return Err(ConnError::Key(format!("ssh-agent signing failed: {error}")));
            },
        }
    }
    Ok(false)
}

#[async_trait]
impl Connection for SshConn {
    async fn start(&mut self, auth: ConnectionAuth, size: TerminalSize) -> Result<(), ConnError> {
        let host = self
            .spec
            .host
            .clone()
            .ok_or(ConnError::InvalidSession("SSH host is missing"))?;
        let user = self
            .spec
            .username
            .clone()
            .ok_or(ConnError::InvalidSession("SSH username is missing"))?;
        let port = self.spec.port.unwrap_or(22);
        let config = Arc::new(client::Config {
            inactivity_timeout: Some(Duration::from_secs(30)),
            keepalive_interval: Some(Duration::from_secs(15)),
            keepalive_max: 3,
            ..Default::default()
        });
        let (x11_tx, x11_rx) = if self.spec.x11_forwarding {
            let (tx, rx) = mpsc::unbounded_channel();
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };
        let (forwarded_tx, forwarded_rx) = mpsc::unbounded_channel();
        let handler = ClientHandler {
            host: host.clone(),
            port,
            x11_tx,
            forwarded_tx: Some(forwarded_tx),
        };
        let mut handle = client::connect(config, (host.as_str(), port), handler).await?;

        let authenticated = match auth {
            ConnectionAuth::Password(password) => {
                handle
                    .authenticate_password(user.clone(), password.as_str())
                    .await?
            },
            ConnectionAuth::KeyFile { path, passphrase } => {
                let key =
                    russh::keys::load_secret_key(path, passphrase.as_deref().map(String::as_str))
                        .map_err(|error| ConnError::Key(error.to_string()))?;
                handle
                    .authenticate_publickey(user.clone(), Arc::new(key))
                    .await?
            },
            ConnectionAuth::KeyboardInteractive(response) => {
                use client::KeyboardInteractiveAuthResponse as Response;
                let mut state = handle
                    .authenticate_keyboard_interactive_start(user.clone(), None::<String>)
                    .await?;
                loop {
                    match state {
                        Response::Success => break true,
                        Response::Failure => break false,
                        Response::InfoRequest { prompts, .. } => {
                            state = handle
                                .authenticate_keyboard_interactive_respond(
                                    prompts.iter().map(|_| response.to_string()).collect(),
                                )
                                .await?;
                        },
                    }
                }
            },
            ConnectionAuth::Agent => authenticate_via_agent(&mut handle, &user).await?,
        };
        if !authenticated {
            return Err(ConnError::AuthRejected);
        }

        let channel = handle.channel_open_session().await?;
        channel
            .request_pty(
                true,
                "xterm-256color",
                size.cols.into(),
                size.rows.into(),
                size.pixel_width,
                size.pixel_height,
                &[],
            )
            .await?;
        channel.request_shell(true).await?;
        // X11 forwarding (Prompt 4.1): request `x11` on the shell channel so
        // the server sets `DISPLAY` remotely and opens `x11` channels back
        // for each viewer; the drain task proxies them to the local X server.
        if self.spec.x11_forwarding {
            let cookie = x11_cookie_hex();
            if cookie.is_empty() {
                tracing::warn!("no Xauthority cookie found; requesting x11 with empty cookie");
            }
            channel
                .request_x11(true, false, "MIT-MAGIC-COOKIE-1", cookie, 0)
                .await?;
            if let Some(rx) = x11_rx {
                self.x11_task = Some(tokio::spawn(drain_x11_channels(rx)));
            }
            tracing::info!(session = %self.spec.name, "X11 forwarding requested");
        }
        self.handle = Some(handle);
        self.channel = Some(channel);
        self.forwarded_rx = Some(forwarded_rx);
        tracing::info!(session = %self.spec.name, %host, port, "SSH shell opened");
        Ok(())
    }

    async fn write(&mut self, data: &[u8]) -> Result<(), ConnError> {
        self.channel
            .as_ref()
            .ok_or(ConnError::RemoteClosed)?
            .data(data)
            .await?;
        Ok(())
    }

    async fn resize(&mut self, size: TerminalSize) -> Result<(), ConnError> {
        self.channel
            .as_ref()
            .ok_or(ConnError::RemoteClosed)?
            .window_change(
                size.cols.into(),
                size.rows.into(),
                size.pixel_width,
                size.pixel_height,
            )
            .await?;
        Ok(())
    }

    async fn next_event(&mut self) -> Result<ConnectionEvent, ConnError> {
        // Remote-forward data path: drain buffered channels first so `-R`
        // delivery never waits on shell output.
        self.pump_forwarded();
        let channel = self.channel.as_mut().ok_or(ConnError::RemoteClosed)?;
        loop {
            match channel.wait().await {
                Some(ChannelMsg::Data { data }) | Some(ChannelMsg::ExtendedData { data, .. }) => {
                    return Ok(ConnectionEvent::Output(data.to_vec()));
                },
                Some(ChannelMsg::ExitStatus { exit_status }) => {
                    return Ok(ConnectionEvent::ExitStatus(exit_status))
                },
                Some(ChannelMsg::Eof | ChannelMsg::Close) | None => {
                    return Ok(ConnectionEvent::Eof)
                },
                Some(_) => {},
            }
        }
    }

    /// Execute one command on a fresh `session` channel and collect
    /// stdout+stderr until EOF/close (Prompt 5.4 remote tools). Bounded by
    /// a 120 s cap so a hung remote cannot wedge the caller forever.
    async fn exec(&mut self, command: &str) -> Result<String, ConnError> {
        let handle = self.handle.as_ref().ok_or(ConnError::RemoteClosed)?;
        let mut channel = handle.channel_open_session().await?;
        channel.exec(true, command).await?;
        let collected = async {
            let mut output = Vec::new();
            loop {
                match channel.wait().await {
                    Some(ChannelMsg::Data { data })
                    | Some(ChannelMsg::ExtendedData { data, .. }) => {
                        output.extend_from_slice(&data);
                    },
                    Some(ChannelMsg::ExitStatus { .. })
                    | Some(ChannelMsg::Eof | ChannelMsg::Close)
                    | None => break,
                    Some(_) => {},
                }
            }
            output
        };
        let output = tokio::time::timeout(std::time::Duration::from_secs(120), collected)
            .await
            .map_err(|_| ConnError::RemoteClosed)?;
        Ok(String::from_utf8_lossy(&output).into_owned())
    }

    async fn shutdown(&mut self) -> Result<(), ConnError> {
        if let Some(task) = self.x11_task.take() {
            task.abort();
        }
        self.forwarded_sink.take();
        if let Some(channel) = self.channel.take() {
            let _ = channel.eof().await;
            let _ = channel.close().await;
        }
        if let Some(handle) = self.handle.take() {
            handle.disconnect(Disconnect::ByApplication, "", "").await?;
        }
        Ok(())
    }

    async fn open_direct_channel(
        &mut self,
        host: &str,
        port: u32,
    ) -> Result<ForwardStream, ConnError> {
        let handle = self.handle.as_ref().ok_or(ConnError::RemoteClosed)?;
        let channel = handle
            .channel_open_direct_tcpip(host, port, "127.0.0.1", 0)
            .await?;
        Ok(channel.into_stream())
    }

    async fn request_remote_forward(&mut self, bind: &str, port: u32) -> Result<u32, ConnError> {
        let handle = self.handle.as_mut().ok_or(ConnError::RemoteClosed)?;
        Ok(handle.tcpip_forward(bind, port).await?)
    }

    async fn cancel_remote_forward(&mut self, bind: &str, port: u32) -> Result<(), ConnError> {
        let handle = self.handle.as_ref().ok_or(ConnError::RemoteClosed)?;
        handle.cancel_tcpip_forward(bind, port).await?;
        Ok(())
    }

    fn set_forwarded_sink(&mut self, sink: Option<mpsc::UnboundedSender<crate::ForwardedTcpIp>>) {
        // Drain stale channels from a previous forward: they belong to a
        // cancelled listen and must never reach the next forward.
        if sink.is_none() {
            if let Some(rx) = self.forwarded_rx.as_mut() {
                while rx.try_recv().is_ok() {}
            }
        }
        self.forwarded_sink = sink;
    }
}

impl SshConn {
    /// Drain buffered `forwarded-tcpip` channels to the live sink (if any).
    /// Called on every event-loop pass, so delivery tracks shell activity
    /// and the 16 ms actor tick when the shell is idle.
    fn pump_forwarded(&mut self) {
        if self.forwarded_sink.is_none() {
            return;
        }
        if let (Some(rx), Some(tx)) = (self.forwarded_rx.as_mut(), self.forwarded_sink.as_ref()) {
            while let Ok(stream) = rx.try_recv() {
                if tx.send(stream).is_err() {
                    self.forwarded_sink = None;
                    break;
                }
            }
        }
    }
}

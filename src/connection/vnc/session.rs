//! RFB client session (Prompt 4.3): handshake, auth, update loop, input.
//!
//! Wire scope is deliberately narrow — RFB 3.7/3.8, `None` + `VNC`
//! security, and the four decoders in [`framebuffer`](super::framebuffer).
//! Anything else fails loudly at handshake instead of misbehaving mid-frame.
//!
//! Transports: direct TCP, or an SSH `direct-tcpip` channel stream (tunnel
//! through an existing SSH account — the recommended secure setup). The
//! handshake core takes any `AsyncRead + AsyncWrite` stream, so the mock
//! server in tests exercises the exact production path headlessly.

use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::Mutex;

use super::framebuffer::{apply_rect, Framebuffer, PixelFormat, RectHeader, SUPPORTED_ENCODINGS};
use super::input::{KeyPress, PointerEvent};
use super::VncError;

/// Byte-stream capability for RFB transports.
pub trait RfbStream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> RfbStream for T {}

/// Boxed byte stream (TCP socket, SSH channel stream, or test duplex).
type BoxStream = Box<dyn RfbStream>;

/// RFB security types we speak.
const SECURITY_NONE: u8 = 1;
const SECURITY_VNC: u8 = 2;

/// VNC endpoint configuration (no secrets stored — the password rides the
/// connect call only).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VncConfig {
    pub host: String,
    /// TCP port (conventionally `5900 + display`).
    pub port: u16,
    /// Display number (port hint only; the wire uses absolute ports).
    pub display: u8,
    /// Ask the server to share the desktop (vs exclusive access).
    pub shared: bool,
    /// Never send input (observe-only).
    pub view_only: bool,
    /// Compression preference 0–9 (`0` = Raw-first/low CPU, else
    /// Hextile-first). Maps to `SetEncodings` order.
    pub compression: u8,
    /// Tunnel through SSH instead of direct TCP (recommended).
    pub ssh_tunnel: Option<SshTunnel>,
}

/// SSH tunnel endpoint for VNC (separate control connection, like SFTP).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshTunnel {
    pub ssh_host: String,
    pub ssh_port: u16,
    pub ssh_username: String,
}

impl VncConfig {
    /// Direct TCP endpoint (`host`, `5900 + display`).
    pub fn direct(host: &str, display: u8) -> Self {
        Self {
            host: host.to_string(),
            port: 5900 + u16::from(display),
            display,
            shared: true,
            view_only: false,
            compression: 6,
            ssh_tunnel: None,
        }
    }

    /// `host:port` dial string.
    pub fn address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    /// Encoding preference for `SetEncodings` (compression knob).
    pub fn encodings(&self) -> Vec<i32> {
        if self.compression == 0 {
            vec![
                super::framebuffer::ENCODING_RAW,
                super::framebuffer::ENCODING_COPYRECT,
                super::framebuffer::ENCODING_RRE,
                super::framebuffer::ENCODING_HEXTILE,
            ]
        } else {
            SUPPORTED_ENCODINGS.to_vec()
        }
    }
}

/// Live VNC session: shared transport + framebuffer, background update loop.
pub struct VncSession {
    config: VncConfig,
    transport: Arc<Mutex<BoxStream>>,
    framebuffer: Arc<Mutex<Framebuffer>>,
    generation: AtomicU64,
    view_only: bool,
    /// Latest published frame (sync lock: the iced widget polls this at
    /// frame-tick rate without touching the async runtime).
    latest: std::sync::Mutex<Option<Arc<super::framebuffer::FrameSnapshot>>>,
    last_cut_text: std::sync::Mutex<Option<String>>,
    run_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl std::fmt::Debug for VncSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VncSession")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl VncSession {
    /// Connect over TCP and run the handshake.
    pub async fn connect_tcp(
        config: VncConfig,
        password: Option<&str>,
    ) -> Result<Arc<Self>, VncError> {
        let address = config.address();
        let stream = tokio::net::TcpStream::connect(&address)
            .await
            .map_err(VncError::io)?;
        Self::open(config, Box::new(stream), password).await
    }

    /// Connect through an SSH `direct-tcpip` channel (tunnel), then handshake.
    pub async fn connect_via_tunnel(
        config: VncConfig,
        tunnel: &SshTunnel,
        auth: crate::connection::ConnectionAuth,
        password: Option<&str>,
    ) -> Result<Arc<Self>, VncError> {
        let stream = dial_tunnel(tunnel, auth, &config.host, config.port).await?;
        Self::open(config, stream, password).await
    }

    /// Handshake over an established stream + spawn the update loop.
    /// Used by both transports and by tests (duplex + mock server).
    pub async fn open(
        config: VncConfig,
        mut stream: BoxStream,
        password: Option<&str>,
    ) -> Result<Arc<Self>, VncError> {
        let (width, height) = perform_handshake(&mut stream, &config, password).await?;
        let session = Arc::new(Self {
            view_only: config.view_only,
            config,
            transport: Arc::new(Mutex::new(stream)),
            framebuffer: Arc::new(Mutex::new(Framebuffer::new(width, height))),
            generation: AtomicU64::new(0),
            latest: std::sync::Mutex::new(None),
            last_cut_text: std::sync::Mutex::new(None),
            run_task: Mutex::new(None),
        });
        let task = tokio::spawn(run_update_loop(Arc::clone(&session)));
        *session.run_task.lock().await = Some(task);
        Ok(session)
    }

    /// Latest published frame for the viewer widget (sync: polled at the
    /// frame-tick rate; `None` before the first update lands).
    pub fn latest_frame(&self) -> Option<Arc<super::framebuffer::FrameSnapshot>> {
        self.latest.lock().expect("frame lock poisoned").clone()
    }

    /// Remote desktop size (the viewer sizes its texture from this).
    pub fn desktop_size(&self) -> (u16, u16) {
        // Dimensions are stable post-handshake; read via the latest snapshot
        // without the async lock, defaulting to a sane placeholder.
        self.latest
            .lock()
            .expect("frame lock poisoned")
            .as_ref()
            .map(|frame| (frame.width, frame.height))
            .unwrap_or((1024, 768))
    }

    /// Last `ServerCutText` body, if any (clipboard sync).
    pub fn last_cut_text(&self) -> Option<String> {
        self.last_cut_text
            .lock()
            .expect("cut-text lock poisoned")
            .clone()
    }

    /// Publish the current framebuffer for viewers (called per update batch).
    async fn publish_frame(&self) {
        let generation = self.generation.load(Ordering::SeqCst);
        let snapshot = {
            let fb = self.framebuffer.lock().await;
            Arc::new(fb.snapshot(generation))
        };
        *self.latest.lock().expect("frame lock poisoned") = Some(snapshot);
    }

    /// Request a full (non-incremental) refresh, e.g. after reconnect.
    pub async fn request_full_refresh(&self) -> Result<(), VncError> {
        self.request_update(false).await
    }

    /// Send one key event (rejected in view-only mode).
    pub async fn send_key(&self, press: KeyPress) -> Result<(), VncError> {
        self.require_input()?;
        let mut message = [0u8; 8];
        message[0] = 4;
        message[1] = u8::from(press.down);
        message[4..8].copy_from_slice(&press.keysym.to_be_bytes());
        self.write_message(&message).await
    }

    /// Type text (printable chars as taps; other chars skipped).
    pub async fn send_text(&self, text: &str) -> Result<(), VncError> {
        self.require_input()?;
        for c in text.chars() {
            if let Some([down, up]) = KeyPress::tap_char(c) {
                self.send_key(down).await?;
                self.send_key(up).await?;
            }
        }
        Ok(())
    }

    /// Send a pointer event (rejected in view-only mode).
    pub async fn send_pointer(&self, event: PointerEvent) -> Result<(), VncError> {
        self.require_input()?;
        let mut message = [0u8; 6];
        message[0] = 5;
        message[1] = event.buttons;
        message[2..4].copy_from_slice(&event.x.to_be_bytes());
        message[4..6].copy_from_slice(&event.y.to_be_bytes());
        self.write_message(&message).await
    }

    /// Send local clipboard text to the remote side.
    pub async fn send_cut_text(&self, text: &str) -> Result<(), VncError> {
        self.require_input()?;
        let body = super::input::encode_cut_text(text);
        let mut message = Vec::with_capacity(8 + body.len());
        message.extend_from_slice(&[6, 0, 0, 0]);
        message.extend_from_slice(&(body.len() as u32).to_be_bytes());
        message.extend_from_slice(&body);
        self.write_message(&message).await
    }

    /// Graceful shutdown (aborts the loop; handles drop on scope exit —
    /// nothing leaks).
    pub async fn disconnect(&self) {
        if let Some(task) = self.run_task.lock().await.take() {
            task.abort();
        }
    }

    fn require_input(&self) -> Result<(), VncError> {
        if self.view_only {
            return Err(VncError::protocol("session is view-only: input disabled"));
        }
        Ok(())
    }

    async fn write_message(&self, bytes: &[u8]) -> Result<(), VncError> {
        let mut transport = self.transport.lock().await;
        transport.write_all(bytes).await.map_err(VncError::io)?;
        transport.flush().await.map_err(VncError::io)?;
        Ok(())
    }

    async fn request_update(&self, incremental: bool) -> Result<(), VncError> {
        let (width, height) = self.desktop_size();
        let mut message = [0u8; 10];
        message[0] = 3;
        message[1] = u8::from(incremental);
        message[6..8].copy_from_slice(&width.to_be_bytes());
        message[8..10].copy_from_slice(&height.to_be_bytes());
        self.write_message(&message).await
    }
}

/// Background frame pump: incremental requests; idle polls back off so an
/// untouched desktop costs ~one empty round trip per 33 ms, while an active
/// one re-requests immediately (≈15 fps+ on typical desktops).
async fn run_update_loop(session: Arc<VncSession>) {
    loop {
        if session.request_update(true).await.is_err() {
            break;
        }
        match read_server_message(&session).await {
            Ok(updates) => {
                if updates == 0 {
                    tokio::time::sleep(Duration::from_millis(33)).await;
                }
            },
            Err(_) => break,
        }
    }
}

/// Read one server message; returns rect count for updates (0 for cut text).
async fn read_server_message(session: &VncSession) -> Result<usize, VncError> {
    let mut transport = session.transport.lock().await;
    let mut kind = [0u8; 1];
    transport
        .read_exact(&mut kind)
        .await
        .map_err(VncError::io)?;
    match kind[0] {
        0 => {
            let mut pad = [0u8; 1];
            let mut count = [0u8; 2];
            transport.read_exact(&mut pad).await.map_err(VncError::io)?;
            transport
                .read_exact(&mut count)
                .await
                .map_err(VncError::io)?;
            let rects = u16::from_be_bytes(count) as usize;
            let format = PixelFormat::xrgb32();
            let mut fb = session.framebuffer.lock().await;
            for _ in 0..rects {
                let mut raw = [0u8; 12];
                transport.read_exact(&mut raw).await.map_err(VncError::io)?;
                let header = RectHeader::parse(&raw);
                apply_rect(&mut fb, &format, &header, &mut *transport).await?;
            }
            drop(fb);
            session.generation.fetch_add(1, Ordering::SeqCst);
            session.publish_frame().await;
            Ok(rects)
        },
        2 => Ok(0), // Bell: nothing to render.
        3 => {
            let mut pad = [0u8; 3];
            let mut len = [0u8; 4];
            transport.read_exact(&mut pad).await.map_err(VncError::io)?;
            transport.read_exact(&mut len).await.map_err(VncError::io)?;
            let len = u32::from_be_bytes(len) as usize;
            let mut body = vec![0u8; len.min(1 << 20)];
            transport
                .read_exact(&mut body)
                .await
                .map_err(VncError::io)?;
            *session
                .last_cut_text
                .lock()
                .expect("cut-text lock poisoned") = Some(super::input::decode_cut_text(&body));
            Ok(0)
        },
        other => Err(VncError::protocol(format!(
            "unknown server message: {other}"
        ))),
    }
}

// ---------------------------------------------------------------------------
// Handshake
// ---------------------------------------------------------------------------

/// Version + security + init handshake. Returns `(width, height)`.
async fn perform_handshake<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
    config: &VncConfig,
    password: Option<&str>,
) -> Result<(u16, u16), VncError> {
    // 1. Version: accept RFB 3.x, speak 3.8.
    let mut version = [0u8; 12];
    stream
        .read_exact(&mut version)
        .await
        .map_err(VncError::io)?;
    if !version.starts_with(b"RFB ") {
        return Err(VncError::protocol("not an RFB server"));
    }
    stream
        .write_all(b"RFB 003.008\n")
        .await
        .map_err(VncError::io)?;

    // 2. Security types.
    let mut count = [0u8; 1];
    stream.read_exact(&mut count).await.map_err(VncError::io)?;
    if count[0] == 0 {
        return Err(read_conn_failed(stream).await?);
    }
    let mut types = vec![0u8; count[0] as usize];
    stream.read_exact(&mut types).await.map_err(VncError::io)?;
    let selected = if password.is_some() && types.contains(&SECURITY_VNC) {
        SECURITY_VNC
    } else if types.contains(&SECURITY_NONE) {
        SECURITY_NONE
    } else {
        return Err(VncError::Security(format!(
            "no common security type: {types:?}"
        )));
    };
    stream.write_all(&[selected]).await.map_err(VncError::io)?;

    // 3. Auth.
    if selected == SECURITY_VNC {
        let password =
            password.ok_or_else(|| VncError::Auth("server requires a password".into()))?;
        let mut challenge = [0u8; 16];
        stream
            .read_exact(&mut challenge)
            .await
            .map_err(VncError::io)?;
        let response = vnc_auth_response(password.as_bytes(), &challenge)?;
        stream.write_all(&response).await.map_err(VncError::io)?;
        let mut status = [0u8; 4];
        stream.read_exact(&mut status).await.map_err(VncError::io)?;
        if u32::from_be_bytes(status) != 0 {
            return Err(VncError::Auth("server rejected the password".into()));
        }
    } else {
        // SecurityResult also follows `None` auth in 3.8.
        let mut status = [0u8; 4];
        stream.read_exact(&mut status).await.map_err(VncError::io)?;
        if u32::from_be_bytes(status) != 0 {
            return Err(VncError::Auth("server refused the connection".into()));
        }
    }

    // 4. ClientInit + ServerInit.
    stream
        .write_all(&[u8::from(config.shared)])
        .await
        .map_err(VncError::io)?;
    let mut init = [0u8; 24];
    stream.read_exact(&mut init).await.map_err(VncError::io)?;
    let width = u16::from_be_bytes([init[0], init[1]]);
    let height = u16::from_be_bytes([init[2], init[3]]);
    if width == 0 || height == 0 {
        return Err(VncError::protocol("server sent an empty desktop"));
    }
    let name_len = u32::from_be_bytes([init[20], init[21], init[22], init[23]]) as usize;
    if name_len > 0 {
        let mut name = vec![0u8; name_len.min(1024)];
        stream.read_exact(&mut name).await.map_err(VncError::io)?;
    }

    // 5. Our pixel format + encodings.
    let mut format_message = [0u8; 20];
    format_message[0] = 0;
    format_message[4..20].copy_from_slice(&PixelFormat::xrgb32().to_wire());
    stream
        .write_all(&format_message)
        .await
        .map_err(VncError::io)?;
    let encodings = config.encodings();
    let mut encodings_message = Vec::with_capacity(4 + 4 * encodings.len());
    encodings_message.extend_from_slice(&[2, 0]);
    encodings_message.extend_from_slice(&(encodings.len() as u16).to_be_bytes());
    for encoding in encodings {
        encodings_message.extend_from_slice(&encoding.to_be_bytes());
    }
    stream
        .write_all(&encodings_message)
        .await
        .map_err(VncError::io)?;
    stream.flush().await.map_err(VncError::io)?;
    Ok((width, height))
}

async fn read_conn_failed<S: AsyncRead + Unpin>(stream: &mut S) -> Result<VncError, VncError> {
    let mut len = [0u8; 4];
    stream.read_exact(&mut len).await.map_err(VncError::io)?;
    let len = u32::from_be_bytes(len) as usize;
    let mut reason = vec![0u8; len.min(1024)];
    stream.read_exact(&mut reason).await.map_err(VncError::io)?;
    Ok(VncError::Security(
        String::from_utf8_lossy(&reason).into_owned(),
    ))
}

/// VNC password response: DES-ECB over both challenge halves with the
/// bit-reversed password as key (RFB §7.2.2).
pub fn vnc_auth_response(password: &[u8], challenge: &[u8; 16]) -> Result<[u8; 16], VncError> {
    use cipher::{generic_array::GenericArray, BlockEncrypt, KeyInit};

    let mut key = [0u8; 8];
    for (i, slot) in key.iter_mut().enumerate() {
        *slot = password.get(i).copied().unwrap_or(0).reverse_bits();
    }
    let cipher = des::Des::new_from_slice(&key).map_err(|err| VncError::Auth(err.to_string()))?;
    let mut first = *GenericArray::from_slice(&challenge[..8]);
    let mut second = *GenericArray::from_slice(&challenge[8..]);
    cipher.encrypt_block(&mut first);
    cipher.encrypt_block(&mut second);
    let mut out = [0u8; 16];
    out[..8].copy_from_slice(&first);
    out[8..].copy_from_slice(&second);
    Ok(out)
}

/// Open a TCP connection to `host:port` through an SSH `direct-tcpip`
/// channel (tunnel), reusing the SFTP-style dial pattern.
async fn dial_tunnel(
    tunnel: &SshTunnel,
    auth: crate::connection::ConnectionAuth,
    vnc_host: &str,
    vnc_port: u16,
) -> Result<BoxStream, VncError> {
    use russh::client;
    use std::sync::Arc;

    #[derive(Debug)]
    struct TunnelHandler {
        host: String,
        port: u16,
    }

    #[async_trait::async_trait]
    impl client::Handler for TunnelHandler {
        type Error = russh::Error;
        async fn check_server_key(
            &mut self,
            key: &russh::keys::key::PublicKey,
        ) -> Result<bool, Self::Error> {
            match russh::keys::check_known_hosts(&self.host, self.port, key) {
                Ok(accepted) => Ok(accepted),
                Err(error) => {
                    tracing::warn!(%error, "tunnel known_hosts check failed");
                    Ok(false)
                },
            }
        }
    }

    let config = Arc::new(client::Config {
        inactivity_timeout: Some(Duration::from_secs(30)),
        keepalive_interval: Some(Duration::from_secs(15)),
        keepalive_max: 3,
        ..Default::default()
    });
    let mut handle = client::connect(
        config,
        (tunnel.ssh_host.as_str(), tunnel.ssh_port),
        TunnelHandler {
            host: tunnel.ssh_host.clone(),
            port: tunnel.ssh_port,
        },
    )
    .await
    .map_err(|err| VncError::Ssh(err.to_string()))?;

    let authenticated = match auth {
        crate::connection::ConnectionAuth::Password(password) => handle
            .authenticate_password(&tunnel.ssh_username, password.as_str())
            .await
            .map_err(|err| VncError::Ssh(err.to_string()))?,
        crate::connection::ConnectionAuth::KeyFile { path, passphrase } => {
            let key = russh::keys::load_secret_key(path, passphrase.as_deref().map(String::as_str))
                .map_err(|err| VncError::Ssh(err.to_string()))?;
            handle
                .authenticate_publickey(&tunnel.ssh_username, Arc::new(key))
                .await
                .map_err(|err| VncError::Ssh(err.to_string()))?
        },
        _ => return Err(VncError::Ssh("tunnel needs password or key auth".into())),
    };
    if !authenticated {
        return Err(VncError::Ssh("tunnel authentication rejected".into()));
    }
    let channel = handle
        .channel_open_direct_tcpip(vnc_host, vnc_port as u32, "127.0.0.1", 0)
        .await
        .map_err(|err| VncError::Ssh(err.to_string()))?;
    Ok(Box::new(channel.into_stream()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    #[test]
    fn des_kat_proves_correct_wiring() {
        // Standard DES ECB known answer (FIPS-46): proves the `des` crate is
        // driven correctly before trusting it with VNC challenges.
        use cipher::{generic_array::GenericArray, BlockEncrypt, KeyInit};
        let cipher =
            des::Des::new_from_slice(&[0x13, 0x34, 0x57, 0x79, 0x9B, 0xBC, 0xDF, 0xF1]).unwrap();
        let mut block =
            *GenericArray::from_slice(&[0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF]);
        cipher.encrypt_block(&mut block);
        assert_eq!(
            &block[..],
            &[0x85, 0xE8, 0x13, 0x54, 0x0F, 0x0A, 0xB4, 0x05]
        );
    }

    #[test]
    fn auth_response_is_deterministic_and_password_sensitive() {
        let challenge = [0x11u8; 16];
        let a = vnc_auth_response(b"secret", &challenge).unwrap();
        let b = vnc_auth_response(b"secret", &challenge).unwrap();
        assert_eq!(a, b);
        assert_ne!(a, vnc_auth_response(b"other", &challenge).unwrap());
        // >8-char passwords truncate to 8.
        assert_eq!(
            vnc_auth_response(b"12345678", &challenge).unwrap(),
            vnc_auth_response(b"12345678EXTRA", &challenge).unwrap()
        );
    }

    #[test]
    fn config_address_and_encoding_order() {
        let config = VncConfig::direct("vnc.example", 1);
        assert_eq!(config.address(), "vnc.example:5901");
        assert_eq!(
            config.encodings(),
            vec![5, 2, 1, 0],
            "compressed default prefers Hextile"
        );
        let mut raw_first = config.clone();
        raw_first.compression = 0;
        assert_eq!(raw_first.encodings()[0], 0);
    }

    /// Strict-order mock RFB server speaking just enough for one Raw frame.
    async fn mock_server(
        mut stream: tokio::io::DuplexStream,
        payload: Vec<u8>,
    ) -> Result<(), String> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut buf = [0u8; 12];
        stream
            .write_all(b"RFB 003.008\n")
            .await
            .map_err(|e| e.to_string())?;
        stream
            .read_exact(&mut buf)
            .await
            .map_err(|e| e.to_string())?; // client version
        stream
            .write_all(&[1, SECURITY_NONE])
            .await
            .map_err(|e| e.to_string())?;
        let mut sel = [0u8; 1];
        stream
            .read_exact(&mut sel)
            .await
            .map_err(|e| e.to_string())?;
        stream
            .write_all(&0u32.to_be_bytes())
            .await
            .map_err(|e| e.to_string())?;
        let mut init = [0u8; 1];
        stream
            .read_exact(&mut init)
            .await
            .map_err(|e| e.to_string())?; // ClientInit
        let mut server_init = Vec::new();
        server_init.extend_from_slice(&2u16.to_be_bytes());
        server_init.extend_from_slice(&2u16.to_be_bytes());
        server_init.extend_from_slice(&PixelFormat::xrgb32().to_wire());
        server_init.extend_from_slice(&0u32.to_be_bytes());
        stream
            .write_all(&server_init)
            .await
            .map_err(|e| e.to_string())?;
        let mut rest = vec![0u8; 20 + 20];
        stream
            .read_exact(&mut rest)
            .await
            .map_err(|e| e.to_string())?; // format + encodings
        let mut request = [0u8; 10];
        stream
            .read_exact(&mut request)
            .await
            .map_err(|e| e.to_string())?; // update request
        stream
            .write_all(&payload)
            .await
            .map_err(|e| e.to_string())?; // one update
                                          // Serve follow-up incremental requests with empty updates.
        loop {
            let mut next = [0u8; 10];
            if stream.read_exact(&mut next).await.is_err() {
                break;
            }
            if stream.write_all(&[0, 0, 0, 0]).await.is_err() {
                break;
            }
        }
        Ok(())
    }

    fn raw_update_frame(pixels: [u32; 4]) -> Vec<u8> {
        let mut out = vec![0, 0, 0, 1]; // FramebufferUpdate, 1 rect
        out.extend_from_slice(&[0, 0, 0, 0, 0, 2, 0, 2]); // x,y,w,h
        out.extend_from_slice(&0i32.to_be_bytes()); // Raw
        for pixel in pixels {
            out.extend_from_slice(&[
                (pixel & 0xFF) as u8,
                ((pixel >> 8) & 0xFF) as u8,
                ((pixel >> 16) & 0xFF) as u8,
                0,
            ]);
        }
        out
    }

    #[tokio::test]
    async fn handshake_and_single_frame_against_mock() {
        let (client, server) = duplex(1 << 20);
        let server_task = tokio::spawn(mock_server(
            server,
            raw_update_frame([0xFFFF_0000, 0xFF00_FF00, 0xFF00_00FF, 0xFFFF_0000]),
        ));
        let config = VncConfig::direct("mock", 0);
        let session = VncSession::open(config, Box::new(client), None)
            .await
            .expect("handshake");
        tokio::time::sleep(Duration::from_millis(200)).await;
        let snapshot = session.latest_frame().expect("frame published");
        assert_eq!((snapshot.width, snapshot.height), (2, 2));
        // R channel of the top-left red pixel.
        assert_eq!(snapshot.rgba[0], 255);
        assert_eq!(snapshot.rgba[1], 0);
        session.disconnect().await;
        server_task.abort();
    }

    #[tokio::test]
    async fn view_only_rejects_input() {
        let (client, server) = duplex(1 << 20);
        let server_task = tokio::spawn(mock_server(server, raw_update_frame([0; 4])));
        let mut config = VncConfig::direct("mock", 0);
        config.view_only = true;
        let session = VncSession::open(config, Box::new(client), None)
            .await
            .expect("handshake");
        assert!(session
            .send_pointer(PointerEvent::new(1, 0, 0))
            .await
            .is_err());
        session.disconnect().await;
        server_task.abort();
    }
}

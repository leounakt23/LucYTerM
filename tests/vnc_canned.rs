//! VNC integration test against a canned in-process RFB server
//! (Prompt 4.3). Hermetic: loopback TCP only, no packages, no live
//! server, runs in the default suite.
//!
//! The server speaks just enough RFB 3.8 for the client's real
//! handshake (version, `None` auth, `ServerInit`) and answers update
//! requests with one solid Raw rect. Real TigerVNC/x11vnc interop stays
//! behind `MBXT_VNC_TEST_ADDR` in `vnc_integration.rs`.

#![cfg(feature = "vnc")]

use std::time::Duration;

use remote_app::connection::vnc::{VncConfig, VncSession};

const WIDTH: u16 = 64;
const HEIGHT: u16 = 48;
const PIXEL: u32 = 0x00FF_8040;

/// Serve exactly one client: handshake, then one solid Raw rect per
/// update request until EOF. Returns the bound port.
async fn start_canned_server() -> (u16, tokio::task::JoinHandle<()>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("loopback bind");
    let port = listener.local_addr().expect("addr").port();
    let task = tokio::spawn(async move {
        let Ok((mut socket, _)) = listener.accept().await else {
            return;
        };
        // 1. Version: the server speaks first.
        if socket.write_all(b"RFB 003.008\n").await.is_err() {
            return;
        }
        let mut version = [0u8; 12];
        if socket.read_exact(&mut version).await.is_err() {
            return;
        }
        // 2. Security: one type, None.
        if socket.write_all(&[1u8, 1u8]).await.is_err() {
            return;
        }
        let mut selected = [0u8; 1];
        if socket.read_exact(&mut selected).await.is_err() || selected[0] != 1 {
            return;
        }
        // 3. SecurityResult OK.
        if socket.write_all(&0u32.to_be_bytes()).await.is_err() {
            return;
        }
        // 4. ClientInit, then ServerInit (32bpp LE XRGB + name).
        let mut init = [0u8; 1];
        if socket.read_exact(&mut init).await.is_err() {
            return;
        }
        let mut server_init = Vec::new();
        server_init.extend_from_slice(&WIDTH.to_be_bytes());
        server_init.extend_from_slice(&HEIGHT.to_be_bytes());
        server_init.extend_from_slice(&[32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 16, 8, 0, 0, 0, 0]);
        server_init.extend_from_slice(&6u32.to_be_bytes());
        server_init.extend_from_slice(b"canned");
        if socket.write_all(&server_init).await.is_err() {
            return;
        }
        // 5. Client pixel format + encodings (length-delimited).
        let mut format = [0u8; 20];
        if socket.read_exact(&mut format).await.is_err() {
            return;
        }
        let mut enc_head = [0u8; 4];
        if socket.read_exact(&mut enc_head).await.is_err() {
            return;
        }
        let count = u16::from_be_bytes([enc_head[2], enc_head[3]]) as usize;
        let mut encodings = vec![0u8; count * 4];
        if !encodings.is_empty() && socket.read_exact(&mut encodings).await.is_err() {
            return;
        }
        // 6. Update requests: one solid Raw rect each.
        let pixels = PIXEL.to_le_bytes().repeat(WIDTH as usize * HEIGHT as usize);
        loop {
            let mut kind = [0u8; 1];
            if socket.read_exact(&mut kind).await.is_err() {
                return;
            }
            if kind[0] != 3 {
                return;
            }
            let mut request = [0u8; 9];
            if socket.read_exact(&mut request).await.is_err() {
                return;
            }
            let mut update = Vec::with_capacity(4 + 12 + pixels.len());
            update.extend_from_slice(&[0u8, 0u8]); // FramebufferUpdate
            update.extend_from_slice(&1u16.to_be_bytes());
            update.extend_from_slice(&0u16.to_be_bytes()); // x
            update.extend_from_slice(&0u16.to_be_bytes()); // y
            update.extend_from_slice(&WIDTH.to_be_bytes());
            update.extend_from_slice(&HEIGHT.to_be_bytes());
            update.extend_from_slice(&0i32.to_be_bytes()); // Raw
            update.extend_from_slice(&pixels);
            if socket.write_all(&update).await.is_err() {
                return;
            }
        }
    });
    (port, task)
}

#[tokio::test]
async fn canned_server_handshake_and_first_frame() {
    let (port, server) = start_canned_server().await;
    let mut config = VncConfig::direct("127.0.0.1", 0);
    config.port = port;
    let session = tokio::time::timeout(
        Duration::from_secs(10),
        VncSession::connect_tcp(config, None),
    )
    .await
    .expect("connect in time")
    .expect("RFB handshake");
    session.request_full_refresh().await.expect("refresh");
    let frame = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(frame) = session.latest_frame() {
                return frame;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("frame in time");
    assert_eq!(
        frame.rgba.len(),
        WIDTH as usize * HEIGHT as usize * 4,
        "frame shape"
    );
    assert_eq!(session.desktop_size(), (WIDTH, HEIGHT));
    assert!(
        frame
            .rgba
            .chunks_exact(4)
            .all(|pixel| pixel == [0xFF, 0x80, 0x40, 0xFF]),
        "solid canned color decodes through XRGB32"
    );
    session.disconnect().await;
    server.abort();
}

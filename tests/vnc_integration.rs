//! VNC integration test against a real server (Prompt 4.3).
//!
//! Ignored by default — needs a reachable VNC server without a password
//! (e.g. `x11vnc -nopw` or TigerVNC with `SecurityTypes=None` in CI):
//!
//! ```sh
//! MBXT_VNC_TEST_ADDR=127.0.0.1:5900 \
//! cargo test --test vnc_integration --features vnc -- --ignored
//! ```
//!
//! Without env vars the harness reports a single passing skip so plain
//! `cargo test` stays green. The embedded mock-server path is covered by
//! unit tests in `connection::vnc::session` instead.

use std::env;

fn live_addr() -> Option<String> {
    env::var("MBXT_VNC_TEST_ADDR").ok()
}

#[test]
fn requires_live_server_env() {
    if live_addr().is_none() {
        eprintln!("MBXT_VNC_TEST_ADDR not set; live VNC test skipped");
    }
}

#[cfg(feature = "vnc")]
#[tokio::test]
#[ignore = "needs a live VNC server (see module docs)"]
async fn handshake_and_first_frame() {
    use remote_app::connection::vnc::{VncConfig, VncSession};

    let Some(addr) = live_addr() else {
        return;
    };
    let (host, port) = addr.rsplit_once(':').expect("HOST:PORT");
    let mut config = VncConfig::direct(host, 0);
    config.port = port.parse().expect("numeric port");
    let session = VncSession::connect_tcp(config, None)
        .await
        .expect("RFB handshake");
    session.request_full_refresh().await.expect("refresh");
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let frame = session.latest_frame().expect("first frame published");
    assert!(!frame.rgba.is_empty());
    assert_eq!(
        frame.rgba.len(),
        frame.width as usize * frame.height as usize * 4
    );
    session.disconnect().await;
}

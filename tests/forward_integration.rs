//! Port-forwarding integration tests against a real SSH server (Prompt 5.3).
//!
//! Ignored by default — needs a reachable OpenSSH server (e.g. in CI):
//!
//! ```sh
//! MBXT_FORWARD_TEST_ADDR=127.0.0.1:2222 \
//! MBXT_FORWARD_TEST_USER=test MBXT_FORWARD_TEST_PASSWORD=test \
//! cargo test --test forward_integration --features ssh -- --ignored
//! ```
//!
//! The data path under test is looped back through the SSH port itself
//! (the `SSH-` banner is readable with no extra services): a `direct-tcpip`
//! channel to port 22 must yield the banner, a `-R` listen must be granted
//! and releasable, and a SOCKS `CONNECT` to port 22 must relay the banner.
//! Without env vars the harness reports a passing skip so plain `cargo
//! test` stays green.

use std::env;

fn live_config() -> Option<(String, String, String)> {
    let addr = env::var("MBXT_FORWARD_TEST_ADDR").ok()?;
    let user = env::var("MBXT_FORWARD_TEST_USER").ok()?;
    let password = env::var("MBXT_FORWARD_TEST_PASSWORD").ok()?;
    Some((addr, user, password))
}

#[test]
fn requires_live_server_env() {
    if live_config().is_none() {
        eprintln!("MBXT_FORWARD_TEST_ADDR not set; live forwarding tests skipped");
    }
}

#[cfg(feature = "ssh")]
#[tokio::test]
#[ignore = "needs a live SSH server (see module docs)"]
async fn direct_channel_reads_ssh_banner() {
    use remote_app::connection::forward::dial_tunnel;
    use remote_app::connection::ConnectionAuth;
    use tokio::io::AsyncReadExt;

    let Some((addr, user, password)) = live_config() else {
        return;
    };
    remote_app::test_utils::learn_live_host_key(&addr);
    let (host, port) = addr.rsplit_once(':').expect("HOST:PORT");
    let auth = ConnectionAuth::Password(zeroize::Zeroizing::new(password));
    let dial = dial_tunnel(host, port.parse().expect("port"), &user, auth)
        .await
        .expect("tunnel dial");
    let mut stream = dial
        .handle
        .channel_open_direct_tcpip(host, 22, "127.0.0.1", 0)
        .await
        .expect("direct-tcpip open")
        .into_stream();
    let mut banner = [0u8; 4];
    stream.read_exact(&mut banner).await.expect("banner");
    assert_eq!(&banner, b"SSH-", "looped back through the SSH port itself");
}

#[cfg(feature = "ssh")]
#[tokio::test]
#[ignore = "needs a live SSH server (see module docs)"]
async fn remote_listen_granted_and_released() {
    use remote_app::connection::forward::dial_tunnel;
    use remote_app::connection::ConnectionAuth;

    let Some((addr, user, password)) = live_config() else {
        return;
    };
    remote_app::test_utils::learn_live_host_key(&addr);
    let (host, port) = addr.rsplit_once(':').expect("HOST:PORT");
    let auth = ConnectionAuth::Password(zeroize::Zeroizing::new(password));
    let dial = dial_tunnel(host, port.parse().expect("port"), &user, auth)
        .await
        .expect("tunnel dial");
    let mut handle = dial.handle;
    let bound = handle
        .tcpip_forward("127.0.0.1", 0)
        .await
        .expect("server grants a remote listen");
    assert!(bound > 0, "server picked a port");
    handle
        .cancel_tcpip_forward("127.0.0.1", bound)
        .await
        .expect("release succeeds");
}

#[cfg(feature = "ssh")]
#[tokio::test]
#[ignore = "needs a live SSH server (see module docs)"]
async fn socks_connect_relays_banner() {
    use remote_app::connection::forward::{dial_tunnel, serve_socks5};
    use remote_app::connection::ConnectionAuth;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let Some((addr, user, password)) = live_config() else {
        return;
    };
    remote_app::test_utils::learn_live_host_key(&addr);
    let (host, port) = addr.rsplit_once(':').expect("HOST:PORT");
    let port: u16 = port.parse().expect("port");
    let auth = ConnectionAuth::Password(zeroize::Zeroizing::new(password));
    let dial = dial_tunnel(host, port, &user, auth)
        .await
        .expect("tunnel dial");

    let (mut client, mut server) = tokio::io::duplex(65536);
    let relay = tokio::spawn(async move {
        serve_socks5(&mut server, |target_host, target_port| async move {
            dial.handle
                .channel_open_direct_tcpip(target_host, u32::from(target_port), "127.0.0.1", 0)
                .await
                .map(|channel| channel.into_stream())
                .map_err(|err| {
                    remote_app::connection::forward::SocksError::Protocol(err.to_string())
                })
        })
        .await
    });
    // SOCKS CONNECT 127.0.0.1:22 (the SSH port itself).
    client.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
    let mut methods = [0u8; 2];
    client.read_exact(&mut methods).await.unwrap();
    assert_eq!(methods, [0x05, 0x00]);
    client
        .write_all(&[0x05, 0x01, 0x00, 0x01, 127, 0, 0, 1, 0, 22])
        .await
        .unwrap();
    let mut reply = [0u8; 10];
    client.read_exact(&mut reply).await.unwrap();
    assert_eq!(reply[1], 0x00, "SOCKS relay established");
    let mut banner = [0u8; 4];
    client.read_exact(&mut banner).await.unwrap();
    assert_eq!(&banner, b"SSH-");
    drop(client);
    let _ = relay.await;
}

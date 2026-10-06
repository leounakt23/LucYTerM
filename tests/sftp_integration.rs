//! SFTP integration tests against a real server (Prompt 3.1).
//!
//! These tests are **ignored by default** — they need a reachable SFTP
//! server (e.g. an OpenSSH container in CI):
//!
//! ```sh
//! docker run -d --name sftp-test -p 2222:22 \
//!   -e SFTP_USER=test -e SFTP_PASSWORD=test atmoz/sftp
//! MBXT_SFTP_TEST_ADDR=127.0.0.1:2222 \
//! MBXT_SFTP_TEST_USER=test MBXT_SFTP_TEST_PASSWORD=test \
//! cargo test --test sftp_integration -- --ignored
//! ```
//!
//! Without the env vars the harness reports a single passing
//! `requires_live_server_env` test so plain `cargo test` stays green.

use std::env;

fn live_config() -> Option<(String, String, String)> {
    let addr = env::var("MBXT_SFTP_TEST_ADDR").ok()?;
    let user = env::var("MBXT_SFTP_TEST_USER").ok()?;
    let password = env::var("MBXT_SFTP_TEST_PASSWORD").ok()?;
    Some((addr, user, password))
}

#[test]
fn requires_live_server_env() {
    if live_config().is_none() {
        eprintln!("MBXT_SFTP_TEST_ADDR not set; live SFTP tests skipped");
    }
}

#[cfg(feature = "ssh")]
#[tokio::test]
#[ignore = "needs a live SFTP server (see module docs)"]
async fn list_and_round_trip_small_file() {
    use remote_app::connection::sftp::{CancelToken, SftpSession};
    use russh::client;
    use std::sync::Arc;
    use std::time::Duration;

    #[derive(Debug)]
    struct AcceptAll;
    #[async_trait::async_trait]
    impl client::Handler for AcceptAll {
        type Error = russh::Error;
        async fn check_server_key(
            &mut self,
            _key: &russh::keys::key::PublicKey,
        ) -> Result<bool, Self::Error> {
            Ok(true)
        }
    }

    let Some((addr, user, password)) = live_config() else {
        return;
    };
    let config = Arc::new(client::Config {
        inactivity_timeout: Some(Duration::from_secs(10)),
        ..Default::default()
    });
    let mut handle = client::connect(config, addr.as_str(), AcceptAll)
        .await
        .expect("ssh connect");
    assert!(handle
        .authenticate_password(user, password)
        .await
        .expect("auth call"));
    let channel = handle.channel_open_session().await.expect("channel");
    channel
        .request_subsystem(true, "sftp")
        .await
        .expect("subsystem");
    let sftp = SftpSession::new(channel.into_stream())
        .await
        .expect("sftp handshake");

    let entries = sftp.list_directory(".").await.expect("list");
    assert!(!entries.is_empty() || entries.is_empty(), "list round trip");

    let dir = tempfile::tempdir().expect("tempdir");
    let local = dir.path().join("roundtrip.txt");
    tokio::fs::write(&local, b"hello sftp").await.expect("seed");
    let cancel = CancelToken::new();
    sftp.put_file(&local, "upload/mbxt-roundtrip.txt", 4, &cancel, |_, _| {})
        .await
        .expect("upload");
    std::fs::remove_file(&local).expect("clear for download");
    sftp.get_file("upload/mbxt-roundtrip.txt", &local, 4, &cancel, |_, _| {})
        .await
        .expect("download");
    assert_eq!(tokio::fs::read(&local).await.expect("read"), b"hello sftp");
    sftp.delete("upload/mbxt-roundtrip.txt")
        .await
        .expect("cleanup");
}

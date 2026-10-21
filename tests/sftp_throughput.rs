//! SFTP throughput (M5: > 100 MB/s on the local network).
//!
//! Uploads then downloads a large file against a disposable SFTP container
//! (pattern follows `tests/sftp_integration.rs`: strict host key via
//! throwaway HOME) and reports up/down MB/s. Default size is 512 MiB
//! (reliable end-to-end in ~20 s); `MBXT_SFTP_PERF_BYTES` overrides it.
//!
//! Resolved finding (2026-10-15, russh 0.61.1): the 1 GiB download stall
//! recorded on 2026-10-14 under russh 0.45.0 (reproducible at ~800 MiB,
//! pipeline depths 4 and 16; server exonerated via the OpenSSH `sftp`
//! CLI) was the client rejecting mid-session rekey
//! (`StrictKeyExchangeViolation`). 0.61.1 handles rekey; 1 GiB up/down now
//! completes with verified checksums. Set `MBXT_SFTP_PERF_BYTES=1073741824`
//! to exercise it.
//!
//! ```sh
//! docker run -d --name sftp-perf -p 2222:22 \
//!   -e SFTP_USER=test -e SFTP_PASSWORD=test atmoz/sftp
//! MBXT_SFTP_TEST_ADDR=127.0.0.1:2222 \
//! MBXT_SFTP_TEST_USER=test MBXT_SFTP_TEST_PASSWORD=test \
//! cargo test --test sftp_throughput -- --ignored --nocapture
//! ```
//!
//! Without the env vars the harness reports a single passing
//! `requires_live_server_env` test so plain `cargo test` stays green.
//! Skips cleanly (passing, with a reason) when the temp filesystem has
//! less than size + 512 MiB headroom. Only TEST-NET / disposable docker
//! hosts: never point this at production.

#![cfg(feature = "sftp")]

use std::env;
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn live_config() -> Option<(String, String, String)> {
    let addr = env::var("MBXT_SFTP_TEST_ADDR").ok()?;
    let user = env::var("MBXT_SFTP_TEST_USER").ok()?;
    let password = env::var("MBXT_SFTP_TEST_PASSWORD").ok()?;
    Some((addr, user, password))
}

#[test]
fn requires_live_server_env() {
    if live_config().is_none() {
        eprintln!("MBXT_SFTP_TEST_ADDR not set; SFTP throughput tests skipped");
    }
}

/// Bytes requested for the throughput file (default 512 MiB; 1 GiB via
/// `MBXT_SFTP_PERF_BYTES=1073741824` reproduces the known download stall).
fn target_bytes() -> u64 {
    env::var("MBXT_SFTP_PERF_BYTES")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|n| *n >= 1024 * 1024)
        .unwrap_or(512 * 1024 * 1024)
}

/// SFTP pipeline depth (`MBXT_SFTP_PERF_DEPTH`, default 16 → 1 MiB chunks).
fn pipeline_depth() -> u32 {
    env::var("MBXT_SFTP_PERF_DEPTH")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|n| *n >= 1 && *n <= 64)
        .unwrap_or(16)
}

/// Available bytes on the filesystem holding `path` (via `df -B1`).
fn fs_avail_bytes(path: &std::path::Path) -> Option<u64> {
    let output = std::process::Command::new("df")
        .args(["--output=avail", "-B1"])
        .arg(path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .nth(1)
        .and_then(|line| line.trim().parse().ok())
}

/// Deterministic 1 MiB chunk (no RNG dependency, reproducible content).
fn chunk_1m() -> Vec<u8> {
    (0..1024 * 1024).map(|i| (i % 251) as u8).collect()
}

/// Streaming checksum (u64 wrapping sum): verifies content without holding
/// the whole file in memory.
fn checksum_file(path: &std::path::Path) -> std::io::Result<(u64, u64)> {
    use std::io::Read as _;
    let mut file = std::fs::File::open(path)?;
    let mut sum = 0u64;
    let mut len = 0u64;
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        for byte in &buf[..n] {
            sum = sum.wrapping_add(u64::from(*byte));
        }
        len += n as u64;
    }
    Ok((sum, len))
}

#[cfg(feature = "ssh")]
#[tokio::test]
#[ignore = "needs a live SFTP server (see module docs)"]
async fn throughput_up_down_reports_mb_per_s() {
    use remote_app::connection::sftp::CancelToken;
    use russh::client;
    use std::sync::Arc;

    #[derive(Debug)]
    struct StrictHostKey {
        host: String,
        port: u16,
    }
    impl client::Handler for StrictHostKey {
        type Error = russh::Error;
        async fn check_server_key(
            &mut self,
            key: &russh::keys::PublicKeyOrCertificate,
        ) -> Result<bool, Self::Error> {
            match mbxt_connections::ssh::verify_host_key(&self.host, self.port, key) {
                Ok(accepted) => Ok(accepted),
                Err(error) => {
                    tracing::warn!(%error, host = %self.host, "known_hosts check failed");
                    Ok(false)
                },
            }
        }
    }

    let Some((addr, user, password)) = live_config() else {
        return;
    };
    let size = target_bytes();
    let dir = tempfile::tempdir().expect("tempdir");
    let local: PathBuf = dir.path().join("mbxt-throughput.bin");
    let headroom: u64 = 512 * 1024 * 1024;
    match fs_avail_bytes(dir.path()) {
        Some(avail) if avail < size + headroom => {
            eprintln!(
                "[measure] sftp throughput SKIPPED: tmpfs avail {avail} < size {size} + headroom {headroom}"
            );
            return;
        },
        Some(avail) => {
            eprintln!("[measure] sftp throughput: tmpfs avail {avail} bytes, file {size} bytes")
        },
        None => eprintln!("[measure] sftp throughput: tmpfs avail unknown, attempting anyway"),
    }

    // Seed the local file in 1 MiB streaming writes (never whole file in RAM).
    {
        use std::io::Write as _;
        let chunk = chunk_1m();
        let mut file = std::fs::File::create(&local).expect("seed file");
        let mut remaining = size;
        while remaining > 0 {
            let take = (chunk.len() as u64).min(remaining) as usize;
            file.write_all(&chunk[..take]).expect("seed write");
            remaining -= take as u64;
        }
        file.sync_all().expect("seed sync");
    }
    let (seed_sum, seed_len) = checksum_file(&local).expect("seed checksum");
    assert_eq!(seed_len, size);

    remote_app::test_utils::learn_live_host_key(&addr);
    // One SSH+SFTP connection per direction. Rationale (measured): a single
    // connection that carries 1 GiB up AND the 1 GiB download reproducibly
    // stalls the download at ~806 MiB (same offset at depth 4 and 16;
    // OpenSSH CLI downloads the same file in ~2 s, so the server is fine).
    // Per-direction connections are also what the product does (one SFTP
    // channel per SSH session, fresh on reconnect), so both legs stay
    // product-path measurements.
    async fn connect_sftp(
        addr: &str,
        user: &str,
        password: &str,
    ) -> (
        russh::client::Handle<StrictHostKey>,
        remote_app::connection::sftp::SftpSession,
    ) {
        use remote_app::connection::sftp::SftpSession;
        let (host, port) = addr.rsplit_once(':').expect("HOST:PORT");
        let config = Arc::new(client::Config {
            inactivity_timeout: Some(Duration::from_secs(60)),
            ..Default::default()
        });
        let mut handle = client::connect(
            config,
            addr,
            StrictHostKey {
                host: host.to_string(),
                port: port.parse().expect("numeric port"),
            },
        )
        .await
        .expect("ssh connect");
        assert!(handle
            .authenticate_password(user, password)
            .await
            .expect("auth call")
            .success());
        let channel = handle.channel_open_session().await.expect("channel");
        channel
            .request_subsystem(true, "sftp")
            .await
            .expect("subsystem");
        // Generous per-request timeout for multi-GB transfers under debug
        // profiles: it only tolerates a slow peer, it cannot inflate MB/s
        // (throughput derives from wall-clock bytes, not from this budget).
        let raw = russh_sftp::client::SftpSession::new_with_config(
            channel.into_stream(),
            russh_sftp::client::Config {
                request_timeout_secs: 120,
                ..Default::default()
            },
        )
        .await
        .expect("sftp handshake");
        (handle, SftpSession::from_client(raw))
    }

    let (up_handle, up_sftp) = connect_sftp(&addr, &user, &password).await;

    let remote = format!("upload/mbxt-throughput-{}.bin", std::process::id());
    let cancel = CancelToken::new();
    let depth = pipeline_depth();
    eprintln!("[measure] sftp throughput: pipeline_depth={depth}");

    let up_started = Instant::now();
    let mut up_mark = 0u64;
    let up_bytes = up_sftp
        .put_file(&local, &remote, depth, &cancel, |done, _| {
            if done - up_mark >= 256 * 1024 * 1024 {
                up_mark = done;
                eprintln!(
                    "[measure] sftp upload progress {done}/{size} ({:.1}s)",
                    up_started.elapsed().as_secs_f64()
                );
            }
        })
        .await
        .expect("upload");
    let up_secs = up_started.elapsed().as_secs_f64();
    assert_eq!(up_bytes, size);
    let up_mbs = size as f64 / up_secs / 1_000_000.0;

    std::fs::remove_file(&local).expect("clear for download");
    let down_started = Instant::now();
    let mut last_mark = 0u64;
    // Fresh connection for the download leg (see above): the download must
    // not share a connection that already carried 1 GiB up.
    let (down_handle, down_sftp) = connect_sftp(&addr, &user, &password).await;
    up_handle
        .disconnect(russh::Disconnect::ByApplication, "", "")
        .await
        .expect("up bye");
    let down_bytes = down_sftp
        .get_file(&remote, &local, depth, &cancel, |done, _| {
            if done - last_mark >= 256 * 1024 * 1024 {
                last_mark = done;
                eprintln!(
                    "[measure] sftp download progress {done}/{size} ({:.1}s)",
                    down_started.elapsed().as_secs_f64()
                );
            }
        })
        .await
        .expect("download");
    let down_secs = down_started.elapsed().as_secs_f64();
    assert_eq!(down_bytes, size);
    let down_mbs = size as f64 / down_secs / 1_000_000.0;

    let (roundtrip_sum, roundtrip_len) = checksum_file(&local).expect("roundtrip checksum");
    assert_eq!(roundtrip_len, size, "downloaded length mismatch");
    assert_eq!(roundtrip_sum, seed_sum, "downloaded content mismatch");

    eprintln!(
        "[measure] sftp throughput addr={addr} size={size} up={up_mbs:.1}MB/s ({up_secs:.2}s) down={down_mbs:.1}MB/s ({down_secs:.2}s) target=100MB/s"
    );

    down_sftp.delete(&remote).await.expect("cleanup");
    down_handle
        .disconnect(russh::Disconnect::ByApplication, "", "")
        .await
        .expect("bye");
}

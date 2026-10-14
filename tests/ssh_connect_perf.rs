//! SSH connect performance (M2: handshake < 3 s).
//!
//! Measures `N` consecutive password handshakes against a disposable
//! OpenSSH container plus one echo round-trip, reporting median/p95.
//! Pattern follows `tests/ssh_auth_live.rs` (strict host key via throwaway
//! HOME) and `tests/ssh_loopback.rs` (3 s handshake budget).
//!
//! ```sh
//! docker run -d --name openssh-perf -p 2223:22 \
//!   -e PUID=1000 -e PGID=1000 -e USER_NAME=test -e USER_PASSWORD=test \
//!   -e PASSWORD_ACCESS=true \
//!   -v $PWD/docker/sshd_config:/config/sshd/sshd_config \
//!   lscr.io/linuxserver/openssh-server:latest
//! MBXT_SSH_TEST_ADDR=127.0.0.1:2223 \
//! MBXT_SSH_TEST_USER=test MBXT_SSH_TEST_PASSWORD=test \
//! cargo test --test ssh_connect_perf -- --ignored --nocapture
//! ```
//!
//! Without the env vars the harness reports a single passing
//! `requires_live_server_env` test so plain `cargo test` stays green.
//! `MBXT_SSH_PERF_ITERS` overrides N (default 10). Only TEST-NET /
//! disposable docker hosts: never point this at production.

#![cfg(feature = "ssh")]

use std::env;
use std::time::{Duration, Instant};

use mbxt_connections::{Connection, ConnectionAuth, ConnectionEvent, TerminalSize};
use mbxt_core::{AuthMethod, Protocol, SessionSpec};
use zeroize::Zeroizing;

fn live_config() -> Option<(String, String, String)> {
    let addr = env::var("MBXT_SSH_TEST_ADDR").ok()?;
    let user = env::var("MBXT_SSH_TEST_USER").ok()?;
    let password = env::var("MBXT_SSH_TEST_PASSWORD").ok()?;
    Some((addr, user, password))
}

#[test]
fn requires_live_server_env() {
    if live_config().is_none() {
        eprintln!("MBXT_SSH_TEST_ADDR not set; SSH connect-perf tests skipped");
    }
}

fn block_on<F: std::future::Future>(fut: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
        .block_on(fut)
}

fn spec(addr: &str, user: &str) -> SessionSpec {
    let (host, port) = addr.rsplit_once(':').expect("HOST:PORT");
    SessionSpec {
        name: "ssh-connect-perf".into(),
        protocol: Protocol::Ssh,
        host: Some(host.to_string()),
        port: Some(port.parse().expect("numeric port")),
        username: Some(user.to_string()),
        auth: AuthMethod::Password,
        tags: Vec::new(),
        notes: String::new(),
        x11_forwarding: false,
        serial: None,
        forwards: Vec::new(),
    }
}

fn iters() -> usize {
    env::var("MBXT_SSH_PERF_ITERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|n| *n > 0 && *n <= 100)
        .unwrap_or(10)
}

fn median_ms(mut samples: Vec<u128>) -> u128 {
    samples.sort_unstable();
    samples[samples.len() / 2]
}

fn percentile_ms(mut samples: Vec<u128>, p: f64) -> u128 {
    samples.sort_unstable();
    let rank = ((p / 100.0) * (samples.len() as f64)).ceil() as usize;
    samples[rank.saturating_sub(1).min(samples.len() - 1)]
}

#[test]
#[ignore = "needs a live OpenSSH server (see module docs)"]
fn connect_latency_n10_reports_median_p95() {
    let Some((addr, user, password)) = live_config() else {
        return;
    };
    remote_app::test_utils::learn_live_host_key(&addr);
    let n = iters();
    let session_spec = spec(&addr, &user);
    let mut handshakes_ms: Vec<u128> = Vec::with_capacity(n);

    for i in 0..n {
        let started = Instant::now();
        block_on(async {
            let mut conn = mbxt_connections::ssh::SshConn::new(&session_spec);
            tokio::time::timeout(
                Duration::from_secs(30),
                conn.start(
                    ConnectionAuth::Password(Zeroizing::new(password.clone())),
                    TerminalSize::default(),
                ),
            )
            .await
            .unwrap_or_else(|_| panic!("connect {i} timed out"))
            .unwrap_or_else(|err| panic!("connect {i} failed: {err}"));
            let elapsed = started.elapsed();
            handshakes_ms.push(elapsed.as_millis());
            conn.shutdown().await.expect("shutdown");
        });
    }

    let median = median_ms(handshakes_ms.clone());
    let p95 = percentile_ms(handshakes_ms.clone(), 95.0);
    let max = handshakes_ms.iter().copied().max().unwrap_or(0);
    eprintln!(
        "[measure] ssh connect N={n} addr={addr} median={median}ms p95={p95}ms max={max}ms samples={handshakes_ms:?}"
    );
    assert!(
        median < 3000,
        "M2 target: median SSH handshake < 3000 ms, got {median} ms (N={n})"
    );
    assert!(
        max < 30_000,
        "every handshake must finish inside the 30 s dial timeout (max {max} ms)"
    );
}

#[test]
#[ignore = "needs a live OpenSSH server (see module docs)"]
fn shell_echo_latency_single_probe() {
    let Some((addr, user, password)) = live_config() else {
        return;
    };
    remote_app::test_utils::learn_live_host_key(&addr);
    let session_spec = spec(&addr, &user);
    block_on(async {
        let mut conn = mbxt_connections::ssh::SshConn::new(&session_spec);
        tokio::time::timeout(
            Duration::from_secs(30),
            conn.start(
                ConnectionAuth::Password(Zeroizing::new(password)),
                TerminalSize::default(),
            ),
        )
        .await
        .expect("connect in time")
        .expect("live login");
        let probe = b"mbxt-echo-perf-probe";
        let started = Instant::now();
        conn.write(probe).await.expect("write");
        let mut buffer = Vec::new();
        let echoed = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let event = conn.next_event().await.expect("no transport error");
                if let ConnectionEvent::Output(bytes) = event {
                    buffer.extend_from_slice(&bytes);
                    if buffer.windows(probe.len()).any(|w| w == probe) {
                        return true;
                    }
                } else {
                    return false;
                }
            }
        })
        .await
        .expect("echo in time");
        let elapsed_ms = started.elapsed().as_millis();
        eprintln!(
            "[measure] ssh shell echo addr={addr} latency={elapsed_ms}ms bytes={}",
            buffer.len()
        );
        assert!(echoed, "probe bytes missing from shell output ({buffer:?})");
        conn.shutdown().await.expect("shutdown");
    });
}

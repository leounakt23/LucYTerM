//! M5 acceptance soak: 10 concurrent SSH shell sessions (8 full hours) +
//! one looping 128 MiB SFTP transfer, with in-process RSS sampling.
//!
//! Disposable TEST-NET services only (never arbitrary hosts):
//!
//! ```sh
//! docker run -d --rm --name mbxt-soak-ssh -p 2323:22 -e PUID=1000 -e PGID=1000 \
//!   -e USER_NAME=test -e USER_PASSWORD=test -e PASSWORD_ACCESS=true \
//!   -v ./docker/sshd_config:/config/sshd/sshd_config \
//!   lscr.io/linuxserver/openssh-server:latest
//! docker run -d --rm --name mbxt-soak-sftp -p 2324:22 \
//!   --tmpfs /home/test/upload atmoz/sftp:alpine test:test:1001::upload
//! cargo test --test soak_acceptance -- --ignored --nocapture
//! ```
//!
//! Stay off ports 2222/2223 (sibling SFTP-fix workstream owns those).
//!
//! Behavior:
//! - `N` (default 10) concurrent `SshConn` password shell sessions against
//!   `MBXT_SOAK_SSH_ADDR` (default `127.0.0.1:2323`, `test`/`test`), each
//!   sending a tick command every 30 s and keeping FULL scrollback (every
//!   output byte ever received is retained in driver memory, never
//!   truncated). Reconnects with backoff on transport errors.
//! - One SFTP loop against `MBXT_SOAK_SFTP_ADDR` (default
//!   `127.0.0.1:2324`): upload 128 MiB, download it back, verify FNV-1a
//!   checksum, delete remote, repeat until the deadline.
//! - A sampler task appends one row every 60 s to
//!   `target/soak-rss.csv`: `VmRSS`/`VmHWM` from `/proc/self/status` plus
//!   `Rss`/`Pss` from `/proc/self/smaps_rollup`. Every sample is real.
//! - Duration: `MBXT_SOAK_SECS` (default 28800 = 8 h). Shorter values are
//!   for smoke-testing only and never count as acceptance evidence.
//!
//! Acceptance criteria (evaluated post-hoc from the CSV, not in-proc):
//! PASS = peak VmHWM < 300 MiB (307200 KiB) AND |linear-fit slope of VmRSS
//! over hours 2-8| < 1 MiB/h (1024 KiB/h).

#![cfg(feature = "ssh")]

use std::env;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use mbxt_connections::{Connection, ConnectionAuth, ConnectionEvent, TerminalSize};
use mbxt_core::{AuthMethod, Protocol, SessionSpec};
use zeroize::Zeroizing;

/// 128 MiB transfer size for the looping SFTP leg.
const FILE_BYTES: u64 = 128 * 1024 * 1024;
/// Shell tick period per session.
const TICK_SECS: u64 = 30;
/// RSS sample period.
const SAMPLE_SECS: u64 = 60;

#[derive(Debug, Default)]
struct SoakStats {
    sessions_alive: AtomicU64,
    scrollback_bytes: AtomicU64,
    ticks_total: AtomicU64,
    ssh_reconnects: AtomicU64,
    sftp_reconnects: AtomicU64,
    sftp_iters: AtomicU64,
    sftp_bytes_total: AtomicU64,
}

fn env_or(key: &str, fallback: &str) -> String {
    env::var(key).unwrap_or_else(|_| fallback.to_string())
}

fn manifest_target(child: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(child)
}

fn unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Learn BOTH live host keys into one throwaway HOME so the strict
/// `known_hosts` check passes exactly as in production.
///
/// (Unlike `learn_live_host_key`, which overwrites `known_hosts` per call,
/// this concatenates both servers' keys. Never touches the user's HOME.)
fn learn_both_host_keys(ssh_addr: &str, sftp_addr: &str) {
    let home = env::temp_dir().join(format!("mbxt-soak-home-{}", std::process::id()));
    let ssh_dir = home.join(".ssh");
    std::fs::create_dir_all(&ssh_dir).expect("temp ssh dir");
    let mut known = Vec::new();
    for addr in [ssh_addr, sftp_addr] {
        let (host, port) = addr.rsplit_once(':').expect("HOST:PORT");
        let output = std::process::Command::new("ssh-keyscan")
            .args(["-p", port, "-t", "rsa,ecdsa,ed25519", host])
            .output()
            .expect("ssh-keyscan runs (openssh-client required)");
        assert!(
            !output.stdout.is_empty(),
            "ssh-keyscan found no host keys at {addr}"
        );
        known.extend_from_slice(&output.stdout);
    }
    std::fs::write(ssh_dir.join("known_hosts"), &known).expect("write known_hosts");
    env::set_var("HOME", &home);
    eprintln!("[soak] learned host keys for {ssh_addr} + {sftp_addr}");
}

fn ssh_spec(addr: &str, user: &str, id: usize) -> SessionSpec {
    let (host, port) = addr.rsplit_once(':').expect("HOST:PORT");
    SessionSpec {
        name: format!("soak-{id}"),
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

/// Deterministic 128 MiB upload file (xorshift64* stream); returns FNV-1a.
fn prepare_upload_file(path: &Path) -> u64 {
    if path.exists() {
        let hash = fnv_file(path);
        eprintln!(
            "[soak] upload file cached: {} hash={hash:016x}",
            path.display()
        );
        assert_eq!(
            std::fs::metadata(path).expect("stat").len(),
            FILE_BYTES,
            "stale upload file"
        );
        return hash;
    }
    let mut state: u64 = 0x9E3779B97F4A7C15;
    let mut block = vec![0u8; 1 << 20];
    let mut file = std::fs::File::create(path).expect("create upload file");
    let mut hash: u64 = 0xcbf29ce484222325;
    let mut remaining = FILE_BYTES;
    while remaining > 0 {
        for byte in block.iter_mut() {
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            *byte = (state.wrapping_mul(0x2545F4914F6CDD1D) >> 56) as u8;
        }
        for &b in &block {
            hash ^= b as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        let n = remaining.min(block.len() as u64) as usize;
        file.write_all(&block[..n]).expect("write upload file");
        remaining -= n as u64;
    }
    file.sync_all().expect("sync upload file");
    eprintln!(
        "[soak] upload file ready: {} bytes hash={hash:016x}",
        path.display()
    );
    hash
}

fn fnv_file(path: &Path) -> u64 {
    use std::io::Read;
    let mut hash: u64 = 0xcbf29ce484222325;
    let mut file = std::fs::File::open(path).expect("open for hash");
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf).expect("read for hash");
        if n == 0 {
            break;
        }
        for &b in &buf[..n] {
            hash ^= b as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    hash
}

fn read_proc_self() -> (u64, u64, u64, u64) {
    let status = std::fs::read_to_string("/proc/self/status").expect("proc status");
    let mut rss = 0;
    let mut hwm = 0;
    for line in status.lines() {
        if let Some(v) = line.strip_prefix("VmRSS:") {
            rss = v
                .split_whitespace()
                .next()
                .and_then(|n| n.parse().ok())
                .unwrap_or(0);
        } else if let Some(v) = line.strip_prefix("VmHWM:") {
            hwm = v
                .split_whitespace()
                .next()
                .and_then(|n| n.parse().ok())
                .unwrap_or(0);
        }
    }
    let mut roll_rss = 0;
    let mut roll_pss = 0;
    if let Ok(roll) = std::fs::read_to_string("/proc/self/smaps_rollup") {
        for line in roll.lines() {
            if let Some(v) = line.strip_prefix("Rss:") {
                roll_rss = v
                    .split_whitespace()
                    .next()
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(0);
            } else if let Some(v) = line.strip_prefix("Pss:") {
                roll_pss = v
                    .split_whitespace()
                    .next()
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(0);
            }
        }
    }
    (rss, hwm, roll_rss, roll_pss)
}

async fn sampler_task(csv: PathBuf, start: Instant, total_secs: u64, stats: Arc<SoakStats>) {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&csv)
        .expect("open csv");
    writeln!(
        file,
        "t_secs,wall_clock,vmrss_kb,vmhwm_kb,smaps_rss_kb,smaps_pss_kb,\
         sessions_alive,scrollback_bytes,ticks_total,ssh_reconnects,\
         sftp_iters,sftp_bytes_total,sftp_reconnects"
    )
    .expect("csv header");
    file.flush().expect("flush");
    // Strict 60 s grid anchored at t=0: row k lands at t=k*60.
    let mut row: u64 = 0;
    loop {
        let t = row * SAMPLE_SECS;
        let (rss, hwm, roll_rss, roll_pss) = read_proc_self();
        writeln!(
            file,
            "{t},{},{rss},{hwm},{roll_rss},{roll_pss},{},{},{},{},{},{},{}",
            unix_secs(),
            stats.sessions_alive.load(Ordering::Relaxed),
            stats.scrollback_bytes.load(Ordering::Relaxed),
            stats.ticks_total.load(Ordering::Relaxed),
            stats.ssh_reconnects.load(Ordering::Relaxed),
            stats.sftp_iters.load(Ordering::Relaxed),
            stats.sftp_bytes_total.load(Ordering::Relaxed),
            stats.sftp_reconnects.load(Ordering::Relaxed),
        )
        .expect("csv row");
        file.flush().expect("flush");
        eprintln!(
            "[soak] t={t}s elapsed={:?} rss={rss}kB hwm={hwm}kB alive={} scrollback={}B ticks={} sftp_iters={}",
            start.elapsed(),
            stats.sessions_alive.load(Ordering::Relaxed),
            stats.scrollback_bytes.load(Ordering::Relaxed),
            stats.ticks_total.load(Ordering::Relaxed),
            stats.sftp_iters.load(Ordering::Relaxed),
        );
        if t >= total_secs {
            break;
        }
        row += 1;
        let wake_at = start + Duration::from_secs(row * SAMPLE_SECS);
        let now = Instant::now();
        if wake_at > now {
            tokio::time::sleep(wake_at - now).await;
        }
    }
    eprintln!("[soak] sampler done");
}

async fn shell_session_task(
    id: usize,
    addr: String,
    user: String,
    password: String,
    stop: Arc<AtomicBool>,
    stats: Arc<SoakStats>,
) {
    // FULL scrollback: every output byte ever received, never truncated.
    let mut scrollback: Vec<u8> = Vec::new();
    let spec = ssh_spec(&addr, &user, id);
    while !stop.load(Ordering::Relaxed) {
        let mut conn = mbxt_connections::ssh::SshConn::new(&spec);
        let auth = ConnectionAuth::Password(Zeroizing::new(password.clone()));
        match tokio::time::timeout(
            Duration::from_secs(60),
            conn.start(auth, TerminalSize::default()),
        )
        .await
        {
            Ok(Ok(())) => {},
            other => {
                stats.ssh_reconnects.fetch_add(1, Ordering::Relaxed);
                eprintln!("[soak] session {id} connect failed: {other:?}; retrying");
                tokio::time::sleep(Duration::from_secs(10)).await;
                continue;
            },
        }
        stats.sessions_alive.fetch_add(1, Ordering::Relaxed);
        eprintln!(
            "[soak] session {id} connected (scrollback={}B)",
            scrollback.len()
        );
        let mut ticker = tokio::time::interval(Duration::from_secs(TICK_SECS));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        // Drain MOTD/prompt noise into scrollback first (bounded wait).
        let drain_deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < drain_deadline {
            match tokio::time::timeout(Duration::from_secs(1), conn.next_event()).await {
                Ok(Ok(ConnectionEvent::Output(b))) => {
                    stats
                        .scrollback_bytes
                        .fetch_add(b.len() as u64, Ordering::Relaxed);
                    scrollback.extend_from_slice(&b);
                },
                _ => break,
            }
        }
        let mut alive = true;
        while alive && !stop.load(Ordering::Relaxed) {
            tokio::select! {
                _ = ticker.tick() => {
                    let cmd = format!(
                        "echo soak-tick s{id} n{} t{}\n",
                        stats.ticks_total.load(Ordering::Relaxed),
                        unix_secs()
                    );
                    if conn.write(cmd.as_bytes()).await.is_err() {
                        alive = false;
                    } else {
                        stats.ticks_total.fetch_add(1, Ordering::Relaxed);
                    }
                }
                ev = conn.next_event() => {
                    match ev {
                        Ok(ConnectionEvent::Output(b)) => {
                            stats
                                .scrollback_bytes
                                .fetch_add(b.len() as u64, Ordering::Relaxed);
                            scrollback.extend_from_slice(&b);
                        }
                        Ok(ConnectionEvent::ExitStatus(_)) | Ok(ConnectionEvent::Eof) => {
                            alive = false;
                        }
                        Err(e) => {
                            eprintln!("[soak] session {id} event error: {e}");
                            alive = false;
                        }
                    }
                }
            }
        }
        stats.sessions_alive.fetch_sub(1, Ordering::Relaxed);
        let _ = conn.shutdown().await;
        if stop.load(Ordering::Relaxed) {
            break;
        }
        stats.ssh_reconnects.fetch_add(1, Ordering::Relaxed);
        eprintln!(
            "[soak] session {id} dropped; reconnecting (scrollback={}B)",
            scrollback.len()
        );
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
    eprintln!(
        "[soak] session {id} exiting; retained scrollback={}B",
        scrollback.len()
    );
    // `scrollback` drops here; its peak residency is what the sampler saw.
}

async fn sftp_loop_task(
    addr: String,
    user: String,
    password: String,
    upload: PathBuf,
    download: PathBuf,
    expected_hash: u64,
    stop: Arc<AtomicBool>,
    stats: Arc<SoakStats>,
) {
    use remote_app::connection::sftp::{CancelToken, SftpSession};

    #[derive(Debug)]
    struct StrictHostKey {
        host: String,
        port: u16,
    }
    #[async_trait::async_trait]
    impl russh::client::Handler for StrictHostKey {
        type Error = russh::Error;
        async fn check_server_key(
            &mut self,
            key: &russh::keys::key::PublicKey,
        ) -> Result<bool, Self::Error> {
            match russh::keys::check_known_hosts(&self.host, self.port, key) {
                Ok(accepted) => Ok(accepted),
                Err(error) => {
                    tracing::warn!(%error, host = %self.host, "known_hosts check failed");
                    Ok(false)
                },
            }
        }
    }

    // Fresh connection per direction per iteration (measured lesson from
    // tests/sftp_throughput.rs: reusing one connection for big up+down
    // legs can stall the download in the russh-sftp read path; fresh
    // per-direction connections are also what the product does).
    async fn connect_sftp(
        addr: &str,
        user: &str,
        password: &str,
    ) -> Option<(russh::client::Handle<StrictHostKey>, SftpSession)> {
        let (host, port) = addr.rsplit_once(':')?;
        let config = std::sync::Arc::new(russh::client::Config {
            inactivity_timeout: Some(Duration::from_secs(60)),
            ..Default::default()
        });
        let mut handle = russh::client::connect(
            config,
            addr,
            StrictHostKey {
                host: host.to_string(),
                port: port.parse().ok()?,
            },
        )
        .await
        .ok()?;
        if !handle
            .authenticate_password(user, password)
            .await
            .unwrap_or(false)
        {
            return None;
        }
        let channel = handle.channel_open_session().await.ok()?;
        channel.request_subsystem(true, "sftp").await.ok()?;
        let raw = russh_sftp::client::SftpSession::new_with_config(
            channel.into_stream(),
            russh_sftp::client::Config {
                request_timeout_secs: 120,
                ..Default::default()
            },
        )
        .await
        .ok()?;
        Some((handle, SftpSession::from_client(raw)))
    }

    let remote = format!("upload/soak-128m-{}.bin", std::process::id());
    let mut first = true;
    while !stop.load(Ordering::Relaxed) {
        // --- upload leg (fresh connection) ---
        let (up_handle, up_sftp) = loop {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            match connect_sftp(&addr, &user, &password).await {
                Some(pair) => break pair,
                None => {
                    stats.sftp_reconnects.fetch_add(1, Ordering::Relaxed);
                    eprintln!("[soak] sftp up-connect failed; retrying");
                    tokio::time::sleep(Duration::from_secs(10)).await;
                },
            }
        };
        if first {
            eprintln!("[soak] sftp connected; starting 128 MiB loop");
            first = false;
        }
        let cancel = CancelToken::new();
        // Delete first so resume logic can never turn an iteration into
        // a no-op: every loop moves a full 128 MiB each way.
        let _ = up_sftp.delete(&remote).await;
        let iter_started = Instant::now();
        let up_ok = up_sftp
            .put_file(&upload, &remote, 4, &cancel, |_, _| {})
            .await
            .map(|n| n == FILE_BYTES)
            .unwrap_or(false);
        let _ = up_handle
            .disconnect(russh::Disconnect::ByApplication, "", "")
            .await;
        if !up_ok {
            stats.sftp_reconnects.fetch_add(1, Ordering::Relaxed);
            eprintln!("[soak] sftp upload failed; retrying");
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        }
        // --- download leg (fresh connection) ---
        let (down_handle, down_sftp) = loop {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            match connect_sftp(&addr, &user, &password).await {
                Some(pair) => break pair,
                None => {
                    stats.sftp_reconnects.fetch_add(1, Ordering::Relaxed);
                    eprintln!("[soak] sftp down-connect failed; retrying");
                    tokio::time::sleep(Duration::from_secs(10)).await;
                },
            }
        };
        let _ = std::fs::remove_file(&download);
        let down_ok = down_sftp
            .get_file(&remote, &download, 4, &cancel, |_, _| {})
            .await
            .map(|n| n == FILE_BYTES)
            .unwrap_or(false);
        if down_ok {
            let got = fnv_file(&download);
            assert_eq!(got, expected_hash, "round-trip checksum mismatch");
        }
        let _ = down_sftp.delete(&remote).await;
        let _ = down_handle
            .disconnect(russh::Disconnect::ByApplication, "", "")
            .await;
        let _ = std::fs::remove_file(&download);
        if !down_ok {
            stats.sftp_reconnects.fetch_add(1, Ordering::Relaxed);
            eprintln!("[soak] sftp download failed; retrying");
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        }
        stats.sftp_iters.fetch_add(1, Ordering::Relaxed);
        stats
            .sftp_bytes_total
            .fetch_add(2 * FILE_BYTES, Ordering::Relaxed);
        eprintln!(
            "[soak] sftp iter {} done in {:?} ({} MiB total this leg)",
            stats.sftp_iters.load(Ordering::Relaxed),
            iter_started.elapsed(),
            stats.sftp_bytes_total.load(Ordering::Relaxed) / (1 << 20),
        );
    }
    eprintln!(
        "[soak] sftp loop exiting after {} iters",
        stats.sftp_iters.load(Ordering::Relaxed)
    );
}

#[tokio::test]
#[ignore = "M5 acceptance soak: 8 h live run against disposable containers (see module docs)"]
async fn soak_acceptance_8h() {
    let total_secs: u64 = env::var("MBXT_SOAK_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(28800);
    let n_sessions: usize = env::var("MBXT_SOAK_SESSIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);
    let ssh_addr = env_or("MBXT_SOAK_SSH_ADDR", "127.0.0.1:2323");
    let ssh_user = env_or("MBXT_SOAK_SSH_USER", "test");
    let ssh_pass = env_or("MBXT_SOAK_SSH_PASSWORD", "test");
    let sftp_addr = env_or("MBXT_SOAK_SFTP_ADDR", "127.0.0.1:2324");
    let sftp_user = env_or("MBXT_SOAK_SFTP_USER", "test");
    let sftp_pass = env_or("MBXT_SOAK_SFTP_PASSWORD", "test");

    eprintln!(
        "[soak] start: sessions={n_sessions} total={total_secs}s ssh={ssh_addr} sftp={sftp_addr} pid={}",
        std::process::id()
    );
    learn_both_host_keys(&ssh_addr, &sftp_addr);

    let csv = manifest_target("soak-rss.csv");
    let scratch = std::env::temp_dir().join(format!("mbxt-soak-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("scratch dir");
    let upload = scratch.join("soak-128m.bin");
    let download = scratch.join("soak-128m.down");
    let expected_hash = prepare_upload_file(&upload);

    let stats = Arc::new(SoakStats::default());
    let stop = Arc::new(AtomicBool::new(false));
    let start = Instant::now();

    let sampler = tokio::spawn(sampler_task(
        csv.clone(),
        start,
        total_secs,
        Arc::clone(&stats),
    ));
    let sftp = tokio::spawn(sftp_loop_task(
        sftp_addr,
        sftp_user,
        sftp_pass,
        upload.clone(),
        download.clone(),
        expected_hash,
        Arc::clone(&stop),
        Arc::clone(&stats),
    ));
    let mut sessions = Vec::new();
    for id in 0..n_sessions {
        sessions.push(tokio::spawn(shell_session_task(
            id,
            ssh_addr.clone(),
            ssh_user.clone(),
            ssh_pass.clone(),
            Arc::clone(&stop),
            Arc::clone(&stats),
        )));
    }

    // The sampler owns the deadline: when it finishes, time is up.
    sampler.await.expect("sampler runs to completion");
    stop.store(true, Ordering::Relaxed);
    for h in sessions {
        let _ = tokio::time::timeout(Duration::from_secs(120), h).await;
    }
    let _ = tokio::time::timeout(Duration::from_secs(300), sftp).await;

    let (rss, hwm, _, _) = read_proc_self();
    eprintln!(
        "[soak] DONE elapsed={:?} csv={} final_rss={rss}kB final_hwm={hwm}kB \
         ticks={} scrollback={}B sftp_iters={} sftp_bytes={}B ssh_reconnects={} sftp_reconnects={}",
        start.elapsed(),
        csv.display(),
        stats.ticks_total.load(Ordering::Relaxed),
        stats.scrollback_bytes.load(Ordering::Relaxed),
        stats.sftp_iters.load(Ordering::Relaxed),
        stats.sftp_bytes_total.load(Ordering::Relaxed),
        stats.ssh_reconnects.load(Ordering::Relaxed),
        stats.sftp_reconnects.load(Ordering::Relaxed),
    );
    let _ = std::fs::remove_dir_all(&scratch);

    assert!(
        stats.ticks_total.load(Ordering::Relaxed) > 0,
        "no shell activity happened"
    );
    assert!(
        stats.sftp_iters.load(Ordering::Relaxed) > 0,
        "no sftp iteration completed"
    );
}

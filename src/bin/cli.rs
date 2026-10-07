//! Headless CLI: `remote-app-headless <COMMAND>` (prompt 1.2).
//!
//! Session-store commands operate on the same encrypted `sessions.enc` the
//! GUI uses, so exports/imports round-trip between CLI and UI. Passwords are
//! read via `rpassword` (no echo) when not supplied with `--password`, and
//! held in `SecurePassword` (zeroized after use).

use std::path::PathBuf;

use clap_complete::shells::Shell;
use remote_app::utils::config::SessionEntry;
use remote_app::utils::crypto::SecurePassword;
use remote_app::utils::paths::AppPaths;
use remote_app::utils::secure_storage::SecureStorage;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    remote_app::utils::security::initialize()?;
    let cli = mbxt_cli::build_cli(env!("CARGO_PKG_VERSION"), &remote_app::build_info::long());
    let matches = cli.get_matches();
    let paths = AppPaths::init()?;
    let mut config = remote_app::utils::config::load(&paths.config_dir).unwrap_or_default();
    if let Some(channel) = matches.get_one::<String>("channel") {
        config.general.release_channel = channel.parse().expect("clap validates channel");
    }
    if matches.get_flag("show-telemetry") {
        remote_app::feedback::telemetry::record_launch();
        println!(
            "{}",
            remote_app::feedback::telemetry::inspect_json(config.general.release_channel)
        );
        return Ok(());
    }
    if config.general.telemetry_enabled {
        remote_app::feedback::telemetry::record_launch();
    }

    // Logging: RUST_LOG wins, else --verbose maps INFO→DEBUG→TRACE.
    let _log_guard =
        remote_app::utils::logging::init(&paths.logs_dir, matches.get_count("verbose"));
    remote_app::utils::logging::install_panic_hook();
    tracing::info!(
        version = remote_app::build_info::VERSION,
        git = remote_app::build_info::GIT_HASH,
        channel = %config.general.release_channel,
        "starting remote-app-headless"
    );

    match matches.subcommand() {
        Some(("list-sessions", _)) => list_sessions(&paths)?,
        Some(("forward", args)) => forward_command(&paths, args).await?,
        Some(("connect", args)) => connect_command(&paths, args).await?,
        Some(("export-sessions", args)) => {
            let file = PathBuf::from(args.get_one::<String>("FILE").expect("required arg"));
            let password = prompt_password(args.get_one::<String>("password"))?;
            let storage = SecureStorage::new(&paths.config_dir);
            let sessions = storage.load_sessions(Some(password.as_str()))?;
            storage.export_sessions(&file, &sessions, password.as_str())?;
            println!(
                "exported {} session(s) to {}",
                sessions.len(),
                file.display()
            );
        },
        Some(("import-sessions", args)) => {
            let file = PathBuf::from(args.get_one::<String>("FILE").expect("required arg"));
            let password = prompt_password(args.get_one::<String>("password"))?;
            let storage = SecureStorage::new(&paths.config_dir);
            let imported = storage.import_sessions(&file, password.as_str())?;

            // Merge policy: keep existing by name, append new entries.
            let mut merged: Vec<SessionEntry> = if storage.has_store() {
                storage
                    .load_sessions(Some(password.as_str()))
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            let before = merged.len();
            for entry in imported {
                if !merged.iter().any(|e| e.name == entry.name) {
                    merged.push(entry);
                }
            }
            storage.save_sessions(&merged, Some(password.as_str()))?;
            println!(
                "import complete: {} new, {} total",
                merged.len() - before,
                merged.len()
            );
        },
        Some(("reset-config", _)) => {
            let config = paths.config_dir.join("config.ron");
            let store = paths
                .config_dir
                .join(remote_app::utils::secure_storage::STORE_FILE);
            for path in [&config, &store] {
                match std::fs::remove_file(path) {
                    Ok(()) => println!("removed {}", path.display()),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        println!("absent  {}", path.display())
                    },
                    Err(e) => return Err(e.into()),
                }
            }
            // Also drop the keyring cache, if any.
            let _ = remote_app::utils::keyring_bridge::delete_master_password();
            println!("config reset complete");
        },
        Some(("completions", args)) => {
            let shell: Shell = args
                .get_one::<String>("SHELL")
                .expect("required arg")
                .parse()
                .map_err(|e| anyhow::anyhow!("unknown shell: {e}"))?;
            let mut cmd =
                mbxt_cli::build_cli(env!("CARGO_PKG_VERSION"), &remote_app::build_info::long());
            clap_complete::generate(shell, &mut cmd, "remote-app", &mut std::io::stdout());
        },
        _ => {
            if matches.get_flag("master-password-prompt") {
                // --master-password-prompt: force the prompt and verify the
                // store (decrypt with the supplied password).
                let password = prompt_password(None)?;
                let storage = SecureStorage::new(&paths.config_dir);
                match storage.load_sessions(Some(password.as_str())) {
                    Ok(sessions) => {
                        println!("unlocked: {} session(s)", sessions.len());
                        for entry in &sessions {
                            println!(
                                "  #{id} {name} ({proto})",
                                id = entry.id,
                                name = entry.name,
                                proto = format!("{:?}", entry.protocol).to_lowercase()
                            );
                        }
                    },
                    Err(err) => anyhow::bail!("unlock failed: {err}"),
                }
            } else {
                mbxt_cli::build_cli(env!("CARGO_PKG_VERSION"), &remote_app::build_info::long())
                    .print_help()?;
            }
        },
    }
    Ok(())
}

/// `forward` subcommand (Prompt 5.3): open `-L`/`-R`/`-D` tunnels for a
/// stored SSH session over one multiplexed connection; runs until Ctrl-C.
#[cfg(feature = "ssh")]
async fn forward_command(paths: &AppPaths, args: &clap::ArgMatches) -> anyhow::Result<()> {
    use remote_app::connection::forward::{
        bind_with_fallback, dial_tunnel, parse_dynamic_spec, parse_forward_spec, route_matches,
        run_dynamic_forward, run_local_forward, run_remote_forward, ChildTracker, ForwardCounters,
        LogSink,
    };
    use remote_app::connection::sftp::CancelToken;
    use std::sync::Arc;

    fn bad_spec(error: String) -> anyhow::Error {
        anyhow::anyhow!("{error}")
    }

    let wanted = args.get_one::<String>("SESSION").expect("required arg");
    let locals: Vec<String> = args
        .get_many::<String>("local")
        .map(|values| values.cloned().collect())
        .unwrap_or_default();
    let remotes: Vec<String> = args
        .get_many::<String>("remote")
        .map(|values| values.cloned().collect())
        .unwrap_or_default();
    let dynamics: Vec<String> = args
        .get_many::<String>("dynamic")
        .map(|values| values.cloned().collect())
        .unwrap_or_default();
    if locals.is_empty() && remotes.is_empty() && dynamics.is_empty() {
        anyhow::bail!("nothing to forward: pass at least one of -L/-R/-D");
    }

    // Resolve the session (name or numeric id) from the encrypted store.
    let store_password = prompt_password(None)?;
    let storage = SecureStorage::new(&paths.config_dir);
    let sessions = storage.load_sessions(Some(store_password.as_str()))?;
    let entry = sessions
        .iter()
        .find(|entry| entry.name == *wanted || entry.id.to_string() == *wanted)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("unknown session {wanted:?}"))?;
    if !matches!(
        entry.protocol,
        mbxt_core::Protocol::Ssh | mbxt_core::Protocol::Sftp | mbxt_core::Protocol::X11
    ) {
        anyhow::bail!("forwarding needs an SSH-family session");
    }
    let host = entry.host.clone().unwrap_or_else(|| "localhost".into());
    let port = entry.port.unwrap_or(22);
    let username = entry
        .username
        .clone()
        .ok_or_else(|| anyhow::anyhow!("session has no username"))?;

    // SSH auth: explicit key file wins, else the prompted password.
    let auth = match args.get_one::<String>("key-file") {
        Some(path) => remote_app::connection::ConnectionAuth::KeyFile {
            path: path.into(),
            passphrase: None,
        },
        None => {
            let secret = prompt_password(args.get_one::<String>("password"))?;
            remote_app::connection::ConnectionAuth::Password(zeroize::Zeroizing::new(
                secret.as_str().to_string(),
            ))
        },
    };
    let dial = dial_tunnel(&host, port, &username, auth).await?;
    let mut handle = dial.handle;
    let cancel = CancelToken::new();
    let mut tasks = Vec::new();

    // Remote listens first (owned handle for `tcpip_forward`, which needs
    // `&mut`). One dispatcher fans the single channel receiver out to
    // per-forward loops by listen endpoint.
    if !remotes.is_empty() {
        let mut rx = dial.forwarded_rx;
        let mut routes = Vec::new();
        for spec in remotes.iter().map(|text| parse_forward_spec(text)) {
            let spec = spec.map_err(bad_spec)?;
            let bound = handle
                .tcpip_forward(&spec.bind, u32::from(spec.port))
                .await
                .map_err(|err| anyhow::anyhow!("remote listen refused: {err}"))?;
            println!(
                "-R {}:{bound} → {}:{}",
                spec.bind, spec.host, spec.host_port
            );
            let (tx, forward_rx) = tokio::sync::mpsc::unbounded_channel();
            let (local_host, local_port) = (spec.host.clone(), spec.host_port);
            let cancel = cancel.clone();
            tasks.push(tokio::spawn(run_remote_forward(
                forward_rx,
                spec.bind.clone(),
                bound,
                move || {
                    let (local_host, local_port) = (local_host.clone(), local_port);
                    async move {
                        tokio::net::TcpStream::connect((local_host.as_str(), local_port))
                            .await
                            .map_err(remote_app::connection::forward::ForwardError::io)
                    }
                },
                Arc::new(ForwardCounters::default()),
                LogSink::new(50),
                cancel,
                ChildTracker::default(),
            )));
            routes.push((spec.bind.clone(), bound, tx));
        }
        tasks.push(tokio::spawn(async move {
            while let Some(opened) = rx.recv().await {
                let target = routes.iter().position(|(bind, port, _)| {
                    route_matches(&opened.connected_host, opened.connected_port, bind, *port)
                });
                match target {
                    Some(index) => {
                        let _ = routes[index].2.send(opened);
                    },
                    None => {
                        eprintln!(
                            "dropped channel for {}:{} (no matching -R)",
                            opened.connected_host, opened.connected_port
                        );
                    },
                }
            }
            Ok::<(), remote_app::connection::forward::ForwardError>(())
        }));
    }

    let handle = Arc::new(handle);
    for spec in locals.iter().map(|text| parse_forward_spec(text)) {
        let spec = spec.map_err(bad_spec)?;
        let (listener, actual) = bind_with_fallback("127.0.0.1", spec.port).await?;
        println!("-L 127.0.0.1:{actual} → {}:{}", spec.host, spec.host_port);
        let handle = Arc::clone(&handle);
        let cancel = cancel.clone();
        tasks.push(tokio::spawn(run_local_forward(
            listener,
            move || {
                let handle = Arc::clone(&handle);
                let (host, port) = (spec.host.clone(), spec.host_port);
                async move {
                    handle
                        .channel_open_direct_tcpip(host, u32::from(port), "127.0.0.1", 0)
                        .await
                        .map(|channel| channel.into_stream())
                        .map_err(|err| {
                            remote_app::connection::forward::ForwardError::Ssh(err.to_string())
                        })
                }
            },
            Arc::new(ForwardCounters::default()),
            LogSink::new(50),
            cancel,
            ChildTracker::default(),
        )));
    }
    for spec in dynamics.iter().map(|text| parse_dynamic_spec(text)) {
        let (bind, port) = spec.map_err(bad_spec)?;
        let (listener, actual) = bind_with_fallback(&bind, port).await?;
        println!("-D {bind}:{actual} (SOCKS)");
        let handle = Arc::clone(&handle);
        let cancel = cancel.clone();
        tasks.push(tokio::spawn(run_dynamic_forward(
            listener,
            move |host: String, port: u16| {
                let handle = Arc::clone(&handle);
                async move {
                    handle
                        .channel_open_direct_tcpip(host, u32::from(port), "127.0.0.1", 0)
                        .await
                        .map(|channel| channel.into_stream())
                        .map_err(|err| {
                            remote_app::connection::forward::dynamic::SocksError::Protocol(
                                err.to_string(),
                            )
                        })
                }
            },
            Arc::new(ForwardCounters::default()),
            LogSink::new(50),
            cancel,
            ChildTracker::default(),
        )));
    }

    println!("tunnels up — Ctrl-C to stop");
    tokio::signal::ctrl_c().await?;
    cancel.cancel();
    for task in tasks {
        task.abort();
    }
    Ok(())
}

/// `forward` without the ssh feature compiled in.
#[cfg(not(feature = "ssh"))]
async fn forward_command(_paths: &AppPaths, _args: &clap::ArgMatches) -> anyhow::Result<()> {
    anyhow::bail!("tunnels need the ssh feature");
}

/// `forward` without the ssh feature compiled in.
#[cfg(not(feature = "ssh"))]
async fn forward_command(_paths: &AppPaths, _args: &clap::ArgMatches) -> anyhow::Result<()> {
    anyhow::bail!("tunnels need the ssh feature");
}

/// `connect` subcommand: attach the terminal to a stored session over its
/// transport. stdin/stdout become the terminal; the local tty is put in
/// raw mode (unix) and restored on exit, including on signals.
#[cfg(feature = "ssh")]
async fn connect_command(paths: &AppPaths, args: &clap::ArgMatches) -> anyhow::Result<()> {
    use remote_app::connection::actor::SessionManager;

    let wanted = args.get_one::<String>("SESSION").expect("required arg");
    let store_password = prompt_password(None)?;
    let storage = SecureStorage::new(&paths.config_dir);
    let sessions = storage.load_sessions(Some(store_password.as_str()))?;
    let entry = find_session(&sessions, wanted)?;
    let spec = entry.to_spec();
    // Password-style methods prompt on the terminal (no echo); key and
    // agent methods need no further input.
    let auth = match &entry.auth {
        mbxt_core::AuthMethod::Password | mbxt_core::AuthMethod::KeyboardInteractive => {
            let secret = rpassword::prompt_password("Session password: ")?;
            auth_for_entry(entry, Some(zeroize::Zeroizing::new(secret)))?
        },
        _ => auth_for_entry(entry, None)?,
    };
    let size = query_terminal_size();
    tracing::info!(session = %spec.name, "headless connect");
    eprintln!("connecting to {} ...", spec.name);
    let _raw = RawGuard::enable();
    SessionManager::shared()
        .connect(entry.id, spec, auth, size)
        .map_err(|error| anyhow::anyhow!("{error}"))?;
    let outcome = pump_session(
        SessionManager::shared(),
        entry.id,
        StdinBridge::spawn().map_err(|error| anyhow::anyhow!("stdin: {error}"))?,
        StdoutBridge::spawn(),
    )
    .await;
    let _ = SessionManager::shared().disconnect(entry.id);
    outcome
}

/// `connect` without the ssh feature compiled in.
#[cfg(not(feature = "ssh"))]
async fn connect_command(_paths: &AppPaths, _args: &clap::ArgMatches) -> anyhow::Result<()> {
    anyhow::bail!("connect needs the ssh feature");
}

/// Find a stored session by name or numeric id.
#[cfg(feature = "ssh")]
fn find_session<'a>(
    sessions: &'a [SessionEntry],
    wanted: &str,
) -> anyhow::Result<&'a SessionEntry> {
    sessions
        .iter()
        .find(|entry| entry.name == wanted || entry.id.to_string() == wanted)
        .ok_or_else(|| anyhow::anyhow!("unknown session {wanted:?}"))
}

/// Map a stored auth method to a live credential. `password` feeds the
/// password and keyboard-interactive methods; key files use their stored
/// path (no stored passphrases — encrypted keys fail with a clean
/// engine error instead of a prompt loop).
#[cfg(feature = "ssh")]
fn auth_for_entry(
    entry: &SessionEntry,
    password: Option<zeroize::Zeroizing<String>>,
) -> anyhow::Result<remote_app::connection::ConnectionAuth> {
    use remote_app::connection::ConnectionAuth;
    match &entry.auth {
        mbxt_core::AuthMethod::Password => {
            let secret = password.ok_or_else(|| anyhow::anyhow!("password required"))?;
            Ok(ConnectionAuth::Password(secret))
        },
        mbxt_core::AuthMethod::KeyFile { path } => Ok(ConnectionAuth::KeyFile {
            path: path.into(),
            passphrase: None,
        }),
        mbxt_core::AuthMethod::Agent { .. } => Ok(ConnectionAuth::Agent),
        mbxt_core::AuthMethod::KeyboardInteractive => {
            let secret = password.ok_or_else(|| anyhow::anyhow!("response required"))?;
            Ok(ConnectionAuth::KeyboardInteractive(secret))
        },
    }
}

/// Query the terminal size (unix `TIOCGWINSZ`); falls back to 80x24
/// when stdout is not a tty (piped operation still works).
#[cfg(feature = "ssh")]
fn query_terminal_size() -> remote_app::connection::TerminalSize {
    use remote_app::connection::TerminalSize;
    #[cfg(unix)]
    {
        let mut size: libc::winsize = unsafe { std::mem::zeroed() };
        // SAFETY: TIOCGWINSZ on stdout with a valid winsize buffer.
        let queried = unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut size) };
        if queried == 0 && size.ws_col > 0 && size.ws_row > 0 {
            return TerminalSize {
                cols: size.ws_col,
                rows: size.ws_row,
                pixel_width: 0,
                pixel_height: 0,
            };
        }
    }
    TerminalSize::default()
}

/// Unix raw-mode guard: puts stdin in raw mode for transparent key
/// forwarding (Ctrl-C reaches the remote side) and restores the saved
/// mode on drop — including unwinds and the signal path below, which
/// returns through here. Non-tty stdin skips raw mode with a warning;
/// non-unix builds refuse (no terminal to attach). Nonblocking stdin
/// is owned separately by [`StdinBridge`].
#[cfg(all(feature = "ssh", unix))]
struct RawGuard {
    active: bool,
    original: Option<nix::sys::termios::Termios>,
}

#[cfg(all(feature = "ssh", unix))]
impl RawGuard {
    fn enable() -> Self {
        let stdin = std::io::stdin();
        match nix::sys::termios::tcgetattr(&stdin) {
            Ok(original) => {
                let mut raw = original.clone();
                nix::sys::termios::cfmakeraw(&mut raw);
                if nix::sys::termios::tcsetattr(&stdin, nix::sys::termios::SetArg::TCSANOW, &raw)
                    .is_err()
                {
                    eprintln!("warning: cannot set raw mode; line editing stays local");
                    return Self {
                        active: false,
                        original: None,
                    };
                }
                Self {
                    active: true,
                    original: Some(original),
                }
            },
            Err(_) => {
                eprintln!("warning: stdin is not a tty; running without raw mode");
                Self {
                    active: false,
                    original: None,
                }
            },
        }
    }
}

#[cfg(all(feature = "ssh", unix))]
impl Drop for RawGuard {
    fn drop(&mut self) {
        if self.active {
            if let Some(original) = self.original.take() {
                let stdin = std::io::stdin();
                let _ = nix::sys::termios::tcsetattr(
                    &stdin,
                    nix::sys::termios::SetArg::TCSANOW,
                    &original,
                );
            }
        }
    }
}

/// Async stdin bridge over `tokio::io::unix::AsyncFd` (no dedicated
/// thread, so nothing can hang runtime shutdown on exit). Owns stdin's
/// `O_NONBLOCK` flag for its lifetime and restores it on drop, so piped
/// input works as well as ttys.
#[cfg(feature = "ssh")]
struct StdinBridge {
    fd: tokio::io::unix::AsyncFd<std::io::Stdin>,
    saved_flags: Option<std::os::raw::c_int>,
}

#[cfg(feature = "ssh")]
impl StdinBridge {
    fn spawn() -> std::io::Result<Self> {
        // SAFETY: fcntl get/set on our own stdin fd with no pointer args.
        let previous = unsafe { libc::fcntl(libc::STDIN_FILENO, libc::F_GETFL) };
        if previous < 0 {
            return Err(std::io::Error::last_os_error());
        }
        let current = unsafe {
            libc::fcntl(
                libc::STDIN_FILENO,
                libc::F_SETFL,
                previous | libc::O_NONBLOCK,
            )
        };
        if current < 0 {
            return Err(std::io::Error::last_os_error());
        }
        let stdin = std::io::stdin();
        tokio::io::unix::AsyncFd::new(stdin).map(|fd| Self {
            fd,
            saved_flags: Some(previous),
        })
    }
}

#[cfg(feature = "ssh")]
impl Drop for StdinBridge {
    fn drop(&mut self) {
        if let Some(flags) = self.saved_flags.take() {
            // SAFETY: restoring flags previously read from this fd.
            unsafe {
                libc::fcntl(libc::STDIN_FILENO, libc::F_SETFL, flags);
            }
        }
    }
}

#[cfg(feature = "ssh")]
impl tokio::io::AsyncRead for StdinBridge {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        out: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        use std::io::Read as _;
        use std::task::Poll;
        let this = self.get_mut();
        loop {
            let mut guard = futures::ready!(this.fd.poll_read_ready(cx))
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            let mut chunk = [0u8; 4096];
            let want = out.remaining().min(chunk.len());
            if want == 0 {
                guard.clear_ready();
                return Poll::Ready(Ok(()));
            }
            match this.fd.get_ref().read(&mut chunk[..want]) {
                Ok(0) => {
                    guard.clear_ready();
                    return Poll::Ready(Ok(()));
                },
                Ok(count) => {
                    out.put_slice(&chunk[..count]);
                    guard.clear_ready();
                    return Poll::Ready(Ok(()));
                },
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    guard.clear_ready();
                },
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {},
                Err(error) => return Poll::Ready(Err(error)),
            }
        }
    }
}

/// Async stdout sink: bytes cross an unbounded channel to a blocking
/// writer task (unbounded, so `poll_write` never needs readiness
/// machinery; the writer drains continuously and the task ends when
/// the bridge drops).
#[cfg(feature = "ssh")]
struct StdoutBridge {
    tx: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
}

#[cfg(feature = "ssh")]
impl StdoutBridge {
    fn spawn() -> Self {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
        tokio::task::spawn_blocking(move || {
            use std::io::Write as _;
            // Lock stdout per chunk, never across receives: the tracing
            // console layer locks the same handle per event, so holding
            // it across a blocking recv deadlocks the runtime (observed
            // as a hang with zero CPU after session start).
            while let Some(chunk) = rx.blocking_recv() {
                let mut stdout = std::io::stdout().lock();
                if stdout.write_all(&chunk).is_err() || stdout.flush().is_err() {
                    break;
                }
            }
        });
        Self { tx }
    }
}

#[cfg(feature = "ssh")]
impl tokio::io::AsyncWrite for StdoutBridge {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        use std::task::Poll;
        match self.tx.send(buf.to_vec()) {
            Ok(()) => Poll::Ready(Ok(buf.len())),
            Err(_) => Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "terminal output lost its writer",
            ))),
        }
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        // The writer task flushes per chunk; nothing to wait on.
        std::task::Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
}

/// Pump bytes between stdio and one session actor until either side
/// ends: stdin EOF or a termination signal disconnects; remote close
/// or failure ends the loop. Status goes to stderr so stdout stays a
/// clean terminal stream. Generic over IO for hermetic tests.
#[cfg(feature = "ssh")]
async fn pump_session<R, W>(
    manager: &remote_app::connection::actor::SessionManager,
    id: mbxt_core::SessionId,
    stdin: R,
    mut stdout: W,
) -> anyhow::Result<()>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    use tokio::io::AsyncReadExt;

    let mut events = manager.subscribe();
    let mut input = tokio::io::BufReader::new(stdin);
    let mut buf = [0u8; 4096];
    #[cfg(unix)]
    let mut signals = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    loop {
        #[cfg(unix)]
        {
            tokio::select! {
                read = input.read(&mut buf) => {
                    let count = read?;
                    if count == 0 {
                        break;
                    }
                    manager
                        .write(id, buf[..count].to_vec())
                        .map_err(|error| anyhow::anyhow!("{error}"))?;
                },
                event = events.recv() => {
                    if pump_event(&mut stdout, id, event).await? {
                        break;
                    }
                },
                _ = signals.recv() => {
                    eprintln!("\r\nterminated");
                    break;
                },
            }
        }
        #[cfg(not(unix))]
        {
            tokio::select! {
                read = input.read(&mut buf) => {
                    let count = read?;
                    if count == 0 {
                        break;
                    }
                    manager
                        .write(id, buf[..count].to_vec())
                        .map_err(|error| anyhow::anyhow!("{error}"))?;
                },
                event = events.recv() => {
                    if pump_event(&mut stdout, id, event).await? {
                        break;
                    }
                },
            }
        }
    }
    Ok(())
}

/// Route one actor event to the terminal. Returns true when the
/// session is over.
#[cfg(feature = "ssh")]
async fn pump_event<W>(
    stdout: &mut W,
    id: mbxt_core::SessionId,
    event: Result<mbxt_core::UiEvent, tokio::sync::broadcast::error::RecvError>,
) -> anyhow::Result<bool>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    use mbxt_core::{SessionState, UiEvent};
    use tokio::io::AsyncWriteExt;
    match event {
        Ok(UiEvent::TerminalOutput { session, bytes }) if session == id => {
            stdout.write_all(&bytes).await?;
            stdout.flush().await?;
        },
        Ok(UiEvent::SessionStateChanged { session, state }) if session == id => match state {
            SessionState::Connected => eprintln!("\r\nconnected\r"),
            SessionState::Failed(reason) => {
                eprintln!("\r\nconnection failed: {reason}\r");
                return Ok(true);
            },
            SessionState::Disconnected => {
                eprintln!("\r\ndisconnected\r");
                return Ok(true);
            },
            SessionState::Connecting => {},
        },
        Ok(UiEvent::Error(message)) => {
            eprintln!("\r\nerror: {message}\r");
        },
        Err(tokio::sync::broadcast::error::RecvError::Closed) => return Ok(true),
        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {},
        _ => {},
    }
    Ok(false)
}

fn list_sessions(paths: &AppPaths) -> anyhow::Result<()> {
    let storage = SecureStorage::new(&paths.config_dir);
    if storage.has_store() {
        anyhow::bail!(
            "sessions are encrypted; use `--master-password-prompt` to unlock, \
             or import/export commands"
        );
    }
    // Legacy/plain index (pre-encryption stores and diagnostics).
    let store = mbxt_storage::SessionStore::open(&paths.data_dir.join("sessions.db"))?;
    for name in store.list()? {
        println!("{name}");
    }
    Ok(())
}

/// Resolve the password: explicit `--password` value or interactive prompt
/// (no echo). Held in `SecurePassword` so the copy is zeroized on drop.
fn prompt_password(explicit: Option<&String>) -> anyhow::Result<SecurePassword> {
    let value = match explicit {
        Some(p) => p.clone(),
        None => rpassword::prompt_password("Master password: ")?,
    };
    Ok(SecurePassword::new(&value))
}

#[cfg(all(test, feature = "ssh"))]
mod tests {
    use super::*;

    fn make_entry(name: &str, id: u64, auth: mbxt_core::AuthMethod) -> SessionEntry {
        SessionEntry {
            id,
            name: name.to_string(),
            protocol: mbxt_core::Protocol::Ssh,
            host: Some("h".into()),
            port: Some(22),
            username: Some("ops".into()),
            auth,
            tags: Vec::new(),
            notes: String::new(),
            x11_forwarding: false,
            serial: None,
            forwards: Vec::new(),
            created_secs: 0,
        }
    }

    #[test]
    fn find_session_matches_name_id_and_rejects_unknown() {
        let entries = vec![
            make_entry("web", 1, mbxt_core::AuthMethod::Password),
            make_entry("db", 2, mbxt_core::AuthMethod::Password),
        ];
        assert_eq!(find_session(&entries, "db").unwrap().id, 2);
        assert_eq!(find_session(&entries, "1").unwrap().name, "web");
        assert!(find_session(&entries, "nope").is_err());
    }

    #[test]
    fn auth_for_entry_maps_every_method() {
        use remote_app::connection::ConnectionAuth;
        let password = || Some(zeroize::Zeroizing::new("secret".to_string()));
        let entry = make_entry("w", 1, mbxt_core::AuthMethod::Password);
        assert!(matches!(
            auth_for_entry(&entry, password()).unwrap(),
            ConnectionAuth::Password(_)
        ));
        assert!(auth_for_entry(&entry, None).is_err());
        let entry = make_entry(
            "w",
            1,
            mbxt_core::AuthMethod::KeyFile {
                path: "/home/ops/.ssh/id_ed25519".into(),
            },
        );
        assert!(matches!(
            auth_for_entry(&entry, None).unwrap(),
            ConnectionAuth::KeyFile { .. }
        ));
        let entry = make_entry("w", 1, mbxt_core::AuthMethod::Agent { forward: false });
        assert!(matches!(
            auth_for_entry(&entry, None).unwrap(),
            ConnectionAuth::Agent
        ));
        let entry = make_entry("w", 1, mbxt_core::AuthMethod::KeyboardInteractive);
        assert!(matches!(
            auth_for_entry(&entry, password()).unwrap(),
            ConnectionAuth::KeyboardInteractive(_)
        ));
    }

    #[test]
    fn terminal_size_is_sane_without_tty() {
        let size = query_terminal_size();
        assert!(size.cols > 0 && size.rows > 0);
    }

    #[cfg(unix)]
    #[test]
    fn raw_guard_survives_non_tty_stdin() {
        // Cargo test stdin is not a tty: must construct (inactive) and
        // drop without panicking, never touching terminal state.
        let _guard = RawGuard::enable();
    }

    #[tokio::test]
    async fn pump_event_routes_output_and_end() {
        use mbxt_core::{SessionState, UiEvent};
        let mut out = Vec::new();
        let id = 424242u64;
        let done = pump_event(
            &mut out,
            id,
            Ok(UiEvent::TerminalOutput {
                session: id,
                bytes: b"hi".to_vec(),
            }),
        )
        .await
        .unwrap();
        assert!(!done);
        assert_eq!(out, b"hi");
        // Another session's output is ignored.
        let done = pump_event(
            &mut out,
            id,
            Ok(UiEvent::TerminalOutput {
                session: id + 1,
                bytes: b"xx".to_vec(),
            }),
        )
        .await
        .unwrap();
        assert!(!done);
        assert_eq!(out, b"hi");
        // Remote close ends the pump.
        let done = pump_event(
            &mut out,
            id,
            Ok(UiEvent::SessionStateChanged {
                session: id,
                state: SessionState::Disconnected,
            }),
        )
        .await
        .unwrap();
        assert!(done);
    }

    #[tokio::test]
    async fn pump_session_stops_at_stdin_eof() {
        use remote_app::connection::actor::SessionManager;
        let out = Vec::new();
        let input: &[u8] = b"";
        pump_session(SessionManager::shared(), u64::MAX - 7, input, out)
            .await
            .expect("clean EOF exit without a server");
    }

    /// Full headless path: resolve → auth → actor connect → pump echo →
    /// clean shutdown, against the hermetic echo server. Exercises
    /// everything `connect_command` does except terminal raw mode and
    /// password prompting.
    #[tokio::test]
    async fn full_connect_flow_against_echo_server() {
        use mbxt_core::{AuthMethod, Protocol, SessionSpec};
        use remote_app::connection::actor::SessionManager;
        use remote_app::connection::{ConnectionAuth, TerminalSize};
        use remote_app::test_utils as tu;
        use tokio::io::AsyncWriteExt;

        let _home = tu::ThrowawayHome::set().expect("temp home");
        let server = tu::EchoSshServer::start().await;
        tu::learn_loopback_key(server.port, &server.public);

        let id = ((std::process::id() as u64) << 32) | 0xC11EC7;
        let spec = SessionSpec {
            name: "cli-e2e".into(),
            protocol: Protocol::Ssh,
            host: Some("127.0.0.1".into()),
            port: Some(server.port),
            username: Some("test".into()),
            auth: AuthMethod::Password,
            tags: Vec::new(),
            notes: String::new(),
            x11_forwarding: false,
            serial: None,
            forwards: Vec::new(),
        };
        let manager = SessionManager::shared();
        // Wait for the actor's Connected state (connect is async).
        let mut watch = manager.subscribe();
        manager
            .connect(
                id,
                spec,
                ConnectionAuth::Password(zeroize::Zeroizing::new("test".into())),
                TerminalSize::default(),
            )
            .expect("actor connect");
        tokio::time::timeout(std::time::Duration::from_secs(15), async {
            loop {
                match watch.recv().await {
                    Ok(mbxt_core::UiEvent::SessionStateChanged { session, state })
                        if session == id =>
                    {
                        if state == mbxt_core::SessionState::Connected {
                            break;
                        }
                        if matches!(state, mbxt_core::SessionState::Failed(_)) {
                            panic!("actor failed to connect");
                        }
                    },
                    Ok(_) => {},
                    Err(_) => panic!("event bus closed during connect"),
                }
            }
        })
        .await
        .expect("connect in time");

        let (mut input_write, input_read) = tokio::io::duplex(65536);
        input_write.write_all(b"hello-cli").await.expect("seed");
        let output = SharedOut::default();
        let pump = tokio::spawn(pump_session(manager, id, input_read, output.clone()));
        // Wait for the echo through the actor (the pump stays up: the
        // write end is still open, so no EOF yet).
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                {
                    let guard = output.0.lock().unwrap();
                    if guard.windows(9).any(|w| w == b"hello-cli") {
                        return;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("echo in time");
        drop(input_write);
        tokio::time::timeout(std::time::Duration::from_secs(10), pump)
            .await
            .expect("pump in time")
            .expect("pump")
            .expect("pump ok");
        manager.disconnect(id).ok();
        server.task.abort();
    }

    /// Shared output sink for the flow test ( std mutex: held only for
    /// a `Vec::extend`, never across awaits).
    #[derive(Clone, Default)]
    struct SharedOut(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

    impl tokio::io::AsyncWrite for SharedOut {
        fn poll_write(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
            buf: &[u8],
        ) -> std::task::Poll<std::io::Result<usize>> {
            self.0.lock().unwrap().extend_from_slice(buf);
            std::task::Poll::Ready(Ok(buf.len()))
        }

        fn poll_flush(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::task::Poll::Ready(Ok(()))
        }

        fn poll_shutdown(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::task::Poll::Ready(Ok(()))
        }
    }
}

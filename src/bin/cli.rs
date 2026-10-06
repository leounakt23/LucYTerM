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
        Some(("connect", args)) => {
            let session = args.get_one::<String>("SESSION").expect("required arg");
            tracing::info!(session, "connect requested");
            // TODO(prompt 1.3+): route through ActionRouter → ConnectionFactory.
            anyhow::bail!("connect: not implemented yet (skeleton)");
        },
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

//! GUI entry point.
//!
//! Boot order (prompt 1.3): directories → logging (+ panic hook) →
//! single-instance guard → config load → UI. The logging worker guard is
//! held for the process lifetime so the rotating file layer keeps flushing.
//!
//! Concurrency model (`doc/architecture.md` §4): the Iced event loop owns the
//! main thread (wgpu render + input). With the `tokio` feature enabled, Iced
//! provisions the Tokio multi-thread runtime internally and runs `Task`s and
//! `Subscription`s on it — so we deliberately do *not* build our own runtime
//! here (a second runtime would panic when nested). The headless binary owns
//! its runtime explicitly via `#[tokio::main]`.

fn main() -> iced::Result {
    let startup_started = std::time::Instant::now();
    // Prefer the Vulkan renderer on Linux unless the user overrides it:
    // instantiating the GL backend loads the full GL/GLX vendor stack
    // (~30 MB RSS on hybrid-GPU systems) even when Vulkan renders, and
    // the GL surface path itself crashes on some X11 stacks
    // ("incompatible window kind").
    #[cfg(target_os = "linux")]
    if std::env::var_os("WGPU_BACKEND").is_none() {
        std::env::set_var("WGPU_BACKEND", "vulkan");
    }
    let matches = mbxt_cli::build_cli(env!("CARGO_PKG_VERSION"), &remote_app::build_info::long())
        .get_matches();
    if let Err(err) = remote_app::utils::security::initialize() {
        eprintln!("remote-app: process hardening failed: {err}");
        std::process::exit(1);
    }
    // 1. Directories first: logging needs the log directory.
    let paths = match remote_app::utils::paths::AppPaths::init() {
        Ok(paths) => paths,
        Err(err) => {
            eprintln!("remote-app: cannot initialize directories: {err}");
            std::process::exit(1);
        },
    };

    // 2. Logging: colored stdout + rotating file; guard held for the process.
    let _log_guard = remote_app::utils::logging::init(&paths.logs_dir, 0);
    // 3. Panic handling: human-panic's pretty reporter (release builds) with
    //    our logging hook chained in front of it.
    human_panic::setup_panic!();
    remote_app::utils::logging::install_panic_hook();

    tracing::info!(
        version = remote_app::build_info::VERSION,
        git = remote_app::build_info::GIT_HASH,
        logs = %paths.logs_dir.display(),
        "starting remote-app"
    );

    // 4. Single-instance guard: fails fast if another instance holds the lock.
    let _instance_guard = match remote_app::utils::single_instance::acquire_default() {
        Ok(guard) => guard,
        Err(err) => {
            tracing::error!(%err, "another instance is running");
            eprintln!("remote-app: {err}");
            std::process::exit(1);
        },
    };

    // 5. Configuration (graceful: malformed files fall back to defaults).
    let mut config = match remote_app::utils::config::load(&paths.config_dir) {
        Ok(config) => config,
        Err(err) => {
            tracing::error!(error = %err, "failed to load config; using defaults");
            remote_app::utils::config::AppConfig::default()
        },
    };
    if let Some(channel) = matches.get_one::<String>("channel") {
        config.general.release_channel = channel.parse().expect("clap validates channel");
    }
    if matches.get_flag("show-telemetry") {
        remote_app::feedback::telemetry::record_launch();
        remote_app::feedback::telemetry::record_performance(
            "startup",
            startup_started
                .elapsed()
                .as_millis()
                .min(u128::from(u64::MAX)) as u64,
        );
        println!(
            "{}",
            remote_app::feedback::telemetry::inspect_json(config.general.release_channel)
        );
        return Ok(());
    }
    if config.general.telemetry_enabled {
        remote_app::feedback::telemetry::record_launch();
        remote_app::feedback::telemetry::record_performance(
            "startup",
            startup_started
                .elapsed()
                .as_millis()
                .min(u128::from(u64::MAX)) as u64,
        );
    }
    tracing::info!(channel = %config.general.release_channel, "configuration loaded; starting UI");
    remote_app::app::run(config, paths)
}

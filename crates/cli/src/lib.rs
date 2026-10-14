//! Shared CLI definition.
//!
//! Used in three places so they can never drift apart:
//! 1. `remote-app-headless` runtime argument parsing
//! 2. `remote-app` `build.rs` shell-completion generation (`clap_complete`)
//! 3. `remote-app-headless completions <shell>` runtime generation

use clap::{Arg, ArgAction, Command};

/// Build the CLI command tree. `version`/`build_info` are injected by the
/// caller (main crate embeds git metadata via `build.rs`).
pub fn build_cli(version: &str, build_info: &str) -> Command {
    Command::new("remote-app")
        .version(version.to_owned())
        .about("Linux-native remote computing application (MobaXterm parity)")
        .after_help(format!("build: {build_info}"))
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .global(true)
                .action(ArgAction::Count)
                .help("Increase log verbosity (repeatable)"),
        )
        .arg(
            Arg::new("master-password-prompt")
                .long("master-password-prompt")
                .action(ArgAction::SetTrue)
                .help("Force the master password prompt (verify/unlock the session store)"),
        )
        .arg(
            Arg::new("channel")
                .long("channel")
                .global(true)
                .value_parser(["stable", "beta", "nightly"])
                .help("Use the stable, beta, or nightly update channel"),
        )
        .arg(
            Arg::new("show-telemetry")
                .long("show-telemetry")
                .global(true)
                .action(ArgAction::SetTrue)
                .help("Print the exact anonymous telemetry payload without sending it"),
        )
        .subcommands([
            Command::new("connect")
                .about("Connect to a stored session and attach the terminal")
                .arg(
                    Arg::new("SESSION")
                        .required(true)
                        .help("Session name or id"),
                ),
            Command::new("list-sessions").about("List stored sessions"),
            Command::new("export-sessions")
                .about("Export sessions to an encrypted file")
                .arg(
                    Arg::new("FILE")
                        .required(true)
                        .help("Destination .enc path"),
                )
                .arg(
                    Arg::new("password")
                        .long("password")
                        .action(ArgAction::Set)
                        .help("Encryption password (INSECURE in ps/shell history; omit to be prompted, or use $MBXT_MASTER_PASSWORD)"),
                ),
            Command::new("import-sessions")
                .about("Import sessions from an encrypted file")
                .arg(Arg::new("FILE").required(true).help("Source .enc path"))
                .arg(
                    Arg::new("password")
                        .long("password")
                        .action(ArgAction::Set)
                        .help("Decryption password (INSECURE in ps/shell history; omit to be prompted, or use $MBXT_MASTER_PASSWORD)"),
                ),
            Command::new("import")
                .about("Preview or import sessions from a third-party file (one-way; never overwrites)")
                .arg(
                    Arg::new("from")
                        .long("from")
                        .required(true)
                        .value_parser(["mobaxterm", "putty", "ssh-config"])
                        .help("Source format"),
                )
                .arg(
                    Arg::new("file")
                        .long("file")
                        .required(true)
                        .help("Source file path (.xml, .reg, or ssh_config)"),
                )
                .arg(
                    Arg::new("dry-run")
                        .long("dry-run")
                        .action(ArgAction::SetTrue)
                        .help("Preview only: print the report without storing anything (default)"),
                )
                .arg(
                    Arg::new("commit")
                        .long("commit")
                        .action(ArgAction::SetTrue)
                        .help("Materialize the preview into the encrypted store (requires confirmation report)"),
                ),
            Command::new("forward")
                .about("Open SSH tunnels for a stored session (stays up until Ctrl-C)")
                .arg(
                    Arg::new("SESSION")
                        .required(true)
                        .help("Session name or id"),
                )
                .arg(
                    Arg::new("local")
                        .short('L')
                        .action(ArgAction::Append)
                        .help("Local forward [bind:]port:host:port (repeatable)"),
                )
                .arg(
                    Arg::new("remote")
                        .short('R')
                        .action(ArgAction::Append)
                        .help("Remote forward [bind:]port:host:port (repeatable)"),
                )
                .arg(
                    Arg::new("dynamic")
                        .short('D')
                        .action(ArgAction::Append)
                        .help("Dynamic SOCKS forward [bind:]port (repeatable)"),
                )
                .arg(
                    Arg::new("password")
                        .long("password")
                        .action(ArgAction::Set)
                        .help("SSH password (INSECURE in ps/shell history; omit to be prompted, or use $MBXT_SSH_PASSWORD)"),
                )
                .arg(
                    Arg::new("key-file")
                        .long("key-file")
                        .action(ArgAction::Set)
                        .help("SSH private key path (instead of a password)"),
                ),
            Command::new("reset-config").about("Delete config.ron and the encrypted session store"),
            Command::new("completions")
                .about("Generate shell completions for remote-app")
                .arg(Arg::new("SHELL").required(true).value_parser([
                    "bash",
                    "zsh",
                    "fish",
                    "powershell",
                    "elvish",
                ])),
        ])
}

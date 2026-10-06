//! Build script:
//! 1. Embeds git commit hash + build timestamp (`MBXT_GIT_HASH`, `MBXT_BUILD_TIME`).
//! 2. Validates embedded resources (`.desktop`, icon) included via `include_str!`.
//! 3. Generates shell completions with `clap_complete` into `OUT_DIR`
//!    (`MBXT_COMPLETIONS_DIR`), using the same CLI definition as the binaries.

use clap_complete::shells::{Bash, Fish, PowerShell, Zsh};
use clap_complete::Generator;
use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=assets");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=crates/terminal/shaders/terminal.wgsl");

    // --- 0. Terminal shaders (Prompt 2.4) -----------------------------------
    // Raw WGSL is embedded via `include_str!`; validate entry points early.
    let shader = Path::new("crates/terminal/shaders/terminal.wgsl");
    assert!(
        shader.exists(),
        "missing terminal shader: crates/terminal/shaders/terminal.wgsl"
    );
    let source = std::fs::read_to_string(shader).expect("read terminal.wgsl");
    for needle in ["vs_main", "fs_main", "atlas_texture"] {
        assert!(
            source.contains(needle),
            "terminal.wgsl must contain `{needle}`"
        );
    }

    // --- 1. Build metadata ------------------------------------------------
    let git_hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=MBXT_GIT_HASH={git_hash}");

    // SOURCE_DATE_EPOCH makes release artifacts reproducible while retaining
    // a useful timestamp for developer builds.
    let build_time = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|d| d.as_secs())
        })
        .unwrap_or(0);
    println!("cargo:rustc-env=MBXT_BUILD_TIME=unix:{build_time}");

    // --- 2. Resources -----------------------------------------------------
    for resource in ["assets/remote-app.desktop", "assets/icon.svg"] {
        assert!(
            Path::new(resource).exists(),
            "missing required resource: {resource}"
        );
    }

    // --- 3. Shell completions ---------------------------------------------
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let mut cmd = mbxt_cli::build_cli(&version, &git_hash);

    let out_dir = std::env::var("OUT_DIR").expect("cargo provides OUT_DIR");
    let completions_dir = Path::new(&out_dir).join("completions");
    std::fs::create_dir_all(&completions_dir).expect("create completions dir");

    generate_completion(Bash, "remote-app.bash", &completions_dir, &mut cmd);
    generate_completion(Zsh, "_remote-app", &completions_dir, &mut cmd);
    generate_completion(Fish, "remote-app.fish", &completions_dir, &mut cmd);
    generate_completion(PowerShell, "_remote-app.ps1", &completions_dir, &mut cmd);
    println!(
        "cargo:rustc-env=MBXT_COMPLETIONS_DIR={}",
        completions_dir.display()
    );
}

fn generate_completion<G: Generator>(
    shell: G,
    name: &str,
    directory: &Path,
    command: &mut clap::Command,
) {
    let mut file = std::fs::File::create(directory.join(name))
        .unwrap_or_else(|e| panic!("create {name}: {e}"));
    clap_complete::generate(shell, command, "remote-app", &mut file);
}

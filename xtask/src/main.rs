use std::env;
use std::fs;
use std::path::Path;
use std::process::{Command, ExitCode};

fn run(program: &str, args: &[&str]) -> Result<(), String> {
    let status = Command::new(program)
        .args(args)
        .status()
        .map_err(|err| format!("could not run {program}: {err}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} exited with {status}"))
    }
}

fn docs() -> Result<(), String> {
    run("mdbook", &["build", "docs"])
}

fn i18n() -> Result<(), String> {
    let path = Path::new("locales/en-US/app.ftl");
    let text = fs::read_to_string(path).map_err(|err| format!("read {}: {err}", path.display()))?;
    let mut count = 0usize;
    for (line_no, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, _)) = line.split_once('=') else {
            return Err(format!(
                "{}:{}: expected key = value",
                path.display(),
                line_no + 1
            ));
        };
        if key.trim().is_empty() {
            return Err(format!("{}:{}: empty key", path.display(), line_no + 1));
        }
        count += 1;
    }
    println!(
        "validated {count} English Fluent strings in {}",
        path.display()
    );
    Ok(())
}

fn release() -> Result<(), String> {
    run("bash", &["packaging/scripts/build-all.sh"])
}

fn main() -> ExitCode {
    let result = match env::args().nth(1).as_deref() {
        Some("docs") => docs(),
        Some("i18n") => i18n(),
        Some("release") => release(),
        _ => {
            eprintln!("usage: cargo xtask [docs|i18n|release]");
            Err("unknown task".to_string())
        },
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("xtask: {error}");
            ExitCode::FAILURE
        },
    }
}

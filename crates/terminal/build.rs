//! Build script (Prompt 2.4): validate the raw WGSL shader.
//!
//! The shader is embedded at runtime via `include_str!`; this script fails
//! the build early when the entry points drift (e.g. a rename in the WGSL
//! without updating `renderer.rs`).

use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=shaders/terminal.wgsl");

    let path = Path::new("shaders/terminal.wgsl");
    let source = std::fs::read_to_string(path).expect("missing shaders/terminal.wgsl");

    for needle in ["vs_main", "fs_main", "CellInstance", "atlas_texture"] {
        assert!(
            source.contains(needle),
            "shaders/terminal.wgsl must contain `{needle}`"
        );
    }
}

# Reproducible Builds

The repository pins the Rust toolchain in `rust-toolchain.toml` and commits
`Cargo.lock`. Release jobs set `SOURCE_DATE_EPOCH` to the source commit time.
`build.rs` uses that value instead of wall-clock time, and tarballs normalize
ordering, ownership, and timestamps.

## Local Build

```text
rustup show
export SOURCE_DATE_EPOCH=$(git log -1 --format=%ct)
cargo build --locked --release
```

For all supported targets, use `packaging/scripts/build-all.sh`. It uses
`cross` when available and otherwise uses installed Rust targets and the host's
linkers. The aarch64 and musl builds are best run in the provided cross
containers or CI.

## Verification

Release artifacts are accompanied by `SHA256SUMS`. Compare two builds with:

```text
diffoscope first/remote-app.tar.gz second/remote-app.tar.gz
sha256sum -c SHA256SUMS
```

Native GUI builds can vary when the graphics toolchain embeds platform data;
those differences must be investigated rather than normalized blindly.

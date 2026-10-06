# Performance

## Targets

The release targets are cold start under 2 s, idle memory under 100 MiB, 60
FPS terminal rendering, and more than 100 MB/s for local-network transfers.
These are acceptance targets, not assumptions about every host. Measurements
must record OS, CPU/GPU, build profile, feature set, and workload.

## Measurement

Run the deterministic microbenchmarks with:

```text
cargo bench --bench performance
```

Criterion stores baselines under `target/criterion`. Compare a change against
the saved baseline and investigate any regression over 10 percent. The CI
benchmark job compiles and executes the suite on every push and pull request.

For CPU flame graphs, install `cargo-flamegraph`, run a release binary with a
representative workload, and inspect the generated SVG:

```text
cargo flamegraph --release --bin remote-app --features profiling
```

For allocation work, use `dhat` or `heaptrack` on the release binary. The
application exposes operation timing spans through `utils::profiling`; setting
`RUST_LOG=remote_app=debug` records elapsed microseconds for profiled SFTP
operations. The optional `profiling` feature enables puffin integration for
interactive capture and pulls in the tracing-flame dependency for flame-layer
experiments without affecting default startup.

## Implemented Optimizations

- Terminal region scrolling rotates rows in place instead of cloning the full
  region on every scroll.
- Transfer copies reuse one bounded buffer and emit a profiling span without
  buffering the file.
- Release builds use fat LTO, one codegen unit, symbol stripping, and size
  optimization. Protocol features remain independently selectable.
- The benchmark suite covers terminal scrolling/search and utility formatting.

## Results Log

| Workload | Baseline | Current | Host/profile |
| --- | --- | --- | --- |
| Terminal scroll | pending | measured by Criterion | record in PR |
| Terminal search | pending | measured by Criterion | record in PR |
| SFTP copy throughput | pending | measure on local network | record in PR |
| Cold start / idle memory | pending | measure packaged release | record in PR |

No numeric result is claimed until it is captured on a named host. PRs
changing hot paths must include the before/after Criterion output and the
runtime workload measurements relevant to the change.

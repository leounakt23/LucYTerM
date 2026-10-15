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

Measured 2026-10-14 on KT23-OMEN-EDITION (Ubuntu 24.04.5, AMD Ryzen AI 7
350 16-thread, 30 GiB RAM, AMD Radeon 860M Mesa 25.2.8 + NVIDIA dGPU
present). Profiles are recorded per row: `bench` = bench profile
(release-derived: fat LTO, opt-level z); `test` = debug test profile.

| Workload | Result | Host/profile |
| --- | --- | --- |
| Criterion `terminal/scroll_region_up/24` | 40.135 µs/iter (100 × scroll) | KT23 / bench |
| Criterion `terminal/scroll_region_up/100` | 57.265 µs/iter | KT23 / bench |
| Criterion `terminal/scroll_region_up/500` | 123.15 µs/iter | KT23 / bench |
| Criterion `terminal_search_160x500` | 226.23 µs/iter | KT23 / bench |
| Criterion `terminal_vte_output` | 8.0423 µs/iter | KT23 / bench |
| Criterion `config_ron_serialization` | 1.6090 µs/iter | KT23 / bench |
| Criterion `subnet_calculate` | 42.736 ns/iter | KT23 / bench |
| Criterion `tool_export_json` | 899.69 ns/iter | KT23 / bench |
| Loopback goodput `transfer_loopback` | 12.58 GB/s mean over 10 × 1 s iters (100.7 Gbit/s, raw TCP, no SSH) | KT23 / bench |
| Headless frame `terminal_frame/headless_render_80x24` | 335.03 µs/frame (cold: terminal + atlas build each iter) | KT23 / bench |
| Headless frame `terminal_frame/headless_render_160x50` | 684.34 µs/frame | KT23 / bench |
| Headless flood 240 frames (steady state) | p50 ≈ 54 µs, p95 ≈ 55–60 µs, 100% ≤ 16.7 ms | KT23 / bench |
| SSH handshake N=10 vs disposable openssh:2223 | median 105 ms, p95 114 ms, max 114 ms (target < 3000 ms) | KT23 / test |
| SSH shell echo probe | 1 ms round trip | KT23 / test |
| SFTP 512 MiB vs disposable atmoz:2222 (depth 16) | **up 689.8 MB/s (0.78 s), down 517.1 MB/s (1.04 s)**, checksums verified, independently re-run on the migrated stack (agent repeat 649.8/412.6) | KT23 / test |
| SFTP 1 GiB vs disposable atmoz:2222 | **up 716.8 MB/s (1.50 s), down 672.3 MB/s (1.60 s)**, checksums verified (agent repeat 719.9/525.1); deterministic rekey repro (`RekeyLimit 16M`) completed with 62 `ssh_set_newkeys: rekeying` events | KT23 / test |
| SFTP pre-fix (russh 0.45.0, same server) | up 433.5 MB/s; down 37.0 MB/s and a 1 GiB stall at ~800 MiB — root cause: the client rejected OpenSSH's mid-session rekey (`StrictKeyExchangeViolation`, RUST_LOG trace); fixed by the 0.61.1 migration plus the parallel-handle read pipeline in `src/connection/sftp/session.rs` | KT23 / test |
| 10-session soak (80×24 + headless renderer) | 2.4603 ms/iter; slope 0 (ΔVmRSS = 0 kB over 4000+ iters post-warmup); peak VmHWM ≈ 171 MB in-harness | KT23 / bench |
| 8 h M5 soak: 10 SSH sessions + looping 128 MiB SFTP vs disposable 2323/2324 | 481 samples; peak VmHWM 15132 KiB (14.8 MiB); VmRSS slope h2–8 177.8 KiB/h; 10/10 sessions alive, 0 reconnects (SSH/SFTP), 9600 ticks, 2.75 TB moved; CSV `docs/assets/soak-rss-8h.csv` | KT23 / test |
| Composited present path (Xephyr/GPU frame times) | UNMEASURED: no automated present harness; display :1 (7680×3600) and GPU present, headless CPU path above drives the same instance builder | — |
| Cold start / idle memory of packaged release | UNMEASURED: no packaged release built in this workstream | — |

No numeric result is claimed until it is captured on a named host. PRs
changing hot paths must include the before/after Criterion output and the
runtime workload measurements relevant to the change.

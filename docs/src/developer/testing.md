---
title: Testing
---

# Testing

Remote App follows a testing pyramid: fast unit/property tests are the normal
gate, module integration tests verify boundaries, and live end-to-end tests
run in disposable environments. New behavior needs a test at the lowest tier
that can prove it.

## Unit and Property Tests

Run the workspace tests with `cargo test --workspace`. Pure tests cover core
models, terminal grids, parsers, crypto framing, session storage, transfer
math, UI message mapping, and network-tool parsing. `proptest` tests ensure
malformed subnet, ping, and port specifications do not panic. Use `rstest` for
small parameter matrices and keep I/O out of unit tests.

## Integration Tests

The `tests/` directory contains config, crypto, network-tool, property, and
snapshot integration binaries. `src/test_utils` provides temporary config,
headless terminal, fixture, and live-server environment helpers. Snapshots are
reviewed with `cargo insta review`; update them only when the output change is
intentional.

The existing SSH/SFTP, forwarding, X11, and VNC tests are opt-in when a live
server is configured. They do not silently connect to arbitrary hosts. Use
disposable Docker services and set the documented `MBXT_*_TEST_*` variables.

## End-to-End and Containers

CI builds the application and runs non-interactive integration tests on every
pull request. Nightly jobs should start disposable OpenSSH/SFTP and VNC
containers, run the ignored live tests, and destroy the containers even after
failure. UI screenshot tests require a configured X11/Wayland runner and are
not treated as reliable on a headless developer machine.

Run Criterion performance tests with `cargo bench --bench performance`.
Security parser fuzz targets live under `fuzz/` and use cargo-fuzz/nightly.
Fuzz inputs must not be printed because they may contain credentials or
terminal control sequences.

Run coverage locally with `cargo llvm-cov --workspace --all-features --html`.
The project target is greater than 80 percent line coverage; coverage must not
decrease for a pull request without an explicit maintainer decision. Run short
fuzz smoke tests with `cargo fuzz run vte_parser -- -runs=1000` and use longer
runs in scheduled CI.

When changing a public API, run `cargo doc --workspace --no-deps`. For release
validation, use the packaging containers or CI rather than relying on a
developer workstation's native libraries.

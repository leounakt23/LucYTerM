use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use mbxt_terminal::Grid;
use remote_app::tools::{export_lines, subnet};
use remote_app::utils::config::AppConfig;

fn terminal_scroll(c: &mut Criterion) {
    let mut group = c.benchmark_group("terminal");
    for rows in [24_u16, 100, 500] {
        group.bench_with_input(
            BenchmarkId::new("scroll_region_up", rows),
            &rows,
            |b, &rows| {
                b.iter(|| {
                    let mut grid = Grid::new(160, rows, 10_000);
                    for _ in 0..black_box(100_u16) {
                        grid.scroll_region_up(0, rows - 1, 1);
                    }
                    black_box(grid.scrollback_len());
                });
            },
        );
    }
    group.finish();
}

fn terminal_search(c: &mut Criterion) {
    c.bench_function("terminal_search_160x500", |b| {
        b.iter(|| {
            let grid = Grid::new(160, 500, 10_000);
            black_box(grid.search("needle"));
        });
    });
}

fn terminal_vte(c: &mut Criterion) {
    let output = b"\x1b[1;32mremote-app\x1b[0m $ echo benchmark\r\nbenchmark\r\n";
    c.bench_function("terminal_vte_output", |b| {
        b.iter(|| {
            let mut terminal = mbxt_terminal::Terminal::new(160, 50, 10_000);
            terminal.write_bytes(black_box(output));
            black_box(terminal.get_cell(0, 0));
        });
    });
}

fn config_serialization(c: &mut Criterion) {
    let config = AppConfig::default();
    c.bench_function("config_ron_serialization", |b| {
        b.iter(|| {
            black_box(ron::ser::to_string_pretty(
                black_box(&config),
                ron::ser::PrettyConfig::default(),
            ))
        });
    });
}

fn utility_hot_paths(c: &mut Criterion) {
    c.bench_function("subnet_calculate", |b| {
        b.iter(|| black_box(subnet::calculate("10.20.30.40/16").unwrap()));
    });
    let lines = vec![(remote_app::tools::OutputLevel::Info, "response".repeat(32))];
    c.bench_function("tool_export_json", |b| {
        b.iter(|| black_box(export_lines(&lines, "json")));
    });
}

/// Loopback goodput of the native bandwidth sender (1 s blast into a
/// draining loopback peer). Reported time/iter ≈ 1 s; divide returned
/// bytes by it for MB/s. Debug and release both meaningful; record the
/// profile alongside the number.
fn transfer_loopback(c: &mut Criterion) {
    use remote_app::tools::{bandwidth, CancelToken};
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("bench runtime");
    let mut group = c.benchmark_group("transfer_loopback");
    group.sample_size(10);
    group.bench_function("bandwidth_sender_1s", |b| {
        use std::time::Instant;
        b.iter_custom(|iters| {
            let mut total_bytes = 0u64;
            let started = Instant::now();
            for _ in 0..iters {
                total_bytes += runtime.block_on(async {
                    use tokio::io::AsyncReadExt;
                    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
                        .await
                        .expect("loopback bind");
                    let port = listener.local_addr().expect("addr").port();
                    let drain = tokio::spawn(async move {
                        let (mut socket, _) = listener.accept().await.expect("accept");
                        let mut bytes = 0u64;
                        let mut chunk = [0u8; 65536];
                        while let Ok(count) = socket.read(&mut chunk).await {
                            if count == 0 {
                                break;
                            }
                            bytes += count as u64;
                        }
                        bytes
                    });
                    let stats =
                        bandwidth::run_sender("127.0.0.1", port, 1, 2000, CancelToken::new())
                            .await
                            .expect("sender");
                    let drained = drain.await.expect("drain task");
                    assert_eq!(drained, stats.bytes, "loopback must not lose bytes");
                    stats.bytes
                });
            }
            let elapsed = started.elapsed();
            eprintln!(
                "[measure] {total_bytes} bytes in {elapsed:.2?} = {:.1} Mbit/s",
                total_bytes as f64 * 8.0 / elapsed.as_secs_f64() / 1_000_000.0
            );
            elapsed
        });
    });
    group.finish();
}

/// Headless frame-time capture (M3 60FPS workstream): full-frame renders
/// through `TerminalRenderer::render` (CPU fallback path, no GPU needed).
/// The `headless_flood_240frames` bench pushes every `total_us` into a
/// `FrameTimeRing` and prints p50/p95/%<16.7ms; the per-size benches give
/// Criterion means for regression tracking.
fn terminal_frame_render(c: &mut Criterion) {
    use mbxt_terminal::renderer::{FrameTimeRing, TerminalRenderer};
    use std::time::{Duration, Instant};

    // Deterministic flood: printable ASCII + SGR color + CJK wide chars.
    fn flood_bytes(cols: u16, rows: u16) -> Vec<u8> {
        let mut out = Vec::new();
        for row in 0..rows {
            out.extend_from_slice(b"\x1b[1;32m");
            for _ in 0..cols {
                out.extend_from_slice("A漢".as_bytes());
            }
            out.extend_from_slice(b"\x1b[0m\r\n");
            let _ = row;
        }
        out
    }

    let mut group = c.benchmark_group("terminal_frame");
    for (name, cols, rows) in [("80x24", 80_u16, 24_u16), ("160x50", 160, 50)] {
        group.bench_function(format!("headless_render_{name}"), |b| {
            let flood = flood_bytes(cols, rows);
            b.iter(|| {
                let mut terminal = mbxt_terminal::Terminal::new(cols, rows, 10_000);
                terminal.write_bytes(black_box(flood.as_slice()));
                let mut renderer = TerminalRenderer::new_headless(13);
                let stats = renderer.render(&terminal).expect("headless render");
                black_box(stats.total_us);
            });
        });
    }
    group.bench_function("headless_flood_240frames", |b| {
        b.iter_custom(|iters| {
            let flood = flood_bytes(80, 24);
            let mut ring = FrameTimeRing::new(240);
            let started = Instant::now();
            for _ in 0..iters {
                let mut terminal = mbxt_terminal::Terminal::new(80, 24, 10_000);
                terminal.write_bytes(flood.as_slice());
                let mut renderer = TerminalRenderer::new_headless(13);
                // 240 consecutive frames on one renderer (atlas warms up,
                // ring holds the steady state).
                for _ in 0..240 {
                    if let Ok(stats) = renderer.render(&terminal) {
                        ring.push_us(stats.total_us);
                    }
                }
            }
            let elapsed = started.elapsed();
            let p50 = ring.p50_us().unwrap_or(0);
            let p95 = ring.p95_us().unwrap_or(0);
            let under = ring.fraction_under(Duration::from_micros(16_700)) * 100.0;
            eprintln!(
                "[measure] headless 80x24 flood: p50={p50}us p95={p95}us <16.7ms={under:.1}% n={} (ring cap {})",
                ring.len(),
                ring.capacity()
            );
            elapsed
        });
    });
    group.finish();
}

/// 10-session soak RSS sampler (M5 memory workstream): hold ten live
/// 80x24 terminals + headless renderers, render one frame each, and report
/// VmRSS/VmHWM from `/proc/self/status`. Flat slope == no per-session leak;
/// absolute peak is host-specific (record alongside the number).
fn session_soak_rss(c: &mut Criterion) {
    use mbxt_terminal::renderer::TerminalRenderer;

    fn rss_kb() -> (u64, u64) {
        let text = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
        let mut rss = 0u64;
        let mut hwm = 0u64;
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("VmRSS:") {
                rss = rest
                    .split_whitespace()
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
            } else if let Some(rest) = line.strip_prefix("VmHWM:") {
                hwm = rest
                    .split_whitespace()
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
            }
        }
        (rss, hwm)
    }

    c.bench_function("session_soak_10x80x24_rss", |b| {
        b.iter_custom(|iters| {
            let started = std::time::Instant::now();
            for _ in 0..iters {
                let before = rss_kb().0;
                let mut sessions = Vec::new();
                for i in 0..10_u32 {
                    let mut terminal = mbxt_terminal::Terminal::new(80, 24, 10_000);
                    let line = format!("session {i} soak line\r\n");
                    for _ in 0..24 {
                        terminal.write_bytes(line.as_bytes());
                    }
                    let mut renderer = TerminalRenderer::new_headless(13);
                    let _ = renderer.render(&terminal);
                    sessions.push((terminal, renderer));
                }
                let (after, hwm) = rss_kb();
                let delta = after.saturating_sub(before);
                eprintln!(
                    "[measure] 10-session soak: VmRSS={after}kB VmHWM={hwm}kB delta={delta}kB sessions={} (~{}kB/session)",
                    sessions.len(),
                    delta / 10,
                );
                black_box(sessions.len());
            }
            started.elapsed()
        });
    });
}

criterion_group!(
    performance,
    terminal_scroll,
    terminal_search,
    terminal_vte,
    config_serialization,
    utility_hot_paths,
    transfer_loopback,
    terminal_frame_render,
    session_soak_rss
);
criterion_main!(performance);

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
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

criterion_group!(
    performance,
    terminal_scroll,
    terminal_search,
    terminal_vte,
    config_serialization,
    utility_hot_paths,
    transfer_loopback
);
criterion_main!(performance);

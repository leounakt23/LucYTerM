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

criterion_group!(
    performance,
    terminal_scroll,
    terminal_search,
    terminal_vte,
    config_serialization,
    utility_hot_paths
);
criterion_main!(performance);

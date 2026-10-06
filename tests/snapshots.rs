use insta::assert_snapshot;
use remote_app::tools::{export_lines, OutputLevel};

#[test]
fn exported_output_has_stable_text_shape() {
    let lines = vec![
        (OutputLevel::Info, "hello".to_string()),
        (OutputLevel::Error, "failed".to_string()),
    ];
    assert_snapshot!(export_lines(&lines, "txt"));
}

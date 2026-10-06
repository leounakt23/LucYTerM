use remote_app::tools::dns::{parse_record_type, RECORD_TYPES};
use remote_app::tools::ping::{parse_sample, parse_summary};
use remote_app::tools::port_scanner::parse_ports;
use remote_app::tools::subnet::calculate;
use remote_app::tools::{export_lines, filter_lines, OutputLevel};

#[test]
fn network_parsers_handle_expected_inputs() {
    assert_eq!(
        parse_sample("64 bytes from host: icmp_seq=2 time=4.5 ms")
            .unwrap()
            .seq,
        2
    );
    assert_eq!(parse_ports("22,80-82,80").unwrap(), vec![22, 80, 81, 82]);
    assert_eq!(
        parse_record_type("mx").unwrap(),
        hickory_resolver::proto::rr::RecordType::MX
    );
    assert_eq!(RECORD_TYPES.len(), 5);
    assert_eq!(calculate("192.168.10.7/24").unwrap().host_count, 254);
    let summary = parse_summary(&["2 packets transmitted, 1 received, 50% packet loss".into()]);
    assert_eq!((summary.transmitted, summary.received), (2, 1));
}

#[test]
fn output_helpers_filter_and_export() {
    let lines = vec![
        (OutputLevel::Info, "hello".into()),
        (OutputLevel::Error, "failed".into()),
    ];
    assert_eq!(filter_lines(&lines, "FAIL").len(), 1);
    assert_eq!(export_lines(&lines, "txt"), "hello\nfailed");
    assert!(export_lines(&lines, "json").contains("\"level\": \"Info\""));
}

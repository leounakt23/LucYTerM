use proptest::prelude::*;

proptest! {
    #[test]
    fn subnet_parser_never_panics(input in any::<String>()) {
        let _ = remote_app::tools::subnet::calculate(&input);
    }

    #[test]
    fn ping_parser_never_panics(input in any::<String>()) {
        let _ = remote_app::tools::ping::parse_sample(&input);
    }

    #[test]
    fn port_parser_never_panics(input in any::<String>()) {
        let _ = remote_app::tools::port_scanner::parse_ports(&input);
    }
}

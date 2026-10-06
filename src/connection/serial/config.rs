//! Serial configuration (Prompt 4.4 output path): validated settings +
//! device picker list.
//!
//! Pure data over [`mbxt_core::SerialParams`] — headless-testable. Opening
//! hardware lives in [`super::session::SerialSession`] (transport) and the
//! `serialport` backend.

pub use mbxt_core::{SerialFlowControl, SerialParity, COMMON_BAUD_RATES};

/// Validated serial settings for one console session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerialConfig {
    pub params: mbxt_core::SerialParams,
}

impl SerialConfig {
    /// Validate (`open` refuses the same way; the dialog surfaces `Err`).
    pub fn new(params: mbxt_core::SerialParams) -> Result<Self, String> {
        params.validate().map_err(str::to_string)?;
        Ok(Self { params })
    }

    /// Picker entries (`/dev/ttyUSB0 — USB-Serial …`), empty without the
    /// `serial` feature or hardware.
    pub fn available_ports() -> Vec<String> {
        #[cfg(feature = "serial")]
        {
            mbxt_connections::serial::list_ports()
        }
        #[cfg(not(feature = "serial"))]
        {
            Vec::new()
        }
    }

    /// `true` when the backend is compiled in.
    pub fn backend_available() -> bool {
        cfg!(feature = "serial")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> mbxt_core::SerialParams {
        mbxt_core::SerialParams {
            device: "/dev/ttyUSB0".into(),
            baud_rate: 115200,
            ..mbxt_core::SerialParams::default()
        }
    }

    #[test]
    fn valid_config_accepted() {
        assert!(SerialConfig::new(params()).is_ok());
    }

    #[test]
    fn invalid_configs_rejected_with_actionable_errors() {
        let mut bad = params();
        bad.device.clear();
        assert!(SerialConfig::new(bad.clone()).is_err());
        bad = params();
        bad.baud_rate = 0;
        assert!(SerialConfig::new(bad.clone()).is_err());
        bad = params();
        bad.data_bits = 4;
        assert!(SerialConfig::new(bad.clone()).is_err());
        bad = params();
        bad.stop_bits = 3;
        assert!(SerialConfig::new(bad).is_err());
    }

    #[test]
    fn common_bauds_cover_typical_consoles() {
        assert!(COMMON_BAUD_RATES.contains(&9600));
        assert!(COMMON_BAUD_RATES.contains(&115200));
    }

    #[test]
    fn port_listing_never_fails() {
        // Hardware absence is an empty list, not an error.
        let _ = SerialConfig::available_ports();
    }
}

use remote_app::test_utils::TempConfig;
use remote_app::utils::config::{self, AppConfig};
use remote_app::utils::crypto;
use remote_app::utils::secure_storage::SecureStorage;

#[test]
fn config_round_trip_uses_private_storage() {
    let dir = TempConfig::new().unwrap();
    let mut expected = AppConfig::default();
    expected.terminal.scrollback_lines = 42;
    expected.security.lock_after_idle_secs = 900;
    config::save(dir.path(), &expected).unwrap();
    assert_eq!(config::load(dir.path()).unwrap(), expected);
    assert!(dir.path().join("config.ron").is_file());
}

#[test]
fn malformed_config_is_moved_aside_and_defaults_returned() {
    let dir = TempConfig::new().unwrap();
    std::fs::write(dir.path().join("config.ron"), b"not valid ron").unwrap();
    assert_eq!(config::load(dir.path()).unwrap(), AppConfig::default());
    let backups = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("config.ron.invalid-")
        })
        .count();
    assert_eq!(backups, 1);
}

#[test]
fn encrypted_store_rejects_tampering() {
    let dir = TempConfig::new().unwrap();
    let storage = SecureStorage::new(dir.path());
    let blob = storage.encrypt_sessions(&[], "test-password").unwrap();
    let mut tampered = blob;
    *tampered.last_mut().unwrap() ^= 1;
    assert!(storage
        .decrypt_sessions(&tampered, "test-password")
        .is_err());
    assert_eq!(crypto::NONCE_LEN, 12);
}

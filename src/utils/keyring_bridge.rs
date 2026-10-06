//! Keyring bridge: master-password caching in the desktop keyring
//! (Secret Service on Linux; opt-in via `SecuritySettings::keyring_enabled`,
//! feature matrix #49). Isolated so `mbxt-system` stays the only dbus touchpoint.

/// Keyring entry identifiers for the master password.
pub const KEYRING_SERVICE: &str = "remote-app";
pub const KEYRING_USER: &str = "master";

/// Keyring failures surfaced to callers.
#[derive(Debug, thiserror::Error)]
pub enum KeyringBridgeError {
    #[error("{0}")]
    System(#[from] mbxt_system::SystemError),
}

/// Store the master password (user opt-in only — never implicit).
pub fn store_master_password(password: &str) -> Result<(), KeyringBridgeError> {
    mbxt_system::keyring::store_secret(KEYRING_SERVICE, KEYRING_USER, password)?;
    Ok(())
}

/// Read the cached master password (None when absent).
pub fn load_master_password() -> Result<Option<String>, KeyringBridgeError> {
    Ok(mbxt_system::keyring::get_secret(
        KEYRING_SERVICE,
        KEYRING_USER,
    )?)
}

/// Remove the cached master password (logout / opt-out).
pub fn delete_master_password() -> Result<(), KeyringBridgeError> {
    mbxt_system::keyring::delete_secret(KEYRING_SERVICE, KEYRING_USER)?;
    Ok(())
}

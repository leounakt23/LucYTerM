//! Desktop keyring integration via the Secret Service (gnome-keyring /
//! KWallet) or platform-native backends (feature matrix #49).

/// Store a session secret under `service`/`user`.
pub fn store_secret(service: &str, user: &str, secret: &str) -> Result<(), crate::SystemError> {
    let entry = keyring::Entry::new(service, user)?;
    entry.set_password(secret)?;
    Ok(())
}

/// Retrieve a session secret. Absence is not an error — returns `None`.
pub fn get_secret(service: &str, user: &str) -> Result<Option<String>, crate::SystemError> {
    let entry = keyring::Entry::new(service, user)?;
    match entry.get_password() {
        Ok(secret) => Ok(Some(secret)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(err) => Err(err.into()),
    }
}

/// Remove a stored secret (session deletion path).
pub fn delete_secret(service: &str, user: &str) -> Result<(), crate::SystemError> {
    let entry = keyring::Entry::new(service, user)?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(err.into()),
    }
}

#[cfg(test)]
mod tests {
    // No keyring in CI — covered by manual acceptance (feature §4.1).
}

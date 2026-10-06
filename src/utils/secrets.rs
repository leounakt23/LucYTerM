//! Explicit secret containers for values crossing UI, storage, and transport
//! boundaries. Debug output never contains the underlying value.

use secrecy::{ExposeSecret, SecretBox, SecretString as SecrecyString};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

#[derive(Clone)]
pub struct SecretText(SecrecyString);

impl SecretText {
    pub fn new(value: impl Into<String>) -> Self {
        Self(SecrecyString::new(value.into().into()))
    }
    pub fn expose_secret(&self) -> &str {
        self.0.expose_secret()
    }
    pub fn constant_time_eq(&self, other: &Self) -> bool {
        self.expose_secret()
            .as_bytes()
            .ct_eq(other.expose_secret().as_bytes())
            .into()
    }
}

impl std::fmt::Debug for SecretText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecretText").finish_non_exhaustive()
    }
}

pub struct SecretBytes(SecretBox<Vec<u8>>);

impl Clone for SecretBytes {
    fn clone(&self) -> Self {
        Self::new(self.expose_secret().to_vec())
    }
}

impl SecretBytes {
    pub fn new(value: Vec<u8>) -> Self {
        Self(SecretBox::new(value.into()))
    }
    pub fn expose_secret(&self) -> &[u8] {
        self.0.expose_secret()
    }
    pub fn constant_time_eq(&self, other: &Self) -> bool {
        self.expose_secret().ct_eq(other.expose_secret()).into()
    }
}

impl std::fmt::Debug for SecretBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecretBytes").finish_non_exhaustive()
    }
}

pub type TemporarySecret = Zeroizing<Vec<u8>>;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn secrets_do_not_debug_leak_and_compare() {
        let left = SecretText::new("password");
        let right = SecretText::new("password");
        assert!(left.constant_time_eq(&right));
        assert!(!format!("{left:?}").contains("password"));
    }
}

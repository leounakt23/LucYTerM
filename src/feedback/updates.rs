//! Signed, channel-specific update manifests.

use crate::utils::config::ReleaseChannel;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateManifest {
    pub version: String,
    pub channel: ReleaseChannel,
    pub artifact_url: String,
    pub sha256: String,
    /// Ed25519 signature over the other fields in their documented order.
    pub signature: String,
}

impl UpdateManifest {
    fn signed_payload(&self) -> String {
        format!(
            "{}\n{}\n{}\n{}",
            self.version, self.channel, self.artifact_url, self.sha256
        )
    }

    pub fn verify(&self, public_key: &[u8; 32]) -> Result<(), String> {
        let key = VerifyingKey::from_bytes(public_key).map_err(|error| error.to_string())?;
        let signature_bytes = decode_hex::<64>(&self.signature)?;
        let signature =
            Signature::from_slice(&signature_bytes).map_err(|error| error.to_string())?;
        key.verify(self.signed_payload().as_bytes(), &signature)
            .map_err(|error| format!("invalid update signature: {error}"))
    }

    pub fn verify_artifact(&self, bytes: &[u8]) -> Result<(), String> {
        let actual = format!("{:x}", Sha256::digest(bytes));
        if actual == self.sha256.to_ascii_lowercase() {
            Ok(())
        } else {
            Err("update artifact checksum does not match signed manifest".to_string())
        }
    }
}

#[cfg(feature = "feedback-network")]
pub async fn check(
    base_url: &str,
    channel: ReleaseChannel,
    public_key: &[u8; 32],
) -> Result<UpdateManifest, String> {
    let manifest = reqwest::Client::new()
        .get(format!(
            "{}/{channel}/manifest.json",
            base_url.trim_end_matches('/')
        ))
        .header(reqwest::header::USER_AGENT, "remote-app-updater")
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?
        .json::<UpdateManifest>()
        .await
        .map_err(|error| error.to_string())?;
    if manifest.channel != channel {
        return Err("update manifest channel does not match selection".to_string());
    }
    manifest.verify(public_key)?;
    Ok(manifest)
}

pub fn public_key_from_hex(value: &str) -> Result<[u8; 32], String> {
    decode_hex(value)
}

fn decode_hex<const N: usize>(value: &str) -> Result<[u8; N], String> {
    if value.len() != N * 2 {
        return Err(format!("expected {} hexadecimal characters", N * 2));
    }
    let mut output = [0_u8; N];
    for (index, byte) in output.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| "invalid hexadecimal value".to_string())?;
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_wrong_hex_length() {
        assert!(public_key_from_hex("abcd").is_err());
    }
}

/// Key derivation for KDBX databases using Argon2id.
use argon2::Argon2;
use rand::RngCore;
use sha2::{Sha256, Digest};

/// KDF parameters for deriving the master key from a password.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KdfParams {
    /// Memory cost in kibibytes (standard Argon2 m parameter)
    pub memory_kib: u32,
    /// Number of iterations (standard Argon2 t parameter)
    pub iterations: u32,
    /// Degree of parallelism (standard Argon2 p parameter)
    pub parallelism: u32,
    /// Output key length
    pub output_len: usize,
}

impl Default for KdfParams {
    fn default() -> Self {
        Self {
            memory_kib: 65536,  // 64 MB
            iterations: 3,
            parallelism: 4,
            output_len: 32,
        }
    }
}

/// Derive a composite key from master password and optional key file data.
///
/// KDBX uses a composite key: SHA-256(master_key | key_file_data).
/// The master_key is derived via Argon2id from the password.
pub fn derive_composite_key(
    password: &str,
    keyfile_data: Option<&[u8]>,
    kdf: &KdfParams,
) -> Result<Vec<u8>, String> {
    let salt_bytes = generate_salt(32);
    derive_composite_key_with_salt(password, keyfile_data, kdf, &salt_bytes)
}

/// Derive a composite key using an externally provided salt (for KDBX write/read consistency).
pub fn derive_composite_key_with_salt(
    password: &str,
    keyfile_data: Option<&[u8]>,
    kdf: &KdfParams,
    salt: &[u8],
) -> Result<Vec<u8>, String> {
    let master_key = derive_master_key(password, salt, kdf)?;

    let composite_key = if let Some(kf_data) = keyfile_data {
        let mut hasher = Sha256::new();
        hasher.update(&master_key);
        hasher.update(kf_data);
        hasher.finalize().to_vec()
    } else {
        master_key
    };

    Ok(composite_key)
}

/// Derive a master encryption key from a password using Argon2id.
pub fn derive_master_key(
    password: &str,
    salt: &[u8],
    kdf: &KdfParams,
) -> Result<Vec<u8>, String> {
    let params = argon2::Params::new(
        kdf.memory_kib,
        kdf.iterations,
        kdf.parallelism,
        Some(kdf.output_len),
    )
    .map_err(|e| format!("Invalid Argon2 parameters: {e}"))?;

    let argon2 = Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        params,
    );

    let mut output = vec![0u8; kdf.output_len];
    argon2
        .hash_password_into(password.as_bytes(), salt, &mut output)
        .map_err(|e| format!("Argon2 key derivation failed: {e}"))?;

    Ok(output)
}

/// Verify that a password produces a given composite key (used for unlocking).
pub fn verify_composite_key(
    password: &str,
    keyfile_data: Option<&[u8]>,
    expected_key: &[u8],
    kdf: &KdfParams,
    salt: &[u8],
) -> Result<bool, String> {
    let master_key = derive_master_key(password, salt, kdf)?;

    let candidate = if let Some(kf_data) = keyfile_data {
        let mut hasher = Sha256::new();
        hasher.update(&master_key);
        hasher.update(kf_data);
        hasher.finalize().to_vec()
    } else {
        master_key
    };

    Ok(candidate == expected_key)
}

/// Generate secure random salt bytes.
pub fn generate_salt(len: usize) -> Vec<u8> {
    let mut salt = vec![0u8; len];
    rand::rngs::OsRng.fill_bytes(&mut salt);
    salt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_and_verify() {
        let password = "correct-horse-battery-staple";
        let kdf = KdfParams::default();
        let salt = generate_salt(32);

        let key = derive_master_key(password, &salt, &kdf).unwrap();
        let valid = verify_composite_key(password, None, &key, &kdf, &salt).unwrap();
        let invalid = verify_composite_key("wrong-password", None, &key, &kdf, &salt).unwrap();

        assert!(valid);
        assert!(!invalid);
    }

    #[test]
    fn test_keyfile_affects_key() {
        let password = "test-password";
        let kdf = KdfParams::default();
        let salt = generate_salt(32);

        let key_no_kf = derive_composite_key_with_salt(password, None, &kdf, &salt).unwrap();
        let key_with_kf =
            derive_composite_key_with_salt(password, Some(b"keyfile-data"), &kdf, &salt).unwrap();

        assert_ne!(key_no_kf, key_with_kf);
    }
}

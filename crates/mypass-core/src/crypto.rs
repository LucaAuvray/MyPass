/// KDBX crypto operations: AES-256-GCM and ChaCha20-Poly1305 encryption/decryption.
use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce as AesNonce,
};
use chacha20poly1305::{ChaCha20Poly1305, Nonce as ChaChaNonce};
use rand::RngCore;

/// Supported encryption ciphers for KDBX databases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cipher {
    Aes256,
    ChaCha20,
}

impl Cipher {
    pub fn key_size(&self) -> usize {
        match self {
            Cipher::Aes256 => 32,
            Cipher::ChaCha20 => 32,
        }
    }

    pub fn nonce_size(&self) -> usize {
        match self {
            Cipher::Aes256 => 12,
            Cipher::ChaCha20 => 12,
        }
    }
}

/// Encrypt plaintext using the specified cipher.
/// Returns (nonce, ciphertext). Nonce is randomly generated.
pub fn encrypt(plaintext: &[u8], key: &[u8], cipher: Cipher) -> Result<(Vec<u8>, Vec<u8>), String> {
    match cipher {
        Cipher::Aes256 => encrypt_aes256(plaintext, key),
        Cipher::ChaCha20 => encrypt_chacha20(plaintext, key),
    }
}

/// Decrypt ciphertext using the specified cipher.
pub fn decrypt(
    ciphertext: &[u8],
    key: &[u8],
    nonce: &[u8],
    cipher: Cipher,
) -> Result<Vec<u8>, String> {
    match cipher {
        Cipher::Aes256 => decrypt_aes256(ciphertext, key, nonce),
        Cipher::ChaCha20 => decrypt_chacha20(ciphertext, key, nonce),
    }
}

fn encrypt_aes256(plaintext: &[u8], key: &[u8]) -> Result<(Vec<u8>, Vec<u8>), String> {
    let aead = Aes256Gcm::new_from_slice(key).map_err(|e| format!("Invalid AES key: {e}"))?;

    let mut nonce_bytes = vec![0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = AesNonce::from_slice(&nonce_bytes);

    let ciphertext = aead
        .encrypt(nonce, plaintext)
        .map_err(|e| format!("AES encryption failed: {e}"))?;

    Ok((nonce_bytes, ciphertext))
}

fn decrypt_aes256(ciphertext: &[u8], key: &[u8], nonce: &[u8]) -> Result<Vec<u8>, String> {
    let aead = Aes256Gcm::new_from_slice(key).map_err(|e| format!("Invalid AES key: {e}"))?;
    let nonce = AesNonce::from_slice(nonce);

    let plaintext = aead
        .decrypt(nonce, ciphertext)
        .map_err(|e| format!("AES decryption failed: {e}"))?;

    // Zeroize the plaintext when dropped (caller should clone before drop if needed)
    // We return it so the caller manages lifecycle
    Ok(plaintext)
}

fn encrypt_chacha20(plaintext: &[u8], key: &[u8]) -> Result<(Vec<u8>, Vec<u8>), String> {
    let aead = ChaCha20Poly1305::new_from_slice(key)
        .map_err(|e| format!("Invalid ChaCha20 key: {e}"))?;

    let mut nonce_bytes = vec![0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = ChaChaNonce::from_slice(&nonce_bytes);

    let ciphertext = aead
        .encrypt(nonce, plaintext)
        .map_err(|e| format!("ChaCha20 encryption failed: {e}"))?;

    Ok((nonce_bytes, ciphertext))
}

fn decrypt_chacha20(ciphertext: &[u8], key: &[u8], nonce: &[u8]) -> Result<Vec<u8>, String> {
    let aead = ChaCha20Poly1305::new_from_slice(key)
        .map_err(|e| format!("Invalid ChaCha20 key: {e}"))?;
    let nonce = ChaChaNonce::from_slice(nonce);

    let plaintext = aead
        .decrypt(nonce, ciphertext)
        .map_err(|e| format!("ChaCha20 decryption failed: {e}"))?;

    Ok(plaintext)
}

/// Generate a random encryption key of the given size.
pub fn generate_random_key(size: usize) -> Vec<u8> {
    let mut key = vec![0u8; size];
    OsRng.fill_bytes(&mut key);
    key
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aes256_encrypt_decrypt_roundtrip() {
        let key = generate_random_key(32);
        let plaintext = b"Hello, KDBX! This is a test.";

        let (nonce, ciphertext) = encrypt(plaintext, &key, Cipher::Aes256).unwrap();
        let decrypted = decrypt(&ciphertext, &key, &nonce, Cipher::Aes256).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_chacha20_encrypt_decrypt_roundtrip() {
        let key = generate_random_key(32);
        let plaintext = b"Hello, KDBX! This is a test.";

        let (nonce, ciphertext) = encrypt(plaintext, &key, Cipher::ChaCha20).unwrap();
        let decrypted = decrypt(&ciphertext, &key, &nonce, Cipher::ChaCha20).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_wrong_key_fails() {
        let key = generate_random_key(32);
        let wrong_key = generate_random_key(32);
        let plaintext = b"secret data";

        let (nonce, ciphertext) = encrypt(plaintext, &key, Cipher::Aes256).unwrap();
        let result = decrypt(&ciphertext, &wrong_key, &nonce, Cipher::Aes256);

        assert!(result.is_err());
    }
}

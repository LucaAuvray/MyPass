//! NaCl box protocol implementation for browser integration using crypto_box crate.
//!
//! Protocol overview:
//! 1. Browser extension generates key pair (public + secret)
//! 2. Extension sends public key to MyPass
//! 3. MyPass generates its own key pair and sends public key back
//! 4. All subsequent messages are encrypted using NaCl box (Curve25519 + XSalsa20-Poly1305)

use crypto_box::aead::{Aead, AeadCore, OsRng};
use crypto_box::{PublicKey, SecretKey, SalsaBox};
use rand::RngCore;

/// Size of NaCl public/secret keys in bytes.
pub const KEY_SIZE: usize = 32;

/// Size of NaCl nonce in bytes.
pub const NONCE_SIZE: usize = 24;

/// Generate a new random key pair (public + secret).
pub fn generate_keypair() -> (Vec<u8>, Vec<u8>) {
    let secret = SecretKey::generate(&mut OsRng);
    let public = secret.public_key();
    (public.as_bytes().to_vec(), secret.to_bytes().to_vec())
}

/// Generate a random 24-byte nonce for NaCl box.
pub fn generate_nonce() -> Vec<u8> {
    let mut nonce = [0u8; NONCE_SIZE];
    rand::thread_rng().fill_bytes(&mut nonce);
    nonce.to_vec()
}

/// Encrypt a plaintext message using NaCl box (Curve25519 + XSalsa20-Poly1305).
///
/// Returns (ciphertext, nonce). The nonce must be sent along with the ciphertext.
pub fn encrypt(
    plaintext: &[u8],
    their_public_key: &[u8],
    our_secret_key: &[u8],
) -> Result<(Vec<u8>, Vec<u8>), String> {
    let their_pk = PublicKey::from_slice(their_public_key)
        .map_err(|e| format!("Invalid public key: {e}"))?;
    let our_sk = SecretKey::from_slice(our_secret_key)
        .map_err(|e| format!("Invalid secret key: {e}"))?;

    let salsa_box = SalsaBox::new(&their_pk, &our_sk);

    let nonce = SalsaBox::generate_nonce(&mut OsRng);
    let ciphertext = salsa_box
        .encrypt(&nonce, plaintext)
        .map_err(|e| format!("Encryption error: {e}"))?;

    Ok((ciphertext, nonce.as_slice().to_vec()))
}

/// Encrypt with a caller-chosen nonce. The KeePassXC-Browser protocol
/// requires responses to be encrypted with the request nonce incremented by
/// one — a random nonce would decrypt fine but fail the extension's
/// nonce-match verification.
pub fn encrypt_with_nonce(
    plaintext: &[u8],
    nonce: &[u8],
    their_public_key: &[u8],
    our_secret_key: &[u8],
) -> Result<Vec<u8>, String> {
    let their_pk = PublicKey::from_slice(their_public_key)
        .map_err(|e| format!("Invalid public key: {e}"))?;
    let our_sk = SecretKey::from_slice(our_secret_key)
        .map_err(|e| format!("Invalid secret key: {e}"))?;
    if nonce.len() != NONCE_SIZE {
        return Err(format!("Invalid nonce length: {}", nonce.len()));
    }

    let salsa_box = SalsaBox::new(&their_pk, &our_sk);
    salsa_box
        .encrypt(crypto_box::Nonce::from_slice(nonce), plaintext)
        .map_err(|e| format!("Encryption error: {e}"))
}

/// Decrypt a ciphertext message using NaCl box.
pub fn decrypt(
    ciphertext: &[u8],
    nonce: &[u8],
    their_public_key: &[u8],
    our_secret_key: &[u8],
) -> Result<Vec<u8>, String> {
    let their_pk = PublicKey::from_slice(their_public_key)
        .map_err(|e| format!("Invalid public key: {e}"))?;
    let our_sk = SecretKey::from_slice(our_secret_key)
        .map_err(|e| format!("Invalid secret key: {e}"))?;

    let salsa_box = SalsaBox::new(&their_pk, &our_sk);

    let nonce = crypto_box::Nonce::from_slice(nonce);

    salsa_box
        .decrypt(nonce, ciphertext)
        .map_err(|e| format!("Decryption error: {e}"))
}

/// Increment a nonce by one (for response messages). Little-endian with
/// carry, matching libsodium's `sodium_increment` — the extension increments
/// the same way and compares byte-for-byte, so big-endian would never match.
pub fn increment_nonce(nonce: &[u8]) -> Vec<u8> {
    let mut incremented = nonce.to_vec();
    let mut carry = 1u16;
    for byte in incremented.iter_mut() {
        carry += *byte as u16;
        *byte = carry as u8;
        carry >>= 8;
    }
    incremented
}

/// Generate a random client ID (24 bytes, base64 encoded).
pub fn generate_client_id() -> String {
    let mut id = [0u8; 24];
    rand::thread_rng().fill_bytes(&mut id);
    base64_encode(&id)
}

/// Simple base64 encode.
fn base64_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let (alice_pk, alice_sk) = generate_keypair();
        let (bob_pk, bob_sk) = generate_keypair();

        let plaintext = b"Hello, secure world!";
        let (ciphertext, nonce) = encrypt(plaintext, &bob_pk, &alice_sk).unwrap();

        let decrypted = decrypt(&ciphertext, &nonce, &alice_pk, &bob_sk).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_increment_nonce_libsodium_semantics() {
        // Little-endian +1 with carry, like libsodium's sodium_increment.
        assert_eq!(increment_nonce(&[0u8; 3]), vec![1, 0, 0]);
        assert_eq!(increment_nonce(&[0xff, 0, 0]), vec![0, 1, 0]);
        assert_eq!(increment_nonce(&[0xff, 0xff, 0xff]), vec![0, 0, 0]);
    }

    #[test]
    fn test_encrypt_with_nonce_roundtrip() {
        let (alice_pk, alice_sk) = generate_keypair();
        let (bob_pk, bob_sk) = generate_keypair();
        let nonce = generate_nonce();

        let ciphertext = encrypt_with_nonce(b"hello", &nonce, &bob_pk, &alice_sk).unwrap();
        let decrypted = decrypt(&ciphertext, &nonce, &alice_pk, &bob_sk).unwrap();
        assert_eq!(decrypted, b"hello");
    }

    #[test]
    fn test_wrong_key_fails() {
        let (alice_pk, alice_sk) = generate_keypair();
        let (bob_pk, _bob_sk) = generate_keypair();
        let (_, eve_sk) = generate_keypair();

        let plaintext = b"secret message";
        let (ciphertext, nonce) = encrypt(plaintext, &bob_pk, &alice_sk).unwrap();

        let result = decrypt(&ciphertext, &nonce, &alice_pk, &eve_sk);
        assert!(result.is_err());
    }
}

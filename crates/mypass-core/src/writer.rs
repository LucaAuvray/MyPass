/// KDBX file writer. Serializes XML and encrypts to .kdbx format.
use super::crypto::{self, Cipher};
use super::keys::{KdfParams, derive_composite_key_with_salt};
use super::xml::KeePassFile;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Écrit dans `<path>.tmp` puis `rename` par-dessus la cible : un crash pendant
/// l'écriture ne laisse jamais un coffre local tronqué (même précaution que
/// côté serveur).
pub fn write_database(
    path: &Path,
    keepass_file: &KeePassFile,
    password: &str,
    keyfile_data: Option<&[u8]>,
    cipher: Cipher,
    kdf: &KdfParams,
) -> Result<(), String> {
    let output = write_database_bytes(keepass_file, password, keyfile_data, cipher, kdf)?;

    let mut tmp_name = path.as_os_str().to_owned();
    tmp_name.push(".tmp");
    let tmp_path = PathBuf::from(tmp_name);

    let mut file =
        File::create(&tmp_path).map_err(|e| format!("Failed to write database: {e}"))?;
    file.write_all(&output).map_err(|e| format!("Failed to write database: {e}"))?;
    file.sync_all().map_err(|e| format!("Failed to write database: {e}"))?;
    fs::rename(&tmp_path, path).map_err(|e| format!("Failed to write database: {e}"))
}

pub fn write_database_bytes(
    keepass_file: &KeePassFile,
    password: &str,
    keyfile_data: Option<&[u8]>,
    cipher: Cipher,
    kdf: &KdfParams,
) -> Result<Vec<u8>, String> {
    let xml_str = quick_xml::se::to_string(keepass_file)
        .map_err(|e| format!("Failed to serialize KDBX XML: {e}"))?;

    // Generate master seed (salt) — stored in header so reader can derive same key
    let master_seed = super::keys::generate_salt(32);

    // Derive composite key using the master seed as salt
    let composite_key = derive_composite_key_with_salt(
        password, keyfile_data, kdf, &master_seed,
    )?;

    // Generate encryption IV (nonce) — 12 bytes for AES-GCM/ChaCha20-Poly1305
    let encryption_iv = crypto::generate_random_key(12);

    // Encrypt using the IV as nonce (don't let encrypt() generate its own)
    let encrypted_payload = encrypt_with_iv(
        xml_str.as_bytes(), &composite_key, &encryption_iv, cipher,
    )?;

    // Build KDBX binary
    let mut output = Vec::new();
    output.extend_from_slice(&0x03D9A29Au32.to_le_bytes());
    output.extend_from_slice(&0x67FB4BB5u32.to_le_bytes());
    output.extend_from_slice(&0x00040000u32.to_le_bytes());

    write_header_field(&mut output, 2, &cipher_uuid(cipher));
    write_header_field(&mut output, 3, &[0x01u8]);
    write_header_field(&mut output, 4, &master_seed);
    write_header_field(&mut output, 5, &[]);
    write_header_field(&mut output, 6, &1u64.to_le_bytes());
    write_header_field(&mut output, 7, &encryption_iv);
    write_header_field(&mut output, 8, &[]);
    let stream_start = crypto::generate_random_key(32);
    write_header_field(&mut output, 9, &stream_start);
    write_header_field(&mut output, 10, &[0u8; 4]);
    let kdf_data = build_kdf_variant_map(kdf);
    write_header_field(&mut output, 11, &kdf_data);
    output.push(0); output.push(0); output.push(0);

    // Append encrypted payload (no separate nonce — IV is in header)
    output.extend_from_slice(&encrypted_payload);

    Ok(output)
}

fn encrypt_with_iv(plaintext: &[u8], key: &[u8], iv: &[u8], cipher: Cipher) -> Result<Vec<u8>, String> {
    match cipher {
        Cipher::Aes256 => {
            use aes_gcm::{Aes256Gcm, KeyInit, aead::Aead, Nonce};
            let aead = Aes256Gcm::new_from_slice(key).map_err(|e| format!("AES key: {e}"))?;
            let nonce = Nonce::from_slice(iv);
            aead.encrypt(nonce, plaintext).map_err(|e| format!("AES encrypt: {e}"))
        }
        Cipher::ChaCha20 => {
            use chacha20poly1305::{ChaCha20Poly1305, KeyInit, aead::Aead, Nonce};
            let aead = ChaCha20Poly1305::new_from_slice(key).map_err(|e| format!("ChaCha20 key: {e}"))?;
            let nonce = Nonce::from_slice(iv);
            aead.encrypt(nonce, plaintext).map_err(|e| format!("ChaCha20 encrypt: {e}"))
        }
    }
}

fn write_header_field(output: &mut Vec<u8>, field_type: u8, data: &[u8]) {
    output.push(field_type);
    let size = data.len() as u16;
    output.extend_from_slice(&size.to_le_bytes());
    output.extend_from_slice(data);
}

fn cipher_uuid(cipher: Cipher) -> Vec<u8> {
    match cipher {
        Cipher::Aes256 => vec![0x31,0xC1,0xF2,0xE6,0xBF,0x71,0x43,0x50,0xBE,0x58,0x05,0x21,0x6A,0xFC,0x5A,0xFF],
        Cipher::ChaCha20 => vec![0xD6,0x03,0x8A,0x2B,0x8B,0x6F,0x4C,0xB5,0xA5,0x24,0x33,0x9A,0x31,0xDB,0xB5,0x9A],
    }
}

fn build_kdf_variant_map(kdf: &KdfParams) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&0x0100u16.to_le_bytes());
    let argon2_uuid: [u8; 16] = [0xEF,0x63,0x6D,0xDF,0x8C,0x29,0x44,0x4B,0x91,0xF7,0xA9,0xA4,0x03,0xE3,0x0A,0x0C];
    add_variant_entry(&mut data, b"$UUID", &argon2_uuid);
    add_variant_entry(&mut data, b"M", &kdf.memory_kib.to_le_bytes());
    add_variant_entry(&mut data, b"I", &kdf.iterations.to_le_bytes());
    add_variant_entry(&mut data, b"P", &kdf.parallelism.to_le_bytes());
    add_variant_entry(&mut data, b"V", &0x13u32.to_le_bytes());
    let salt = super::keys::generate_salt(32);
    add_variant_entry(&mut data, b"S", &salt);
    data.push(0x00);
    data
}

fn add_variant_entry(data: &mut Vec<u8>, key: &[u8], value: &[u8]) {
    data.push(0x42);
    data.extend_from_slice(&(key.len() as u32).to_le_bytes());
    data.extend_from_slice(key);
    data.extend_from_slice(&(value.len() as u32).to_le_bytes());
    data.extend_from_slice(value);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader;
    use crate::xml::KeePassFile;

    #[test]
    fn bytes_roundtrip_without_disk() {
        let kf = KeePassFile::new("RoundTrip");
        let kdf = KdfParams::default();
        let bytes =
            write_database_bytes(&kf, "motdepasse", None, Cipher::ChaCha20, &kdf).unwrap();
        assert_eq!(&bytes[0..4], &0x03D9A29Au32.to_le_bytes());
        let result = reader::read_database_bytes(&bytes, "motdepasse", None).unwrap();
        assert_eq!(result.keepass_file.meta.database_name, "RoundTrip");
        assert!(
            reader::read_database_bytes(&bytes, "mauvais", None).is_err(),
            "mauvais mot de passe doit échouer"
        );
    }
}

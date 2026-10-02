/// KDBX file reader. Parses .kdbx files and decrypts the XML payload.
use super::crypto::Cipher;
use super::keys::{KdfParams, derive_composite_key_with_salt};
use super::xml::KeePassFile;
use std::fs;
use std::path::Path;

#[derive(Debug)]
pub struct DatabaseReadResult {
    pub keepass_file: KeePassFile,
    pub cipher: Cipher,
    pub kdf: KdfParams,
    pub salt: Vec<u8>,
}

pub fn read_database(
    path: &Path,
    password: &str,
    keyfile_data: Option<&[u8]>,
) -> Result<DatabaseReadResult, String> {
    let data = fs::read(path).map_err(|e| format!("Failed to read file: {e}"))?;
    read_database_bytes(&data, password, keyfile_data)
}

pub fn read_database_bytes(
    data: &[u8],
    password: &str,
    keyfile_data: Option<&[u8]>,
) -> Result<DatabaseReadResult, String> {
    let (header, encrypted_payload) = parse_header(data)?;

    // Derive composite key using the master seed from the header
    let composite_key = derive_composite_key_with_salt(
        password, keyfile_data, &header.kdf, &header.master_seed,
    )?;

    // Decrypt using the IV from the header as nonce
    let decrypted = decrypt_with_iv(
        &encrypted_payload, &composite_key, &header.encryption_iv, header.cipher,
    )?;

    let xml_str = String::from_utf8(decrypted)
        .map_err(|e| format!("Invalid UTF-8 in decrypted payload: {e}"))?;

    let keepass_file: KeePassFile = quick_xml::de::from_str(&xml_str)
        .map_err(|e| format!("Failed to parse KDBX XML: {e}"))?;

    Ok(DatabaseReadResult { keepass_file, cipher: header.cipher, kdf: header.kdf, salt: header.master_seed })
}

fn decrypt_with_iv(ciphertext: &[u8], key: &[u8], iv: &[u8], cipher: Cipher) -> Result<Vec<u8>, String> {
    match cipher {
        Cipher::Aes256 => {
            use aes_gcm::{Aes256Gcm, KeyInit, aead::Aead, Nonce};
            let aead = Aes256Gcm::new_from_slice(key).map_err(|e| format!("AES key: {e}"))?;
            let nonce = Nonce::from_slice(iv);
            aead.decrypt(nonce, ciphertext).map_err(|e| format!("AES decrypt: {e}"))
        }
        Cipher::ChaCha20 => {
            use chacha20poly1305::{ChaCha20Poly1305, KeyInit, aead::Aead, Nonce};
            let aead = ChaCha20Poly1305::new_from_slice(key).map_err(|e| format!("ChaCha20 key: {e}"))?;
            let nonce = Nonce::from_slice(iv);
            aead.decrypt(nonce, ciphertext).map_err(|e| format!("ChaCha20 decrypt: {e}"))
        }
    }
}

struct ParsedHeader { cipher: Cipher, kdf: KdfParams, master_seed: Vec<u8>, encryption_iv: Vec<u8> }

fn parse_header(data: &[u8]) -> Result<(ParsedHeader, Vec<u8>), String> {
    if data.len() < 12 { return Err("File too small".to_string()); }
    let sig1 = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    let sig2 = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    if sig1 != 0x03D9A29A || sig2 != 0x67FB4BB5 { return Err("Invalid KDBX signature".to_string()); }

    let mut cipher = Cipher::Aes256;
    let mut kdf = KdfParams::default();
    let mut master_seed = Vec::new();
    let mut encryption_iv = Vec::new();

    let mut pos = 12;
    while pos + 3 <= data.len() {
        let field_type = data[pos];
        let field_size = u16::from_le_bytes([data[pos + 1], data[pos + 2]]) as usize;
        pos += 3;
        if field_type == 0 { break; }
        if pos + field_size > data.len() { return Err("Malformed header".to_string()); }
        let fd = &data[pos..pos + field_size];
        match field_type {
            2 => cipher = parse_cipher_id(fd)?,
            4 => master_seed = fd.to_vec(),
            7 => encryption_iv = fd.to_vec(),
            11 => kdf = parse_kdf_params(fd)?,
            _ => {}
        }
        pos += field_size;
    }
    let payload = data[pos..].to_vec();
    Ok((ParsedHeader { cipher, kdf, master_seed, encryption_iv }, payload))
}

fn parse_cipher_id(data: &[u8]) -> Result<Cipher, String> {
    if data.len() >= 16 {
        let aes: [u8; 16] = [0x31,0xC1,0xF2,0xE6,0xBF,0x71,0x43,0x50,0xBE,0x58,0x05,0x21,0x6A,0xFC,0x5A,0xFF];
        let cc: [u8; 16] = [0xD6,0x03,0x8A,0x2B,0x8B,0x6F,0x4C,0xB5,0xA5,0x24,0x33,0x9A,0x31,0xDB,0xB5,0x9A];
        if data[..16] == aes { return Ok(Cipher::Aes256); }
        if data[..16] == cc { return Ok(Cipher::ChaCha20); }
    }
    Ok(Cipher::Aes256)
}

fn parse_kdf_params(_data: &[u8]) -> Result<KdfParams, String> { Ok(KdfParams::default()) }

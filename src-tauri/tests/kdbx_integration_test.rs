//! Integration test: Create a real KDBX file, write entries, read it back.
//! Validates AES-256-GCM and ChaCha20-Poly1305 encryption + Argon2id KDF.

#[test]
fn test_kdbx_create_and_read_aes256() {
    let tmp = std::env::temp_dir().join("mypass_test_aes256.kdbx");
    let _ = std::fs::remove_file(&tmp);

    let password = "Iroise29";
    let kf = mypass_lib::kdbx::xml::KeePassFile::new("Test Vault AES");

    // Write
    mypass_lib::kdbx::writer::write_database(
        &tmp,
        &kf,
        password,
        None,
        mypass_lib::kdbx::crypto::Cipher::Aes256,
        &mypass_lib::kdbx::keys::KdfParams::default(),
    )
    .expect("Failed to write KDBX with AES-256");

    assert!(tmp.exists(), "KDBX file should exist");
    let size = std::fs::metadata(&tmp).unwrap().len();
    println!("✅ AES-256 KDBX created: {} bytes", size);
    assert!(size > 100, "File should be at least 100 bytes");

    // Read back
    let result = mypass_lib::kdbx::reader::read_database(&tmp, password, None)
        .expect("Failed to read KDBX with AES-256");

    assert_eq!(result.keepass_file.meta.database_name, "Test Vault AES");
    println!("✅ AES-256 KDBX read successfully: {:?}", result.cipher);

    // Verify we can add an entry
    let entry = mypass_lib::kdbx::xml::Entry::new("Google", "user", "pass123", "https://google.com");
    assert_eq!(entry.title(), "Google");
    assert_eq!(entry.username(), "user");
    println!("✅ Entry created and fields extracted");

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn test_kdbx_create_and_read_chacha20() {
    let tmp = std::env::temp_dir().join("mypass_test_chacha20.kdbx");
    let _ = std::fs::remove_file(&tmp);

    let password = "Iroise29";
    let kf = mypass_lib::kdbx::xml::KeePassFile::new("Test Vault ChaCha20");

    mypass_lib::kdbx::writer::write_database(
        &tmp,
        &kf,
        password,
        None,
        mypass_lib::kdbx::crypto::Cipher::ChaCha20,
        &mypass_lib::kdbx::keys::KdfParams::default(),
    )
    .expect("Failed to write KDBX with ChaCha20");

    assert!(tmp.exists());
    println!("✅ ChaCha20 KDBX created: {} bytes", std::fs::metadata(&tmp).unwrap().len());

    let result = mypass_lib::kdbx::reader::read_database(&tmp, password, None)
        .expect("Failed to read KDBX with ChaCha20");

    assert_eq!(result.keepass_file.meta.database_name, "Test Vault ChaCha20");
    println!("✅ ChaCha20 KDBX read successfully");

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn test_kdbx_wrong_password_fails() {
    let tmp = std::env::temp_dir().join("mypass_test_wrongpass.kdbx");
    let _ = std::fs::remove_file(&tmp);

    let kf = mypass_lib::kdbx::xml::KeePassFile::new("Password Test");

    mypass_lib::kdbx::writer::write_database(
        &tmp,
        &kf,
        "correct-password",
        None,
        mypass_lib::kdbx::crypto::Cipher::Aes256,
        &mypass_lib::kdbx::keys::KdfParams::default(),
    )
    .expect("Write should succeed");

    // Try reading with wrong password
    let result = mypass_lib::kdbx::reader::read_database(&tmp, "wrong-password", None);
    assert!(result.is_err(), "Should fail with wrong password");
    println!("✅ Wrong password correctly rejected: {}", result.unwrap_err());

    // Try reading with correct password
    let result = mypass_lib::kdbx::reader::read_database(&tmp, "correct-password", None);
    assert!(result.is_ok(), "Should succeed with correct password");
    println!("✅ Correct password accepted");

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn test_kdbx_with_entries() {
    let tmp = std::env::temp_dir().join("mypass_test_entries.kdbx");
    let _ = std::fs::remove_file(&tmp);

    let mut kf = mypass_lib::kdbx::xml::KeePassFile::new("Entries Test");

    // Add entries
    kf.root.group.entries.push(mypass_lib::kdbx::xml::Entry::new(
        "Google", "user@gmail.com", "strong-pass-1", "https://google.com",
    ));
    kf.root.group.entries.push(mypass_lib::kdbx::xml::Entry::new(
        "GitHub", "dev", "strong-pass-2", "https://github.com",
    ));
    kf.root.group.entries.push(mypass_lib::kdbx::xml::Entry::new(
        "Amazon", "shopper", "strong-pass-3", "https://amazon.com",
    ));

    // Write
    mypass_lib::kdbx::writer::write_database(
        &tmp, &kf, "Iroise29", None,
        mypass_lib::kdbx::crypto::Cipher::Aes256,
        &mypass_lib::kdbx::keys::KdfParams::default(),
    )
    .expect("Write with entries should succeed");

    // Read back
    let result = mypass_lib::kdbx::reader::read_database(&tmp, "Iroise29", None)
        .expect("Read with entries should succeed");

    let entries = &result.keepass_file.root.group.entries;
    assert_eq!(entries.len(), 3, "Should have 3 entries");
    assert_eq!(entries[0].title(), "Google");
    assert_eq!(entries[1].title(), "GitHub");
    assert_eq!(entries[2].title(), "Amazon");
    assert_eq!(entries[0].password(), "strong-pass-1");
    println!("✅ 3 entries written and read back: Google, GitHub, Amazon");

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn test_key_derivation_argon2() {
    let password = "Iroise29";
    let salt = mypass_lib::kdbx::keys::generate_salt(32);
    let kdf = mypass_lib::kdbx::keys::KdfParams::default();

    let key = mypass_lib::kdbx::keys::derive_master_key(password, &salt, &kdf)
        .expect("Argon2 key derivation should succeed");
    assert_eq!(key.len(), 32, "Derived key should be 32 bytes");
    println!("✅ Argon2id key derived: {} bytes", key.len());

    // Verify deterministic
    let key2 = mypass_lib::kdbx::keys::derive_master_key(password, &salt, &kdf)
        .expect("Second derivation should succeed");
    assert_eq!(key, key2, "Same password + salt should produce same key");
    println!("✅ Key derivation is deterministic");

    // Verify different password produces different key
    let key3 = mypass_lib::kdbx::keys::derive_master_key("wrong-password", &salt, &kdf)
        .expect("Third derivation should succeed");
    assert_ne!(key, key3, "Different password should produce different key");
    println!("✅ Different passwords produce different keys");
}

#[test]
fn test_crypto_roundtrip_aes() {
    use mypass_lib::kdbx::crypto;
    let key = crypto::generate_random_key(32);
    let plaintext = b"Iroise29 test data for AES-256-GCM encryption";

    let (nonce, ciphertext) = crypto::encrypt(plaintext, &key, crypto::Cipher::Aes256)
        .expect("AES encryption should succeed");
    let decrypted = crypto::decrypt(&ciphertext, &key, &nonce, crypto::Cipher::Aes256)
        .expect("AES decryption should succeed");

    assert_eq!(decrypted, plaintext);
    println!("✅ AES-256-GCM encrypt/decrypt roundtrip OK");
}

#[test]
fn test_crypto_roundtrip_chacha20() {
    use mypass_lib::kdbx::crypto;
    let key = crypto::generate_random_key(32);
    let plaintext = b"Iroise29 test data for ChaCha20-Poly1305 encryption";

    let (nonce, ciphertext) = crypto::encrypt(plaintext, &key, crypto::Cipher::ChaCha20)
        .expect("ChaCha20 encryption should succeed");
    let decrypted = crypto::decrypt(&ciphertext, &key, &nonce, crypto::Cipher::ChaCha20)
        .expect("ChaCha20 decryption should succeed");

    assert_eq!(decrypted, plaintext);
    println!("✅ ChaCha20-Poly1305 encrypt/decrypt roundtrip OK");
}

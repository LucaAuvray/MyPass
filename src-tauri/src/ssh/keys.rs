/// Parsing, génération et export de clés SSH au format OpenSSH.
use ssh_key::{Algorithm, HashAlg, LineEnding, PrivateKey};

#[derive(Clone, Debug)]
pub struct ParsedSshKey {
    /// PEM OpenSSH **déchiffré** — c'est le coffre KDBX qui protège ensuite.
    pub private_key_openssh: String,
    /// Ligne publique complète : "ssh-ed25519 AAAA... commentaire".
    pub public_key_openssh: String,
    /// "SHA256:..." (fingerprint de la clé publique).
    pub fingerprint: String,
    /// Nom d'algorithme SSH : "ssh-ed25519", "ssh-rsa", ...
    pub algorithm: String,
    pub comment: String,
}

/// Parse une clé privée OpenSSH. Erreurs sentinelles pour l'UI :
/// "PASSPHRASE_REQUIRED" si la clé est chiffrée et qu'aucune passphrase n'est
/// fournie, "PASSPHRASE_INVALID" si la passphrase ne déchiffre pas.
pub fn parse_private_key(content: &str, passphrase: Option<&str>) -> Result<ParsedSshKey, String> {
    let key = PrivateKey::from_openssh(content).map_err(|e| format!("Invalid key: {e}"))?;
    let key = if key.is_encrypted() {
        let pass = passphrase.ok_or("PASSPHRASE_REQUIRED")?;
        key.decrypt(pass.as_bytes()).map_err(|_| "PASSPHRASE_INVALID".to_string())?
    } else {
        key
    };
    to_parsed(&key)
}

pub fn generate_ed25519(comment: &str) -> Result<ParsedSshKey, String> {
    let mut key = PrivateKey::random(&mut ssh_key::rand_core::OsRng, Algorithm::Ed25519)
        .map_err(|e| e.to_string())?;
    key.set_comment(comment);
    to_parsed(&key)
}

fn to_parsed(key: &PrivateKey) -> Result<ParsedSshKey, String> {
    Ok(ParsedSshKey {
        private_key_openssh: key
            .to_openssh(LineEnding::LF)
            .map_err(|e| e.to_string())?
            .to_string(),
        public_key_openssh: key.public_key().to_openssh().map_err(|e| e.to_string())?,
        fingerprint: key.public_key().fingerprint(HashAlg::Sha256).to_string(),
        algorithm: key.algorithm().to_string(),
        comment: key.comment().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ssh_key::{LineEnding, PrivateKey};

    #[test]
    fn generate_then_parse_round_trip() {
        let generated = generate_ed25519("luca@mypass").unwrap();
        assert!(generated.private_key_openssh.starts_with("-----BEGIN OPENSSH PRIVATE KEY-----"));
        assert!(generated.public_key_openssh.starts_with("ssh-ed25519 "));
        assert!(generated.fingerprint.starts_with("SHA256:"));
        assert_eq!(generated.algorithm, "ssh-ed25519");

        let parsed = parse_private_key(&generated.private_key_openssh, None).unwrap();
        assert_eq!(parsed.fingerprint, generated.fingerprint);
        assert_eq!(parsed.public_key_openssh, generated.public_key_openssh);
    }

    #[test]
    fn encrypted_key_requires_and_checks_passphrase() {
        let generated = generate_ed25519("test@mypass").unwrap();
        let key = PrivateKey::from_openssh(&generated.private_key_openssh).unwrap();
        let encrypted = key.encrypt(&mut ssh_key::rand_core::OsRng, "s3cret").unwrap();
        let pem = encrypted.to_openssh(LineEnding::LF).unwrap();

        assert_eq!(parse_private_key(&pem, None).unwrap_err(), "PASSPHRASE_REQUIRED");
        assert_eq!(parse_private_key(&pem, Some("wrong")).unwrap_err(), "PASSPHRASE_INVALID");
        let ok = parse_private_key(&pem, Some("s3cret")).unwrap();
        assert_eq!(ok.fingerprint, generated.fingerprint);
        // La clé stockée dans le coffre est TOUJOURS la version déchiffrée.
        assert!(ok.private_key_openssh.starts_with("-----BEGIN OPENSSH PRIVATE KEY-----"));
        let reparsed = PrivateKey::from_openssh(&ok.private_key_openssh).unwrap();
        assert!(!reparsed.is_encrypted());
    }

    #[test]
    fn sign_verify_round_trip() {
        use signature::{Signer, Verifier};
        let generated = generate_ed25519("t").unwrap();
        let key = PrivateKey::from_openssh(&generated.private_key_openssh).unwrap();
        let sig: ssh_key::Signature = key.try_sign(b"challenge-bytes").unwrap();
        // `PublicKey` also has an inherent `verify(namespace, msg, &SshSig)` for the
        // SSHSIG file format, which shadows the `Verifier` trait method during normal
        // resolution — call the trait method explicitly to disambiguate.
        Verifier::verify(key.public_key(), b"challenge-bytes", &sig).unwrap();
    }

    #[test]
    fn garbage_input_is_a_clean_error() {
        assert!(parse_private_key("pas une clé", None).is_err());
    }
}

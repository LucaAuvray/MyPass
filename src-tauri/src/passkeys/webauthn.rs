//! WebAuthn building blocks: keys, authenticator data, CBOR, signatures.
//! Pure functions, no vault access. Mirrors KeePassXC 2.7.10
//! (`src/browser/BrowserPasskeys.cpp`), ES256 only.

use base64::Engine;
use p256::ecdsa::{signature::Signer, Signature, SigningKey};
use p256::elliptic_curve::sec1::ToEncodedPoint;
use p256::pkcs8::{DecodePrivateKey, EncodePrivateKey, EncodePublicKey, LineEnding};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

/// COSE algorithm id of ECDSA P-256 / SHA-256.
pub const ES256: i64 = -7;

const FLAG_UP: u8 = 0x01;
const FLAG_UV: u8 = 0x04;
const FLAG_AT: u8 = 0x40;

pub fn b64url(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// Decodes base64url, also accepting padding and the standard alphabet
/// (credential ids of entries imported from KeePassXC).
pub fn from_b64url(s: &str) -> Option<Vec<u8>> {
    let normalized: String = s
        .trim_end_matches('=')
        .chars()
        .map(|c| match c {
            '+' => '-',
            '/' => '_',
            c => c,
        })
        .collect();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(normalized).ok()
}

/// `kind` is `webauthn.create` or `webauthn.get`. `crossOrigin` is always
/// false: the extension already refuses requests from cross-origin frames.
pub fn client_data_json(kind: &str, challenge: &str, origin: &str) -> String {
    serde_json::json!({ "type": kind, "challenge": challenge, "origin": origin, "crossOrigin": false })
        .to_string()
}

pub struct NewCredential {
    pub id: Vec<u8>,
    pub private_key_pem: Zeroizing<String>,
    pub cose_key: Vec<u8>,
    pub spki_der: Vec<u8>,
}

/// A fresh P-256 key with a random 32-byte credential id.
pub fn new_credential() -> NewCredential {
    use rand::RngCore;
    let mut id = vec![0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut id);

    let secret = p256::SecretKey::random(&mut rand::rngs::OsRng);
    let public = secret.public_key();
    let point = public.to_encoded_point(false);
    // COSE_Key {1: 2 (EC2), 3: -7 (ES256), -1: 1 (P-256), -2: x, -3: y}, canonical order.
    let mut cose_key = vec![0xA5, 0x01, 0x02, 0x03, 0x26, 0x20, 0x01, 0x21, 0x58, 0x20];
    cose_key.extend_from_slice(point.x().expect("uncompressed point has x"));
    cose_key.extend_from_slice(&[0x22, 0x58, 0x20]);
    cose_key.extend_from_slice(point.y().expect("uncompressed point has y"));

    NewCredential {
        id,
        private_key_pem: secret.to_pkcs8_pem(LineEnding::LF).expect("P-256 key encodes to PKCS#8"),
        cose_key,
        spki_der: public.to_public_key_der().expect("P-256 key encodes to SPKI").as_bytes().to_vec(),
    }
}

/// rpIdHash | flags | counter (always 0, like KeePassXC) | attested credential
/// data (registration only: zero AAGUID, id length, id, COSE key).
pub fn authenticator_data(rp_id: &str, attested: Option<(&[u8], &[u8])>) -> Vec<u8> {
    let mut data = Sha256::digest(rp_id.as_bytes()).to_vec();
    data.push(FLAG_UP | FLAG_UV | if attested.is_some() { FLAG_AT } else { 0 });
    data.extend_from_slice(&[0; 4]);
    if let Some((credential_id, cose_key)) = attested {
        data.extend_from_slice(&[0; 16]);
        data.extend_from_slice(&(credential_id.len() as u16).to_be_bytes());
        data.extend_from_slice(credential_id);
        data.extend_from_slice(cose_key);
    }
    data
}

/// CBOR `{"fmt": "none", "attStmt": {}, "authData": <bytes>}`, canonical key order.
pub fn attestation_object_none(auth_data: &[u8]) -> Vec<u8> {
    let mut out = vec![0xA3];
    cbor_text(&mut out, "fmt");
    cbor_text(&mut out, "none");
    cbor_text(&mut out, "attStmt");
    out.push(0xA0);
    cbor_text(&mut out, "authData");
    cbor_head(&mut out, 0x40, auth_data.len());
    out.extend_from_slice(auth_data);
    out
}

/// ECDSA P-256 / SHA-256 signature (DER) over `auth_data || SHA-256(client_data_json)`.
pub fn sign(private_key_pem: &str, auth_data: &[u8], client_data_json: &str) -> Result<Vec<u8>, String> {
    let key = SigningKey::from_pkcs8_pem(private_key_pem).map_err(|e| format!("Invalid passkey private key: {e}"))?;
    let message = [auth_data, Sha256::digest(client_data_json.as_bytes()).as_slice()].concat();
    let signature: Signature = key.sign(&message);
    Ok(signature.to_der().as_bytes().to_vec())
}

/// CBOR head for lengths up to u16 (our maps and authData are far smaller).
fn cbor_head(out: &mut Vec<u8>, major: u8, len: usize) {
    match len {
        0..=23 => out.push(major | len as u8),
        24..=255 => out.extend_from_slice(&[major | 24, len as u8]),
        _ => {
            out.push(major | 25);
            out.extend_from_slice(&(len as u16).to_be_bytes());
        }
    }
}

fn cbor_text(out: &mut Vec<u8>, s: &str) {
    cbor_head(out, 0x60, s.len());
    out.extend_from_slice(s.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use p256::ecdsa::{signature::Verifier, VerifyingKey};
    use p256::pkcs8::DecodePublicKey;

    #[test]
    fn b64url_roundtrip_accepts_padding_and_std_alphabet() {
        assert_eq!(b64url(&[0xfb, 0xff]), "-_8");
        assert_eq!(from_b64url("-_8").unwrap(), vec![0xfb, 0xff]);
        assert_eq!(from_b64url("+/8=").unwrap(), vec![0xfb, 0xff]);
        assert!(from_b64url("%%").is_none());
    }

    #[test]
    fn client_data_json_fields() {
        let v: serde_json::Value =
            serde_json::from_str(&client_data_json("webauthn.create", "abc", "https://x.com")).unwrap();
        assert_eq!(
            v,
            serde_json::json!({"type": "webauthn.create", "challenge": "abc",
                "origin": "https://x.com", "crossOrigin": false})
        );
    }

    #[test]
    fn authenticator_data_layout() {
        let get = authenticator_data("binance.com", None);
        assert_eq!(get.len(), 37);
        assert_eq!(&get[..32], Sha256::digest(b"binance.com").as_slice());
        assert_eq!(get[32], 0x05);
        assert_eq!(&get[33..37], &[0, 0, 0, 0]);

        let cred = new_credential();
        let reg = authenticator_data("binance.com", Some((&cred.id, &cred.cose_key)));
        assert_eq!(reg[32], 0x45);
        assert_eq!(&reg[37..53], &[0u8; 16]);
        assert_eq!(&reg[53..55], &[0x00, 0x20]);
        assert_eq!(&reg[55..87], cred.id.as_slice());
        assert_eq!(&reg[87..], cred.cose_key.as_slice());
    }

    #[test]
    fn attestation_object_is_canonical_none() {
        assert_eq!(
            attestation_object_none(&[0xAA; 3]),
            [
                0xA3, 0x63, b'f', b'm', b't', 0x64, b'n', b'o', b'n', b'e', 0x67, b'a', b't', b't', b'S', b't',
                b'm', b't', 0xA0, 0x68, b'a', b'u', b't', b'h', b'D', b'a', b't', b'a', 0x43, 0xAA, 0xAA, 0xAA
            ]
        );
        assert_eq!(&attestation_object_none(&[0u8; 164])[28..30], &[0x58, 164]);
    }

    #[test]
    fn cose_key_matches_spki_point() {
        let cred = new_credential();
        assert_eq!(cred.id.len(), 32);
        assert!(cred.private_key_pem.starts_with("-----BEGIN PRIVATE KEY-----"));
        assert_eq!(cred.cose_key.len(), 77);
        assert_eq!(&cred.cose_key[..10], &[0xA5, 0x01, 0x02, 0x03, 0x26, 0x20, 0x01, 0x21, 0x58, 0x20]);
        assert_eq!(&cred.cose_key[42..45], &[0x22, 0x58, 0x20]);
        let point = VerifyingKey::from_public_key_der(&cred.spki_der).unwrap().to_encoded_point(false);
        assert_eq!(&cred.cose_key[10..42], &point.as_bytes()[1..33]);
        assert_eq!(&cred.cose_key[45..77], &point.as_bytes()[33..65]);
    }

    #[test]
    fn signature_verifies_over_auth_data_and_client_data_hash() {
        let cred = new_credential();
        let auth = authenticator_data("binance.com", None);
        let cdj = client_data_json("webauthn.get", "Y2hhbGxlbmdl", "https://accounts.binance.com");
        let sig = sign(&cred.private_key_pem, &auth, &cdj).unwrap();
        let vk = VerifyingKey::from_public_key_der(&cred.spki_der).unwrap();
        let msg = [auth.as_slice(), Sha256::digest(cdj.as_bytes()).as_slice()].concat();
        vk.verify(&msg, &Signature::from_der(&sig).unwrap()).unwrap();
    }
}

//! Passkeys (WebAuthn ES256) served to the browser extension through the
//! `passkeys-register` / `passkeys-get` actions, stored in the vault in
//! KeePassXC's `KPEX_PASSKEY_*` fields.

pub mod confirm;
pub mod options;
pub mod vault;
pub mod webauthn;

use crate::commands::database::DbState;
use options::{CREDENTIAL_IS_EXCLUDED, UNKNOWN_ERROR};
use serde_json::{json, Value};
use std::sync::Mutex;
use std::time::Duration;
use webauthn::{b64url, ES256};

/// What the extension receives.
pub enum Outcome {
    /// The `PublicKeyCredential` JSON KeePassXC returns.
    Credential(Value),
    /// A code from `options` (the extension shows it, then falls back).
    Error(u8),
    /// No reply: the extension silently hands over to the browser (Windows).
    Fallback,
}

/// Payload of the in-app confirmation (never carries a key).
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Prompt {
    /// "register" or "get"
    pub kind: &'static str,
    pub rp_id: String,
    pub accounts: Vec<Account>,
    pub replaces: bool,
    #[serde(skip)]
    pub timeout: Duration,
}

#[derive(serde::Serialize, Clone)]
pub struct Account {
    /// Entry uuid, or "new" for a registration into a new entry.
    pub id: String,
    pub username: String,
    pub title: String,
}

/// `navigator.credentials.create`: asks the user, then stores a new passkey.
/// The vault lock is never held while `confirm` waits for the user.
pub fn register(
    db: &Mutex<DbState>,
    options: &Value,
    origin: &str,
    confirm: impl FnOnce(&Prompt) -> Option<String>,
) -> Outcome {
    let req = match options::parse_register(options, origin) {
        Ok(req) => req,
        Err(code) => return Outcome::Error(code),
    };

    let prompt = {
        let Ok(guard) = db.lock() else { return Outcome::Error(UNKNOWN_ERROR) };
        let Some(kf) = guard.keepass_file.as_ref() else { return Outcome::Fallback };
        if vault::find_for_rp(kf, &req.rp_id).iter().any(|p| req.exclude.contains(&p.credential_id)) {
            return Outcome::Error(CREDENTIAL_IS_EXCLUDED);
        }
        let existing = vault::existing_for_user(kf, &req.rp_id, &req.user_id);
        Prompt {
            kind: "register",
            rp_id: req.rp_id.clone(),
            accounts: vec![Account {
                id: existing.clone().unwrap_or_else(|| "new".to_string()),
                username: req.user_name.clone(),
                title: req.rp_name.clone(),
            }],
            replaces: existing.is_some(),
            timeout: req.timeout,
        }
    };
    if confirm(&prompt).is_none() {
        return Outcome::Fallback;
    }

    let cred = webauthn::new_credential();
    let auth_data = webauthn::authenticator_data(&req.rp_id, Some((&cred.id, &cred.cose_key)));
    let client_data = webauthn::client_data_json("webauthn.create", &req.challenge, origin);
    let id = b64url(&cred.id);
    {
        let Ok(mut guard) = db.lock() else { return Outcome::Error(UNKNOWN_ERROR) };
        // Locked (auto-lock) while the user was deciding.
        let Some(kf) = guard.keepass_file.as_mut() else { return Outcome::Error(UNKNOWN_ERROR) };
        let before = kf.clone();
        vault::upsert(
            kf,
            &vault::NewPasskey {
                rp_id: &req.rp_id,
                rp_name: &req.rp_name,
                origin,
                username: &req.user_name,
                credential_id: &id,
                user_handle: &req.user_id,
                private_key_pem: &cred.private_key_pem,
            },
        );
        // Never give the site a key the vault failed to keep, and never lose
        // the key the site still knows (a replaced one) to a later save.
        if guard.save().is_err() {
            guard.keepass_file = Some(before);
            return Outcome::Error(UNKNOWN_ERROR);
        }
    }
    crate::native_messaging::notify_vault_changed();

    Outcome::Credential(json!({
        "authenticatorAttachment": "platform",
        "id": id,
        "type": "public-key",
        "response": {
            "attestationObject": b64url(&webauthn::attestation_object_none(&auth_data)),
            "clientDataJSON": b64url(client_data.as_bytes()),
            "authenticatorData": b64url(&auth_data),
            "publicKeyAlgorithm": ES256,
            "publicKey": b64url(&cred.spki_der),
            "clientExtensionResults": {},
        },
    }))
}

/// `navigator.credentials.get`: asks which account to use, then signs.
/// The private key is read only after the user confirmed.
pub fn get(
    db: &Mutex<DbState>,
    options: &Value,
    origin: &str,
    confirm: impl FnOnce(&Prompt) -> Option<String>,
) -> Outcome {
    let req = match options::parse_get(options, origin) {
        Ok(req) => req,
        Err(code) => return Outcome::Error(code),
    };

    let candidates: Vec<vault::StoredPasskey> = {
        let Ok(guard) = db.lock() else { return Outcome::Error(UNKNOWN_ERROR) };
        let Some(kf) = guard.keepass_file.as_ref() else { return Outcome::Fallback };
        vault::find_for_rp(kf, &req.rp_id)
            .into_iter()
            .filter(|p| {
                if req.allow.is_empty() {
                    !p.user_handle.is_empty()
                } else {
                    req.allow.contains(&p.credential_id)
                }
            })
            .collect()
    };
    if candidates.is_empty() {
        return Outcome::Fallback;
    }

    let prompt = Prompt {
        kind: "get",
        rp_id: req.rp_id.clone(),
        accounts: candidates
            .iter()
            .map(|p| Account { id: p.entry_uuid.clone(), username: p.username.clone(), title: p.title.clone() })
            .collect(),
        replaces: false,
        timeout: req.timeout,
    };
    let Some(choice) = confirm(&prompt) else { return Outcome::Fallback };
    let Some(chosen) = candidates.iter().find(|p| p.entry_uuid == choice) else { return Outcome::Fallback };

    let pem = {
        let Ok(guard) = db.lock() else { return Outcome::Error(UNKNOWN_ERROR) };
        guard.keepass_file.as_ref().and_then(|kf| vault::private_key_pem(kf, &chosen.entry_uuid, &req.rp_id))
    };
    // Vault locked or entry gone (sync, deletion) while the user was deciding.
    let Some(pem) = pem else { return Outcome::Error(UNKNOWN_ERROR) };

    let auth_data = webauthn::authenticator_data(&req.rp_id, None);
    let client_data = webauthn::client_data_json("webauthn.get", &req.challenge, origin);
    let Ok(signature) = webauthn::sign(&pem, &auth_data, &client_data) else { return Outcome::Error(UNKNOWN_ERROR) };

    Outcome::Credential(json!({
        "authenticatorAttachment": "platform",
        "id": b64url(&chosen.credential_id),
        "type": "public-key",
        "response": {
            "authenticatorData": b64url(&auth_data),
            "clientDataJSON": b64url(client_data.as_bytes()),
            "signature": b64url(&signature),
            "userHandle": chosen.user_handle,
            "clientExtensionResults": {},
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::webauthn::from_b64url;
    use super::*;
    use crate::commands::database::DbState;
    use crate::kdbx::keys::KdfParams;
    use crate::kdbx::xml::KeePassFile;
    use p256::ecdsa::{signature::Verifier, Signature, VerifyingKey};
    use p256::pkcs8::DecodePublicKey;
    use serde_json::{json, Value};
    use sha2::{Digest, Sha256};
    use std::sync::Mutex;

    fn open_db() -> Mutex<DbState> {
        let path = std::env::temp_dir().join(format!("mypass-passkeys-{}.kdbx", uuid::Uuid::new_v4()));
        Mutex::new(DbState {
            is_open: true,
            file_path: Some(path),
            keepass_file: Some(KeePassFile::new("t")),
            master_password: Some(zeroize::Zeroizing::new("pw".to_string())),
            kdf: KdfParams { memory_kib: 8, iterations: 1, parallelism: 1, output_len: 32 },
            ..Default::default()
        })
    }

    fn reg_opts() -> Value {
        json!({
            "rp": {"id": "binance.com", "name": "Binance"},
            "user": {"id": "dXNlcjE", "name": "luca@example.com", "displayName": "Luca"},
            "challenge": "Y2hhbGxlbmdl",
            "pubKeyCredParams": [{"type": "public-key", "alg": -7}],
            "attestation": "none",
            "timeout": 60000
        })
    }

    fn b64(v: &Value) -> Vec<u8> {
        from_b64url(v.as_str().expect("base64url string")).expect("valid base64url")
    }

    #[test]
    fn register_then_get_roundtrip() {
        let db = open_db();
        let out = register(&db, &reg_opts(), "https://accounts.binance.com", |p| {
            assert_eq!((p.kind, p.rp_id.as_str(), p.replaces), ("register", "binance.com", false));
            assert_eq!(p.accounts[0].id, "new");
            Some(p.accounts[0].id.clone())
        });
        let Outcome::Credential(c) = out else { panic!("no credential") };
        let id = b64(&c["id"]);
        let ao = b64(&c["response"]["attestationObject"]);
        assert_eq!(&ao[30 + 55..30 + 87], id.as_slice()); // authData starts at byte 30
        assert_eq!(c["response"]["publicKeyAlgorithm"], -7);
        let cdj: Value = serde_json::from_slice(&b64(&c["response"]["clientDataJSON"])).unwrap();
        assert_eq!(
            (cdj["type"].as_str(), cdj["challenge"].as_str(), cdj["origin"].as_str()),
            (Some("webauthn.create"), Some("Y2hhbGxlbmdl"), Some("https://accounts.binance.com"))
        );
        assert!(db.lock().unwrap().file_path.as_ref().unwrap().exists(), "vault saved");

        let opts = json!({"rpId": "binance.com", "challenge": "Z2V0",
                          "allowCredentials": [{"id": c["id"], "type": "public-key"}]});
        let out = get(&db, &opts, "https://www.binance.com", |p| {
            assert_eq!((p.kind, p.accounts.len()), ("get", 1));
            assert_eq!(p.accounts[0].username, "luca@example.com");
            Some(p.accounts[0].id.clone())
        });
        let Outcome::Credential(a) = out else { panic!("no assertion") };
        assert_eq!(a["id"], c["id"]);
        assert_eq!(a["response"]["userHandle"], "dXNlcjE");
        let spki = b64(&c["response"]["publicKey"]);
        let auth = b64(&a["response"]["authenticatorData"]);
        let cdj = b64(&a["response"]["clientDataJSON"]);
        let sig = b64(&a["response"]["signature"]);
        let msg = [auth.as_slice(), Sha256::digest(&cdj).as_slice()].concat();
        VerifyingKey::from_public_key_der(&spki).unwrap().verify(&msg, &Signature::from_der(&sig).unwrap()).unwrap();
    }

    #[test]
    fn cancel_or_unknown_site_falls_back_silently() {
        let db = open_db();
        assert!(matches!(register(&db, &reg_opts(), "https://accounts.binance.com", |_| None), Outcome::Fallback));
        let opts = json!({"rpId": "paypal.com", "challenge": "Z2V0"});
        assert!(matches!(get(&db, &opts, "https://www.paypal.com", |_| panic!("asked")), Outcome::Fallback));
    }

    #[test]
    fn excluded_credential_is_reported() {
        let db = open_db();
        let Outcome::Credential(c) = register(&db, &reg_opts(), "https://accounts.binance.com", |_| Some("new".into()))
        else {
            panic!("no credential")
        };
        let mut o = reg_opts();
        o["excludeCredentials"] = json!([{"id": c["id"], "type": "public-key"}]);
        assert!(matches!(register(&db, &o, "https://accounts.binance.com", |_| panic!("asked")), Outcome::Error(21)));
    }

    #[test]
    fn vault_locked_during_confirmation_returns_no_credential() {
        let db = open_db();
        let out = register(&db, &reg_opts(), "https://accounts.binance.com", |_| {
            db.try_lock().expect("DbState mutex held during confirmation").lock_vault();
            Some("new".into())
        });
        assert!(matches!(out, Outcome::Error(31)));
    }

    #[test]
    fn entry_deleted_during_confirmation_returns_no_credential() {
        let db = open_db();
        register(&db, &reg_opts(), "https://accounts.binance.com", |_| Some("new".into()));
        let out = get(&db, &json!({"rpId": "binance.com", "challenge": "Z2V0"}), "https://www.binance.com", |p| {
            db.try_lock().expect("mutex held").keepass_file.as_mut().unwrap().root.group.entries.clear();
            Some(p.accounts[0].id.clone())
        });
        assert!(matches!(out, Outcome::Error(31)));
    }

    #[test]
    fn failed_save_keeps_the_previous_passkey() {
        let db = open_db();
        let Outcome::Credential(first) = register(&db, &reg_opts(), "https://accounts.binance.com", |_| Some("new".into()))
        else {
            panic!("no credential")
        };
        // A directory as vault path: the write fails.
        db.lock().unwrap().file_path = Some(std::env::temp_dir());
        let out = register(&db, &reg_opts(), "https://accounts.binance.com", |p| {
            assert!(p.replaces);
            Some(p.accounts[0].id.clone())
        });
        assert!(matches!(out, Outcome::Error(31)));
        let guard = db.lock().unwrap();
        let kept = vault::find_for_rp(guard.keepass_file.as_ref().unwrap(), "binance.com");
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].credential_id, b64(&first["id"]));
    }

    #[test]
    fn get_without_allow_list_offers_every_account_of_the_site() {
        let db = open_db();
        register(&db, &reg_opts(), "https://accounts.binance.com", |_| Some("new".into()));
        let mut o = reg_opts();
        o["user"] = json!({"id": "dXNlcjI", "name": "pro@example.com"});
        register(&db, &o, "https://accounts.binance.com", |_| Some("new".into()));
        let out = get(&db, &json!({"rpId": "binance.com", "challenge": "Z2V0"}), "https://www.binance.com", |p| {
            assert_eq!(p.accounts.len(), 2);
            p.accounts.iter().find(|a| a.username == "pro@example.com").map(|a| a.id.clone())
        });
        let Outcome::Credential(a) = out else { panic!("no assertion") };
        assert_eq!(a["response"]["userHandle"], "dXNlcjI");
    }
}

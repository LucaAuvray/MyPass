//! Bindings wasm du coffre MyPass. Une seule session en mémoire (wasm est
//! mono-thread) ; le mode « web » de src/lib/tauri.ts appelle ces fonctions
//! comme il invoquerait les commandes Tauri. Entrées/sorties structurées en
//! JSON (String) — les DTOs serde de mypass_core::ops sont réutilisés tels
//! quels. Erreurs : Err(String) → exception JS via wasm-bindgen.
use mypass_core::crypto::Cipher;
use mypass_core::keys::KdfParams;
use mypass_core::xml::KeePassFile;
use std::cell::RefCell;
use wasm_bindgen::prelude::*;

/// Miroir du DbState desktop : le coffre déchiffré + ce qu'il faut pour le
/// re-chiffrer à la sauvegarde. Le mot de passe reste en mémoire de page,
/// comme le desktop le garde en mémoire de process.
struct Session {
    kf: KeePassFile,
    password: String,
    keyfile: Option<Vec<u8>>,
    cipher: Cipher,
    kdf: KdfParams,
}

thread_local! {
    static SESSION: RefCell<Option<Session>> = const { RefCell::new(None) };
}

fn with_kf<T>(f: impl FnOnce(&KeePassFile) -> Result<T, String>) -> Result<T, String> {
    SESSION.with(|s| {
        let s = s.borrow();
        let session = s.as_ref().ok_or("No open vault")?;
        f(&session.kf)
    })
}

fn with_kf_mut<T>(f: impl FnOnce(&mut KeePassFile) -> Result<T, String>) -> Result<T, String> {
    SESSION.with(|s| {
        let mut s = s.borrow_mut();
        let session = s.as_mut().ok_or("No open vault")?;
        f(&mut session.kf)
    })
}

fn json<T: serde::Serialize>(v: &T) -> Result<String, String> {
    serde_json::to_string(v).map_err(|e| e.to_string())
}

/// Crée un coffre vierge et retourne ses octets KDBX (mêmes défauts que la
/// commande desktop create_database : AES-256, KdfParams::default()).
#[wasm_bindgen]
pub fn create_vault(name: &str, password: &str) -> Result<Vec<u8>, String> {
    let kf = KeePassFile::new(name);
    mypass_core::writer::write_database_bytes(&kf, password, None, Cipher::Aes256, &KdfParams::default())
}

#[wasm_bindgen]
pub fn open_vault(data: &[u8], password: &str, keyfile: Option<Vec<u8>>) -> Result<(), String> {
    let result = mypass_core::reader::read_database_bytes(data, password, keyfile.as_deref())?;
    SESSION.with(|s| {
        *s.borrow_mut() = Some(Session {
            kf: result.keepass_file,
            password: password.to_string(),
            keyfile,
            cipher: result.cipher,
            kdf: result.kdf,
        });
    });
    Ok(())
}

#[wasm_bindgen]
pub fn close_vault() {
    SESSION.with(|s| *s.borrow_mut() = None);
}

#[wasm_bindgen]
pub fn is_unlocked() -> bool {
    SESSION.with(|s| s.borrow().is_some())
}

/// Re-chiffre l'état courant et retourne les octets KDBX (le JS fait le PUT).
#[wasm_bindgen]
pub fn save_vault() -> Result<Vec<u8>, String> {
    SESSION.with(|s| {
        let s = s.borrow();
        let session = s.as_ref().ok_or("No open vault")?;
        mypass_core::writer::write_database_bytes(
            &session.kf,
            &session.password,
            session.keyfile.as_deref(),
            session.cipher,
            &session.kdf,
        )
    })
}

// ============================================================================
// Entrées
// ============================================================================

#[wasm_bindgen]
pub fn get_entries(group_uuid: Option<String>) -> Result<String, String> {
    with_kf(|kf| json(&mypass_core::ops::entries::list(kf, group_uuid.as_deref())?))
}

#[wasm_bindgen]
pub fn get_entry(uuid: &str) -> Result<String, String> {
    with_kf(|kf| json(&mypass_core::ops::entries::get(kf, uuid)?))
}

#[wasm_bindgen]
pub fn create_entry(new_json: &str) -> Result<String, String> {
    let new: mypass_core::ops::entries::NewEntry =
        serde_json::from_str(new_json).map_err(|e| e.to_string())?;
    with_kf_mut(|kf| json(&mypass_core::ops::entries::create(kf, new)?))
}

#[wasm_bindgen]
pub fn update_entry(uuid: &str, update_json: &str) -> Result<String, String> {
    let update: mypass_core::ops::entries::UpdateEntry =
        serde_json::from_str(update_json).map_err(|e| e.to_string())?;
    with_kf_mut(|kf| json(&mypass_core::ops::entries::update(kf, uuid, update)?))
}

#[wasm_bindgen]
pub fn delete_entry(uuid: &str) -> Result<(), String> {
    with_kf_mut(|kf| mypass_core::ops::entries::delete(kf, uuid))
}

#[wasm_bindgen]
pub fn duplicate_entry(uuid: &str) -> Result<String, String> {
    with_kf_mut(|kf| json(&mypass_core::ops::entries::duplicate(kf, uuid)?))
}

// ============================================================================
// Groupes
// ============================================================================

#[wasm_bindgen]
pub fn get_groups() -> Result<String, String> {
    with_kf(|kf| json(&mypass_core::ops::groups::list(kf)))
}

#[wasm_bindgen]
pub fn create_group(name: &str, parent_uuid: Option<String>) -> Result<String, String> {
    with_kf_mut(|kf| json(&mypass_core::ops::groups::create(kf, name, parent_uuid.as_deref())?))
}

#[wasm_bindgen]
pub fn update_group(
    uuid: &str,
    name: Option<String>,
    icon_id: Option<String>,
    is_expanded: Option<bool>,
) -> Result<String, String> {
    with_kf_mut(|kf| json(&mypass_core::ops::groups::update(kf, uuid, name, icon_id, is_expanded)?))
}

#[wasm_bindgen]
pub fn delete_group(uuid: &str) -> Result<(), String> {
    with_kf_mut(|kf| mypass_core::ops::groups::delete(kf, uuid))
}

#[wasm_bindgen]
pub fn move_entry(entry_uuid: &str, group_uuid: &str) -> Result<(), String> {
    with_kf_mut(|kf| mypass_core::ops::groups::move_entry(kf, entry_uuid, group_uuid))
}

// ============================================================================
// Fusion. Boucle de sync 4c : GET → merge_remote (`changed` = le distant
// avait du neuf → rafraîchir l'UI et sauvegarder localement). La décision de
// PUT ne vient PAS de ce `changed` : comme sync.rs côté desktop (fusion
// inverse), il faut pousser dès que la session a des mutations locales que le
// serveur n'a pas — le TS de 4c trace un dirty flag sur ses mutations (ou un
// binding needs_push sera ajouté en 4c si nécessaire).
// ============================================================================

#[wasm_bindgen]
pub fn merge_remote(remote: &[u8]) -> Result<String, String> {
    SESSION.with(|s| {
        let mut s = s.borrow_mut();
        let session = s.as_mut().ok_or("No open vault")?;
        let remote = mypass_core::reader::read_database_bytes(
            remote,
            &session.password,
            session.keyfile.as_deref(),
        )?;
        let outcome = mypass_core::merge::merge(&mut session.kf, &remote.keepass_file);
        let mut v = serde_json::to_value(&outcome).map_err(|e| e.to_string())?;
        v["changed"] = serde_json::Value::Bool(outcome.changed());
        serde_json::to_string(&v).map_err(|e| e.to_string())
    })
}

// ============================================================================
// Générateur / TOTP (sans session : purs)
// ============================================================================

#[wasm_bindgen]
pub fn generate_password(config_json: &str) -> Result<String, String> {
    let config = serde_json::from_str(config_json).map_err(|e| e.to_string())?;
    mypass_core::generator::generate_password(config)
}

#[wasm_bindgen]
pub fn generate_passphrase(config_json: &str) -> Result<String, String> {
    let config = serde_json::from_str(config_json).map_err(|e| e.to_string())?;
    mypass_core::generator::generate_passphrase(config)
}

#[wasm_bindgen]
pub fn evaluate_strength(password: &str) -> Result<String, String> {
    json(&mypass_core::generator::evaluate_strength(password.to_string())?)
}

#[wasm_bindgen]
pub fn generate_totp_code(
    secret: &str,
    algorithm: Option<String>,
    digits: Option<u32>,
    period: Option<u32>,
) -> Result<String, String> {
    json(&mypass_core::totp::generate_totp_code(
        secret.to_string(),
        algorithm,
        digits.map(|d| d as usize),
        period,
    )?)
}

#[wasm_bindgen]
pub fn generate_totp_secret() -> Result<String, String> {
    mypass_core::totp::generate_totp_secret()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_open_save_reopen_roundtrip() {
        let bytes = create_vault("Coffre Test", "s3cret").unwrap();
        open_vault(&bytes, "s3cret", None).unwrap();
        assert!(is_unlocked());
        let saved = save_vault().unwrap();
        close_vault();
        assert!(!is_unlocked());
        assert!(save_vault().is_err(), "save sans session doit échouer");
        open_vault(&saved, "s3cret", None).unwrap();
        assert!(is_unlocked());
        close_vault();
    }

    #[test]
    fn open_with_wrong_password_fails() {
        let bytes = create_vault("Coffre Test", "s3cret").unwrap();
        assert!(open_vault(&bytes, "mauvais", None).is_err());
        assert!(!is_unlocked());
    }

    fn open_fresh(name: &str) {
        let bytes = create_vault(name, "s3cret").unwrap();
        open_vault(&bytes, "s3cret", None).unwrap();
    }

    #[test]
    fn entry_crud_via_json_bindings() {
        open_fresh("Coffre Entrées");

        let created: serde_json::Value = serde_json::from_str(
            &create_entry(r#"{"title":"Site","username":"luca","password":"p4ss","url":"https://x.io"}"#).unwrap(),
        )
        .unwrap();
        let uuid = created["uuid"].as_str().unwrap().to_string();

        let listed: serde_json::Value =
            serde_json::from_str(&get_entries(None).unwrap()).unwrap();
        assert_eq!(listed.as_array().unwrap().len(), 1);

        update_entry(&uuid, r#"{"username":"luca2"}"#).unwrap();
        let got: serde_json::Value =
            serde_json::from_str(&get_entry(&uuid).unwrap()).unwrap();
        assert_eq!(got["username"], "luca2");
        assert_eq!(got["password"], "p4ss");

        let dup: serde_json::Value =
            serde_json::from_str(&duplicate_entry(&uuid).unwrap()).unwrap();
        assert_eq!(dup["title"], "Site (copy)");

        delete_entry(&uuid).unwrap();
        let listed: serde_json::Value =
            serde_json::from_str(&get_entries(None).unwrap()).unwrap();
        assert_eq!(listed.as_array().unwrap().len(), 1); // il reste le duplicata

        close_vault();
    }

    #[test]
    fn group_bindings_roundtrip() {
        open_fresh("Coffre Groupes");

        let g: serde_json::Value =
            serde_json::from_str(&create_group("Travail", None).unwrap()).unwrap();
        let guid = g["uuid"].as_str().unwrap().to_string();

        let renamed: serde_json::Value = serde_json::from_str(
            &update_group(&guid, Some("Perso".to_string()), None, None).unwrap(),
        )
        .unwrap();
        assert_eq!(renamed["name"], "Perso");

        // Entrée dans le groupe puis move vers la racine
        let e: serde_json::Value = serde_json::from_str(
            &create_entry(&format!(
                r#"{{"title":"Dans groupe","username":"u","password":"p","groupUuid":"{guid}"}}"#
            ))
            .unwrap(),
        )
        .unwrap();
        let tree: serde_json::Value =
            serde_json::from_str(&get_groups().unwrap()).unwrap();
        let root_uuid = tree[0]["uuid"].as_str().unwrap().to_string();
        move_entry(e["uuid"].as_str().unwrap(), &root_uuid).unwrap();

        delete_group(&guid).unwrap();
        let tree: serde_json::Value =
            serde_json::from_str(&get_groups().unwrap()).unwrap();
        assert_eq!(tree[0]["children"].as_array().unwrap().len(), 0);
        // L'entrée déplacée a survécu à la suppression du groupe
        let listed: serde_json::Value =
            serde_json::from_str(&get_entries(None).unwrap()).unwrap();
        assert_eq!(listed.as_array().unwrap().len(), 1);

        close_vault();
    }

    #[test]
    fn entry_ops_without_session_fail() {
        assert!(get_entries(None).is_err());
        assert!(create_entry(r#"{"title":"t","username":"u","password":"p"}"#).is_err());
    }

    #[test]
    fn merge_remote_adds_remote_entry() {
        let bytes = create_vault("Coffre Merge", "s3cret").unwrap();
        open_vault(&bytes, "s3cret", None).unwrap();

        // Distant = même lignée (relit les mêmes octets → même uuid racine),
        // avec une entrée en plus.
        let mut remote =
            mypass_core::reader::read_database_bytes(&bytes, "s3cret", None).unwrap();
        remote
            .keepass_file
            .root
            .group
            .entries
            .push(mypass_core::xml::Entry::new("Distant", "u", "p", ""));
        let remote_bytes = mypass_core::writer::write_database_bytes(
            &remote.keepass_file,
            "s3cret",
            None,
            remote.cipher,
            &remote.kdf,
        )
        .unwrap();

        let outcome: serde_json::Value =
            serde_json::from_str(&merge_remote(&remote_bytes).unwrap()).unwrap();
        assert_eq!(outcome["entriesAdded"], 1);
        assert_eq!(outcome["changed"], true);

        let listed: serde_json::Value =
            serde_json::from_str(&get_entries(None).unwrap()).unwrap();
        assert_eq!(listed.as_array().unwrap().len(), 1);

        // Re-fusionner le même distant = no-op
        let outcome: serde_json::Value =
            serde_json::from_str(&merge_remote(&remote_bytes).unwrap()).unwrap();
        assert_eq!(outcome["changed"], false);

        close_vault();
    }

    #[test]
    fn tools_bindings_work() {
        let pw = generate_password(
            r#"{"length":16,"uppercase":true,"lowercase":true,"digits":true,"symbols":false}"#,
        )
        .unwrap();
        assert_eq!(pw.len(), 16);

        let phrase = generate_passphrase(
            r#"{"word_count":4,"separator":"-","word_case":"lower"}"#,
        )
        .unwrap();
        assert_eq!(phrase.split('-').count(), 4);

        let strength: serde_json::Value =
            serde_json::from_str(&evaluate_strength(&pw).unwrap()).unwrap();
        assert!(strength["score"].is_number());

        let secret = generate_totp_secret().unwrap();
        let code: serde_json::Value =
            serde_json::from_str(&generate_totp_code(&secret, None, None, None).unwrap())
                .unwrap();
        assert_eq!(code["code"].as_str().unwrap().len(), 6);
    }

    #[test]
    fn merge_remote_without_session_fails() {
        let bytes = create_vault("Coffre Sans Session", "s3cret").unwrap();
        assert!(merge_remote(&bytes).is_err());
    }
}

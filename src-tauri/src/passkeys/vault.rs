//! Passkeys in the vault, in KeePassXC's entry fields so a vault stays
//! interchangeable with it. One passkey per entry, in its own entry.

use super::webauthn::from_b64url;
use crate::commands::browser::{collect_all_entries, find_entry_mut, set_string_field, set_string_field_protected};
use crate::kdbx::xml::{Entry, KeePassFile};
use zeroize::Zeroizing;

const USERNAME: &str = "KPEX_PASSKEY_USERNAME";
const CREDENTIAL_ID: &str = "KPEX_PASSKEY_CREDENTIAL_ID";
const PRIVATE_KEY_PEM: &str = "KPEX_PASSKEY_PRIVATE_KEY_PEM";
const RELYING_PARTY: &str = "KPEX_PASSKEY_RELYING_PARTY";
const USER_HANDLE: &str = "KPEX_PASSKEY_USER_HANDLE";

/// A passkey found in the vault, without its private key.
pub struct StoredPasskey {
    pub entry_uuid: String,
    pub title: String,
    pub username: String,
    pub credential_id: Vec<u8>,
    pub user_handle: String,
}

pub struct NewPasskey<'a> {
    pub rp_id: &'a str,
    pub rp_name: &'a str,
    pub origin: &'a str,
    pub username: &'a str,
    /// base64url
    pub credential_id: &'a str,
    pub user_handle: &'a str,
    pub private_key_pem: &'a str,
}

pub fn has_passkey(entry: &Entry) -> bool {
    entry.strings.iter().any(|s| s.key.starts_with("KPEX_PASSKEY_"))
}

/// Every passkey of the vault for exactly this relying party id.
pub fn find_for_rp(kf: &KeePassFile, rp_id: &str) -> Vec<StoredPasskey> {
    collect_all_entries(&kf.root.group)
        .into_iter()
        .filter(|e| e.get_field(RELYING_PARTY) == Some(rp_id))
        .filter_map(|e| {
            Some(StoredPasskey {
                entry_uuid: e.uuid.clone(),
                title: e.title().to_string(),
                username: e.get_field(USERNAME).filter(|u| !u.is_empty()).unwrap_or(e.username()).to_string(),
                credential_id: from_b64url(e.get_field(CREDENTIAL_ID)?).filter(|id| !id.is_empty())?,
                user_handle: e.get_field(USER_HANDLE).unwrap_or("").to_string(),
            })
        })
        .collect()
}

/// Read only once the user has confirmed, so the key never waits in memory.
pub fn private_key_pem(kf: &KeePassFile, entry_uuid: &str, rp_id: &str) -> Option<Zeroizing<String>> {
    collect_all_entries(&kf.root.group)
        .into_iter()
        .find(|e| e.uuid == entry_uuid && e.get_field(RELYING_PARTY) == Some(rp_id))
        .and_then(|e| e.get_field(PRIVATE_KEY_PEM))
        .map(|pem| Zeroizing::new(pem.to_string()))
}

/// The entry already holding a passkey for this account of this site.
pub fn existing_for_user(kf: &KeePassFile, rp_id: &str, user_handle: &str) -> Option<String> {
    collect_all_entries(&kf.root.group)
        .into_iter()
        .find(|e| e.get_field(RELYING_PARTY) == Some(rp_id) && e.get_field(USER_HANDLE) == Some(user_handle))
        .map(|e| e.uuid.clone())
}

/// Replaces the passkey of the same account, or adds a dedicated entry at the
/// root (like set-login). Returns `(entry uuid, replaced)`.
pub fn upsert(kf: &mut KeePassFile, p: &NewPasskey) -> (String, bool) {
    if let Some(uuid) = existing_for_user(kf, p.rp_id, p.user_handle) {
        if let Some(entry) = find_entry_mut(&mut kf.root.group, &uuid) {
            write_fields(entry, p);
            // The sync merge is last-writer-wins on this timestamp.
            entry.times.touch();
            return (uuid, true);
        }
    }
    let mut entry = Entry::new(p.rp_name, p.username, "", p.origin);
    write_fields(&mut entry, p);
    let uuid = entry.uuid.clone();
    kf.root.group.entries.push(entry);
    (uuid, false)
}

fn write_fields(entry: &mut Entry, p: &NewPasskey) {
    set_string_field(entry, USERNAME, p.username);
    set_string_field(entry, RELYING_PARTY, p.rp_id);
    set_string_field_protected(entry, CREDENTIAL_ID, p.credential_id);
    set_string_field_protected(entry, PRIVATE_KEY_PEM, p.private_key_pem);
    set_string_field_protected(entry, USER_HANDLE, p.user_handle);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kdbx::xml::{Entry, KeePassFile};

    fn sample() -> NewPasskey<'static> {
        NewPasskey {
            rp_id: "binance.com",
            rp_name: "Binance",
            origin: "https://accounts.binance.com",
            username: "luca@example.com",
            credential_id: "AQID",
            user_handle: "dXNlcjE",
            private_key_pem: "-----BEGIN PRIVATE KEY-----\nX\n-----END PRIVATE KEY-----\n",
        }
    }

    fn protected(e: &Entry, k: &str) -> Option<String> {
        e.strings.iter().find(|s| s.key == k).and_then(|s| s.value.protected.clone())
    }

    #[test]
    fn upsert_creates_dedicated_entry_with_protected_fields() {
        let mut kf = KeePassFile::new("t");
        let (uuid, replaced) = upsert(&mut kf, &sample());
        assert!(!replaced);
        let e = kf.root.group.entries.iter().find(|e| e.uuid == uuid).unwrap();
        assert_eq!(
            (e.title(), e.username(), e.password(), e.url()),
            ("Binance", "luca@example.com", "", "https://accounts.binance.com")
        );
        for k in ["KPEX_PASSKEY_CREDENTIAL_ID", "KPEX_PASSKEY_PRIVATE_KEY_PEM", "KPEX_PASSKEY_USER_HANDLE"] {
            assert_eq!(protected(e, k).as_deref(), Some("True"), "{k}");
        }
        assert_eq!(e.get_field("KPEX_PASSKEY_RELYING_PARTY"), Some("binance.com"));
        assert_eq!(e.get_field("KPEX_PASSKEY_USERNAME"), Some("luca@example.com"));
        assert!(has_passkey(e));
    }

    #[test]
    fn upsert_same_user_handle_replaces() {
        let mut kf = KeePassFile::new("t");
        let (first, _) = upsert(&mut kf, &sample());
        let (second, replaced) = upsert(&mut kf, &NewPasskey { credential_id: "BAUG", ..sample() });
        assert!(replaced);
        assert_eq!(first, second);
        let found = find_for_rp(&kf, "binance.com");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].credential_id, vec![4u8, 5, 6]);
        assert_eq!(existing_for_user(&kf, "binance.com", "dXNlcjE").as_deref(), Some(first.as_str()));
    }

    #[test]
    fn find_for_rp_matches_rp_exactly_and_decodes_ids() {
        let mut kf = KeePassFile::new("t");
        // Entry imported from KeePassXC: id in padded standard base64.
        upsert(&mut kf, &NewPasskey { credential_id: "AQI=", ..sample() });
        assert_eq!(find_for_rp(&kf, "binance.com")[0].credential_id, vec![1u8, 2]);
        assert!(find_for_rp(&kf, "accounts.binance.com").is_empty());
    }

    #[test]
    fn private_key_pem_requires_matching_rp() {
        let mut kf = KeePassFile::new("t");
        let (uuid, _) = upsert(&mut kf, &sample());
        assert!(private_key_pem(&kf, &uuid, "binance.com").is_some());
        assert!(private_key_pem(&kf, &uuid, "paypal.com").is_none());
        assert!(private_key_pem(&kf, "missing", "binance.com").is_none());
    }
}

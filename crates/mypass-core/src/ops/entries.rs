/// Pure vault operations on entries: no Tauri, no locking, no I/O.
/// Callers (Tauri commands, wasm bindings) own the lock/save around these.
use crate::totp::TotpCode;
use crate::xml::{self, Entry, EntryString, Group, KeePassFile, Value};
use std::collections::HashMap;

#[derive(serde::Serialize, Clone)]
pub struct EntryInfo {
    pub uuid: String,
    pub group_uuid: String,
    pub title: String,
    pub username: String,
    pub password: String,
    pub url: String,
    pub notes: String,
    pub tags: Vec<String>,
    pub icon: Option<String>,
    pub created: Option<String>,
    pub modified: Option<String>,
    pub has_totp: bool,
    /// The 2FA `otpauth://` link, or `""` (prefills the entry form).
    pub totp: String,
    pub has_passkey: bool,
    pub expired: bool,
    #[serde(rename = "customFields")]
    pub custom_fields: HashMap<String, String>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewEntry {
    pub group_uuid: Option<String>,
    pub title: String,
    pub username: String,
    pub password: String,
    pub url: Option<String>,
    pub notes: Option<String>,
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub custom_fields: Option<HashMap<String, String>>,
    /// 2FA key or link; `None` = none.
    #[serde(default)]
    pub totp: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateEntry {
    pub title: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub url: Option<String>,
    pub notes: Option<String>,
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub custom_fields: Option<HashMap<String, String>>,
    /// `None` keeps the 2FA, `""` removes it, anything else replaces it.
    #[serde(default)]
    pub totp: Option<String>,
}

pub fn list(kf: &KeePassFile, group_uuid: Option<&str>) -> Result<Vec<EntryInfo>, String> {
    let entries = if let Some(guid) = group_uuid {
        let group = find_group(&kf.root.group, guid)?;
        collect_entries(group)
    } else {
        collect_entries(&kf.root.group)
    };

    Ok(entries)
}

pub fn get(kf: &KeePassFile, uuid: &str) -> Result<EntryInfo, String> {
    let entry = find_entry(&kf.root.group, uuid)?;
    Ok(entry_to_info(entry, ""))
}

pub fn create(kf: &mut KeePassFile, new: NewEntry) -> Result<EntryInfo, String> {
    let totp = new.totp.as_deref().map(|v| resolve_totp(v, &new.title)).transpose()?;
    let mut new_entry = xml::Entry::new(
        &new.title,
        &new.username,
        &new.password,
        &new.url.unwrap_or_default(),
    );
    if let Some(notes) = &new.notes {
        set_string_field(&mut new_entry, "Notes", notes);
    }
    if let Some(tags) = new.tags.as_ref().filter(|t| !t.is_empty()) {
        new_entry.tags = Some(tags.join(","));
    }

    if let Some(fields) = &new.custom_fields {
        apply_custom_fields(&mut new_entry, fields);
    }
    if let Some(link) = &totp {
        write_totp(&mut new_entry, link);
    }

    let target_group = if let Some(guid) = &new.group_uuid {
        find_group_mut(&mut kf.root.group, guid)?
    } else {
        &mut kf.root.group
    };

    let info = entry_to_info(&new_entry, &target_group.uuid);
    target_group.entries.push(new_entry);

    Ok(info)
}

pub fn update(kf: &mut KeePassFile, uuid: &str, update: UpdateEntry) -> Result<EntryInfo, String> {
    let entry = find_entry_mut(&mut kf.root.group, uuid)?;
    // Validated before anything is touched: a refused key leaves the entry as it was.
    // The link sent back untouched by the form counts as no change, so a 2FA we
    // cannot read (Steam, KeeOtp…) neither blocks other edits nor gets rewritten.
    let title = update.title.clone().unwrap_or_else(|| entry.title().to_string());
    let current = crate::totp::entry_otp_uri(entry);
    let totp = match update.totp.as_deref() {
        Some(v) if current.as_deref() == Some(v.trim()) => None,
        other => other.map(|v| resolve_totp(v, &title)).transpose()?,
    };

    if let Some(title) = &update.title {
        set_string_field(entry, "Title", title);
    }
    if let Some(username) = &update.username {
        set_string_field(entry, "UserName", username);
    }
    if let Some(password) = &update.password {
        set_string_field_protected(entry, "Password", password);
    }
    if let Some(url) = &update.url {
        set_string_field(entry, "URL", url);
    }
    if let Some(notes) = &update.notes {
        set_string_field(entry, "Notes", notes);
    }
    if let Some(tags) = &update.tags {
        entry.tags = Some(tags.join(","));
    }
    if let Some(fields) = &update.custom_fields {
        apply_custom_fields(entry, fields);
    }
    if let Some(link) = &totp {
        write_totp(entry, link);
    }

    entry.times.touch();

    Ok(entry_to_info(entry, ""))
}

/// Current 2FA code of an entry (`TOTP_NONE` when it has no 2FA).
pub fn get_totp_code(kf: &KeePassFile, uuid: &str, now: u64) -> Result<TotpCode, String> {
    crate::totp::code_for_entry(find_entry(&kf.root.group, uuid)?, now)
}

/// The `otp` link to store for what the user typed: `""` (remove) or a link
/// that gives a real code, else `TOTP_INVALID`.
fn resolve_totp(value: &str, title: &str) -> Result<String, String> {
    if value.trim().is_empty() {
        return Ok(String::new());
    }
    let link = crate::totp::normalize(value, title);
    crate::totp::code_at(&link, 0)?;
    Ok(link)
}

/// One 2FA source of truth: `otp`; the KeePass2 legacy fields go.
fn write_totp(entry: &mut Entry, link: &str) {
    entry.strings.retain(|s| !matches!(s.key.as_str(), "otp" | "TOTP Seed" | "TOTP Settings"));
    if !link.is_empty() {
        set_string_field_protected(entry, "otp", link);
    }
}

pub fn delete(kf: &mut KeePassFile, uuid: &str) -> Result<(), String> {
    remove_entry_from_group(&mut kf.root.group, uuid)?;
    kf.root.deleted_objects.items.push(xml::DeletedObject::now(uuid));

    Ok(())
}

pub fn duplicate(kf: &mut KeePassFile, uuid: &str) -> Result<EntryInfo, String> {
    let original = find_entry(&kf.root.group, uuid)?;
    let mut clone = original.clone();
    clone.uuid = uuid::Uuid::new_v4().to_string();
    clone.times = xml::Times::now(); // le clone ne doit pas hériter des dates de l'original

    // Add " (copy)" to title
    if let Some(ts) = clone.strings.iter_mut().find(|s| s.key == "Title") {
        ts.value.content = format!("{} (copy)", ts.value.content);
    }

    let info = entry_to_info(&clone, "");
    kf.root.group.entries.push(clone);

    Ok(info)
}

// =============================================================================
// Helpers
// =============================================================================

pub(crate) fn entry_to_info(entry: &Entry, group_uuid: &str) -> EntryInfo {
    let totp = crate::totp::entry_otp_uri(entry);
    EntryInfo {
        uuid: entry.uuid.clone(),
        group_uuid: group_uuid.to_string(),
        title: entry.title().to_string(),
        username: entry.username().to_string(),
        password: entry.password().to_string(),
        url: entry.url().to_string(),
        notes: entry.notes().to_string(),
        tags: entry
            .tags
            .as_ref()
            .map(|t| t.split(',').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect())
            .unwrap_or_default(),
        icon: entry.icon_id.clone(),
        created: entry.times.creation_time.clone(),
        modified: entry.times.last_modification_time.clone(),
        has_totp: totp.is_some(),
        totp: totp.unwrap_or_default(),
        has_passkey: entry.strings.iter().any(|s| s.key.starts_with("KPEX_PASSKEY_")),
        expired: entry.times.expires.as_deref() == Some("True"),
        custom_fields: entry
            .strings
            .iter()
            .filter(|s| {
                !matches!(
                    s.key.as_str(),
                    "Title" | "UserName" | "Password" | "URL" | "Notes" | "TOTP Seed" | "TOTP Settings" | "otp"
                ) && !s.key.starts_with("KPEX_PASSKEY_")
            })
            .map(|s| (s.key.clone(), s.value.content.clone()))
            .collect(),
    }
}

pub(crate) fn collect_entries(group: &Group) -> Vec<EntryInfo> {
    let mut result: Vec<EntryInfo> = group
        .entries
        .iter()
        .map(|e| entry_to_info(e, &group.uuid))
        .collect();

    for child in &group.groups {
        result.extend(collect_entries(child));
    }

    result
}

pub(crate) fn find_group<'a>(group: &'a Group, uuid: &str) -> Result<&'a Group, String> {
    if group.uuid == uuid {
        return Ok(group);
    }
    for child in &group.groups {
        if let Ok(found) = find_group(child, uuid) {
            return Ok(found);
        }
    }
    Err(format!("Group not found: {uuid}"))
}

pub(crate) fn find_group_mut<'a>(group: &'a mut Group, uuid: &str) -> Result<&'a mut Group, String> {
    if group.uuid == uuid {
        return Ok(group);
    }
    for child in &mut group.groups {
        if let Ok(found) = find_group_mut(child, uuid) {
            return Ok(found);
        }
    }
    Err(format!("Group not found: {uuid}"))
}

pub fn find_entry<'a>(group: &'a Group, uuid: &str) -> Result<&'a Entry, String> {
    if let Some(entry) = group.entries.iter().find(|e| e.uuid == uuid) {
        return Ok(entry);
    }
    for child in &group.groups {
        if let Ok(found) = find_entry(child, uuid) {
            return Ok(found);
        }
    }
    Err(format!("Entry not found: {uuid}"))
}

pub(crate) fn find_entry_mut<'a>(group: &'a mut Group, uuid: &str) -> Result<&'a mut Entry, String> {
    if let Some(entry) = group.entries.iter_mut().find(|e| e.uuid == uuid) {
        return Ok(entry);
    }
    for child in &mut group.groups {
        if let Ok(found) = find_entry_mut(child, uuid) {
            return Ok(found);
        }
    }
    Err(format!("Entry not found: {uuid}"))
}

pub fn remove_entry_from_group(group: &mut Group, uuid: &str) -> Result<(), String> {
    if group.entries.iter().any(|e| e.uuid == uuid) {
        group.entries.retain(|e| e.uuid != uuid);
        return Ok(());
    }
    for child in &mut group.groups {
        if remove_entry_from_group(child, uuid).is_ok() {
            return Ok(());
        }
    }
    Err(format!("Entry not found: {uuid}"))
}

pub(crate) fn set_string_field(entry: &mut Entry, key: &str, value: &str) {
    if let Some(s) = entry.strings.iter_mut().find(|s| s.key == key) {
        s.value.content = value.to_string();
    } else {
        entry.strings.push(EntryString {
            key: key.to_string(),
            value: Value {
                content: value.to_string(),
                protected: None,
            },
        });
    }
}

pub(crate) fn set_string_field_protected(entry: &mut Entry, key: &str, value: &str) {
    if let Some(s) = entry.strings.iter_mut().find(|s| s.key == key) {
        s.value.content = value.to_string();
        s.value.protected = Some("True".to_string());
    } else {
        entry.strings.push(EntryString {
            key: key.to_string(),
            value: Value {
                content: value.to_string(),
                protected: Some("True".to_string()),
            },
        });
    }
}

/// Champs personnalisés stockés avec le flag Protected (comme les mots de passe).
const PROTECTED_CUSTOM_KEYS: &[&str] = &["CC_Number", "CC_CVC", "DOC_Number", "SSH_PrivateKey"];

/// Upsert des champs personnalisés ; une valeur vide supprime la clé.
pub fn apply_custom_fields(entry: &mut Entry, fields: &HashMap<String, String>) {
    for (key, value) in fields {
        if value.is_empty() {
            entry.strings.retain(|s| &s.key != key);
        } else if PROTECTED_CUSTOM_KEYS.contains(&key.as_str()) {
            set_string_field_protected(entry, key, value);
        } else {
            set_string_field(entry, key, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn card_fields() -> HashMap<String, String> {
        let mut f = HashMap::new();
        f.insert("MyPass_Type".to_string(), "card".to_string());
        f.insert("CC_Number".to_string(), "4242424242424242".to_string());
        f.insert("CC_CVC".to_string(), "123".to_string());
        f.insert("CC_Holder".to_string(), "Luca Auvray".to_string());
        f
    }

    #[test]
    fn apply_custom_fields_protects_secret_keys_only() {
        let mut entry = Entry::new("Visa perso", "", "", "");
        apply_custom_fields(&mut entry, &card_fields());

        let get = |key: &str| entry.strings.iter().find(|s| s.key == key).unwrap();
        assert_eq!(get("MyPass_Type").value.content, "card");
        assert_eq!(get("MyPass_Type").value.protected, None);
        assert_eq!(get("CC_Holder").value.protected, None);
        assert_eq!(get("CC_Number").value.protected.as_deref(), Some("True"));
        assert_eq!(get("CC_CVC").value.protected.as_deref(), Some("True"));
    }

    #[test]
    fn apply_custom_fields_updates_and_removes() {
        let mut entry = Entry::new("Visa perso", "", "", "");
        apply_custom_fields(&mut entry, &card_fields());

        let mut update = HashMap::new();
        update.insert("CC_Holder".to_string(), "L. Auvray".to_string());
        update.insert("CC_CVC".to_string(), String::new()); // vide => suppression
        apply_custom_fields(&mut entry, &update);

        let holder = entry.strings.iter().find(|s| s.key == "CC_Holder").unwrap();
        assert_eq!(holder.value.content, "L. Auvray");
        assert!(entry.strings.iter().all(|s| s.key != "CC_CVC"));
    }

    #[test]
    fn entry_to_info_exposes_custom_fields_without_standard_keys() {
        let mut entry = Entry::new("Visa perso", "user", "pass", "https://x.io");
        apply_custom_fields(&mut entry, &card_fields());
        set_string_field(&mut entry, "TOTP Seed", "JBSWY3DPEHPK3PXP");
        set_string_field(&mut entry, "KPEX_PASSKEY_CREDENTIAL", "cred-blob");

        let info = entry_to_info(&entry, "g1");
        assert_eq!(info.password, "pass");
        assert_eq!(info.custom_fields.get("MyPass_Type").unwrap(), "card");
        assert_eq!(info.custom_fields.get("CC_Number").unwrap(), "4242424242424242");
        // Les clés standard et les secrets TOTP/passkey ne doivent pas fuiter dans customFields
        for hidden_key in [
            "Title",
            "UserName",
            "Password",
            "URL",
            "Notes",
            "TOTP Seed",
            "KPEX_PASSKEY_CREDENTIAL",
        ] {
            assert!(!info.custom_fields.contains_key(hidden_key));
        }
    }

    #[test]
    fn apply_custom_fields_protects_ssh_private_key() {
        let mut entry = Entry::new("Clé serveur", "", "", "");
        let mut f = HashMap::new();
        f.insert("MyPass_Type".to_string(), "ssh_key".to_string());
        f.insert("SSH_PrivateKey".to_string(), "-----BEGIN OPENSSH PRIVATE KEY-----".to_string());
        f.insert("SSH_PublicKey".to_string(), "ssh-ed25519 AAAA test".to_string());
        apply_custom_fields(&mut entry, &f);

        let get = |key: &str| entry.strings.iter().find(|s| s.key == key).unwrap();
        assert_eq!(get("SSH_PrivateKey").value.protected.as_deref(), Some("True"));
        assert_eq!(get("SSH_PublicKey").value.protected, None);
    }

    /// Reproduit le corps de `update()` en appelant directement le helper mutable,
    /// pour vérifier que le LMT avance bien (sync s'appuie dessus).
    #[test]
    fn update_entry_bumps_lmt() {
        let mut kf = xml::KeePassFile::new("Test Vault");
        let mut entry = Entry::new("Site", "user", "pass", "https://x.io");
        entry.times.last_modification_time = Some("2020-01-01T00:00:00Z".to_string());
        let uuid = entry.uuid.clone();
        kf.root.group.entries.push(entry);

        let e = find_entry_mut(&mut kf.root.group, &uuid).unwrap();
        set_string_field(e, "UserName", "new-user");
        e.times.touch();

        let updated = find_entry(&kf.root.group, &uuid).unwrap();
        assert_eq!(updated.username(), "new-user");
        assert!(
            updated.times.last_modification_time.as_deref().unwrap() > "2020-01-01T00:00:00Z",
            "lmt n'a pas avancé après update_entry"
        );
    }

    #[test]
    fn delete_entry_creates_tombstone() {
        let mut kf = xml::KeePassFile::new("Test Vault");
        let entry = Entry::new("Site", "user", "pass", "https://x.io");
        let uuid = entry.uuid.clone();
        kf.root.group.entries.push(entry);
        assert!(kf.root.deleted_objects.items.is_empty());

        remove_entry_from_group(&mut kf.root.group, &uuid).unwrap();
        kf.root.deleted_objects.items.push(xml::DeletedObject::now(&uuid));

        assert!(find_entry(&kf.root.group, &uuid).is_err());
        assert_eq!(kf.root.deleted_objects.items.len(), 1);
        assert_eq!(kf.root.deleted_objects.items[0].uuid, uuid);
    }

    #[test]
    fn create_entry_lands_in_deep_group() {
        let mut kf = xml::KeePassFile::new("Test Vault");
        let g2 = Group::new("Child");
        let g2_uuid = g2.uuid.clone();
        let mut g1 = Group::new("Parent");
        let g1_uuid = g1.uuid.clone();
        g1.groups.push(g2);
        kf.root.group.groups.push(g1);

        let created = create(
            &mut kf,
            NewEntry {
                group_uuid: Some(g2_uuid.clone()),
                title: "Deep Site".to_string(),
                username: "user".to_string(),
                password: "pass".to_string(),
                url: None,
                notes: None,
                tags: None,
                custom_fields: None,
                totp: None,
            },
        )
        .unwrap();

        assert_eq!(created.group_uuid, g2_uuid);
        let root_child_titles: Vec<String> =
            kf.root.group.entries.iter().map(|e| e.title().to_string()).collect();
        assert!(!root_child_titles.contains(&"Deep Site".to_string()));
        let g1_ref = find_group(&kf.root.group, &g1_uuid).unwrap();
        assert!(g1_ref.entries.is_empty());
        let g2_ref = find_group(&kf.root.group, &g2_uuid).unwrap();
        assert_eq!(g2_ref.entries.len(), 1);
        assert_eq!(g2_ref.entries[0].title(), "Deep Site");
    }

    #[test]
    fn list_create_get_update_delete_duplicate_roundtrip() {
        let mut kf = xml::KeePassFile::new("Test Vault");

        let created = create(
            &mut kf,
            NewEntry {
                group_uuid: None,
                title: "Site".to_string(),
                username: "user".to_string(),
                password: "pass".to_string(),
                url: Some("https://x.io".to_string()),
                notes: None,
                tags: None,
                custom_fields: None,
                totp: None,
            },
        )
        .unwrap();

        assert_eq!(list(&kf, None).unwrap().len(), 1);
        assert_eq!(get(&kf, &created.uuid).unwrap().title, "Site");

        let updated = update(
            &mut kf,
            &created.uuid,
            UpdateEntry {
                title: None,
                username: Some("new-user".to_string()),
                password: None,
                url: None,
                notes: None,
                tags: None,
                custom_fields: None,
                totp: None,
            },
        )
        .unwrap();
        assert_eq!(updated.username, "new-user");

        let dup = duplicate(&mut kf, &created.uuid).unwrap();
        assert_eq!(dup.title, "Site (copy)");
        assert_eq!(list(&kf, None).unwrap().len(), 2);

        delete(&mut kf, &created.uuid).unwrap();
        assert_eq!(list(&kf, None).unwrap().len(), 1);
        assert_eq!(kf.root.deleted_objects.items.len(), 1);
    }

    #[test]
    fn create_entry_keeps_notes_and_tags() {
        let mut kf = xml::KeePassFile::new("Test Vault");
        let created = create(
            &mut kf,
            NewEntry {
                group_uuid: None,
                title: "Site".to_string(),
                username: "u".to_string(),
                password: "p".to_string(),
                url: None,
                notes: Some("ligne 1\nligne 2".to_string()),
                tags: Some(vec!["perso".to_string(), "web".to_string()]),
                custom_fields: None,
                totp: None,
            },
        )
        .unwrap();
        assert_eq!(created.notes, "ligne 1\nligne 2");
        assert_eq!(created.tags, vec!["perso", "web"]);
    }

    /// An empty Tags string (saved by the form, or from KeePass) is no tag, not one blank chip.
    #[test]
    fn empty_tags_string_gives_no_tags() {
        let mut entry = Entry::new("Site", "u", "p", "");
        entry.tags = Some(String::new());
        assert!(entry_to_info(&entry, "g").tags.is_empty());
        entry.tags = Some("perso, ,web,".to_string());
        assert_eq!(entry_to_info(&entry, "g").tags, vec!["perso", "web"]);
    }

    /// MyPass has no passkey feature, but a vault from KeePassXC may carry
    /// KPEX_PASSKEY_* fields: editing the entry here must not lose them.
    #[test]
    fn passkey_fields_survive_update_and_save() {
        let mut kf = xml::KeePassFile::new("Test Vault");
        let mut entry = Entry::new("GitHub", "dev", "pw", "https://github.com");
        set_string_field(&mut entry, "KPEX_PASSKEY_CREDENTIAL_ID", "cred-id");
        set_string_field_protected(&mut entry, "KPEX_PASSKEY_PRIVATE_KEY_PEM", "pem-blob");
        let uuid = entry.uuid.clone();
        kf.root.group.entries.push(entry);

        let mut fields = HashMap::new();
        fields.insert("Note".to_string(), "x".to_string());
        update(&mut kf, &uuid, UpdateEntry {
            title: Some("GitHub perso".to_string()), username: None, password: None, url: None,
            notes: None, tags: None, custom_fields: Some(fields), totp: None,
        }).unwrap();

        let bytes = crate::writer::write_database_bytes(
            &kf, "pw", None, crate::crypto::Cipher::Aes256, &crate::keys::KdfParams::default(),
        ).unwrap();
        let back = crate::reader::read_database_bytes(&bytes, "pw", None).unwrap().keepass_file;
        let e = find_entry(&back.root.group, &uuid).unwrap();
        let value = |k: &str| e.strings.iter().find(|s| s.key == k).map(|s| s.value.content.clone());
        assert_eq!(value("KPEX_PASSKEY_CREDENTIAL_ID").as_deref(), Some("cred-id"));
        assert_eq!(value("KPEX_PASSKEY_PRIVATE_KEY_PEM").as_deref(), Some("pem-blob"));
        assert_eq!(e.title(), "GitHub perso");
        assert!(entry_to_info(e, "").has_passkey);
    }

    const LINK: &str = "otpauth://totp/Site?secret=JBSWY3DPEHPK3PXP";

    fn site_with(fields: &[(&str, &str)]) -> (xml::KeePassFile, String) {
        let mut kf = xml::KeePassFile::new("t");
        let mut e = Entry::new("Site", "u", "p", "https://s.io");
        for (k, v) in fields {
            set_string_field(&mut e, k, v);
        }
        let uuid = e.uuid.clone();
        kf.root.group.entries.push(e);
        (kf, uuid)
    }

    /// (content, protected) of the string `key` of entry `uuid`.
    fn field(kf: &xml::KeePassFile, uuid: &str, key: &str) -> Option<(String, bool)> {
        let e = find_entry(&kf.root.group, uuid).unwrap();
        e.strings
            .iter()
            .find(|s| s.key == key)
            .map(|s| (s.value.content.clone(), s.value.protected.as_deref() == Some("True")))
    }

    fn set_totp(totp: Option<&str>) -> UpdateEntry {
        UpdateEntry {
            title: None, username: None, password: None, url: None, notes: None, tags: None,
            custom_fields: None, totp: totp.map(String::from),
        }
    }

    fn new_site(totp: &str) -> NewEntry {
        NewEntry {
            group_uuid: None, title: "Site".into(), username: "u".into(), password: "p".into(),
            url: None, notes: None, tags: None, custom_fields: None, totp: Some(totp.into()),
        }
    }

    #[test]
    fn update_totp_stores_protected_otp_and_drops_legacy() {
        let (mut kf, uuid) = site_with(&[("TOTP Seed", "AAAA"), ("TOTP Settings", "60;8")]);
        let info = update(&mut kf, &uuid, set_totp(Some("jbsw y3dp ehpk 3pxp"))).unwrap();
        assert_eq!(field(&kf, &uuid, "otp"), Some((LINK.to_string(), true)));
        assert_eq!(field(&kf, &uuid, "TOTP Seed"), None);
        assert_eq!(field(&kf, &uuid, "TOTP Settings"), None);
        assert_eq!(info.totp, LINK);
        assert!(info.has_totp);
    }

    #[test]
    fn update_empty_totp_removes_every_totp_field() {
        let (mut kf, uuid) = site_with(&[("otp", LINK), ("TOTP Seed", "AAAA"), ("TOTP Settings", "60;8")]);
        let info = update(&mut kf, &uuid, set_totp(Some("  "))).unwrap();
        for key in ["otp", "TOTP Seed", "TOTP Settings"] {
            assert_eq!(field(&kf, &uuid, key), None, "{key}");
        }
        assert_eq!(info.totp, "");
        assert!(!info.has_totp);
    }

    #[test]
    fn update_invalid_totp_is_refused_and_entry_untouched() {
        let (mut kf, uuid) = site_with(&[("otp", LINK)]);
        let mut upd = set_totp(Some("pas une clé !"));
        upd.title = Some("Changed".to_string());
        assert_eq!(update(&mut kf, &uuid, upd).err().as_deref(), Some("TOTP_INVALID"));
        assert_eq!(find_entry(&kf.root.group, &uuid).unwrap().title(), "Site");
        assert_eq!(field(&kf, &uuid, "otp").unwrap().0, LINK);
    }

    #[test]
    fn update_without_totp_keeps_it() {
        let (mut kf, uuid) = site_with(&[("otp", LINK)]);
        let mut upd = set_totp(None);
        upd.title = Some("Renamed".to_string());
        update(&mut kf, &uuid, upd).unwrap();
        assert_eq!(field(&kf, &uuid, "otp").unwrap().0, LINK);
    }

    #[test]
    fn update_with_same_link_keeps_it() {
        let link = "otpauth://totp/Old%20title?secret=JBSWY3DPEHPK3PXP&digits=8";
        let (mut kf, uuid) = site_with(&[("otp", link)]);
        let mut upd = set_totp(Some(link));
        upd.title = Some("New title".to_string());
        update(&mut kf, &uuid, upd).unwrap();
        assert_eq!(field(&kf, &uuid, "otp").unwrap().0, link);
    }

    #[test]
    fn create_with_totp_then_get_code() {
        let mut kf = xml::KeePassFile::new("t");
        let info = create(&mut kf, new_site("JBSWY3DPEHPK3PXP")).unwrap();
        assert_eq!(info.totp, LINK);
        assert_eq!(get_totp_code(&kf, &info.uuid, 1111111109).unwrap().code, "071271");
        assert!(get_totp_code(&kf, "nope", 0).is_err());
    }

    #[test]
    fn create_with_invalid_totp_is_refused() {
        let mut kf = xml::KeePassFile::new("t");
        assert_eq!(create(&mut kf, new_site("!!!")).err().as_deref(), Some("TOTP_INVALID"));
        assert!(kf.root.group.entries.is_empty());
    }

    #[test]
    fn untouched_unreadable_2fa_does_not_block_other_edits() {
        let link = "otpauth://totp/GitHub:me?secret=JBSWY3DPEHPK3PXP&issuer=GitHub%20Inc";
        let (mut kf, uuid) = site_with(&[("otp", link)]);
        let mut upd = set_totp(Some(link));
        upd.password = Some("new-pass".to_string());
        update(&mut kf, &uuid, upd).unwrap();
        assert_eq!(field(&kf, &uuid, "otp").unwrap().0, link);
        assert_eq!(field(&kf, &uuid, "Password").unwrap().0, "new-pass");
    }

    #[test]
    fn untouched_legacy_steam_2fa_is_kept() {
        let (mut kf, uuid) = site_with(&[("TOTP Seed", "JBSWY3DPEHPK3PXP"), ("TOTP Settings", "30;S")]);
        let prefilled = get(&kf, &uuid).unwrap().totp;
        let mut upd = set_totp(Some(&prefilled));
        upd.title = Some("Steam".to_string());
        update(&mut kf, &uuid, upd).unwrap();
        assert_eq!(field(&kf, &uuid, "TOTP Seed").unwrap().0, "JBSWY3DPEHPK3PXP");
        assert_eq!(field(&kf, &uuid, "TOTP Settings").unwrap().0, "30;S");
        assert_eq!(field(&kf, &uuid, "otp"), None);
    }
}

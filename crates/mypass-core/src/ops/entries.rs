/// Pure vault operations on entries: no Tauri, no locking, no I/O.
/// Callers (Tauri commands, wasm bindings) own the lock/save around these.
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
    pub group_uuid: Option<String>,
    #[serde(default)]
    pub custom_fields: Option<HashMap<String, String>>,
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
    let mut new_entry = xml::Entry::new(
        &new.title,
        &new.username,
        &new.password,
        &new.url.unwrap_or_default(),
    );

    if let Some(fields) = &new.custom_fields {
        apply_custom_fields(&mut new_entry, fields);
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

    // Move to different group if requested
    if let Some(_new_group_uuid) = &update.group_uuid {
        // Remove from current group... complex, skip for now
    }

    entry.times.touch();

    Ok(entry_to_info(entry, ""))
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
            .map(|t| t.split(',').map(|s| s.trim().to_string()).collect())
            .unwrap_or_default(),
        icon: entry.icon_id.clone(),
        created: entry.times.creation_time.clone(),
        modified: entry.times.last_modification_time.clone(),
        has_totp: entry.strings.iter().any(|s| s.key == "TOTP Seed" || s.key == "otp"),
        has_passkey: entry.strings.iter().any(|s| s.key.starts_with("KPEX_PASSKEY_")),
        expired: entry.times.expires.as_deref() == Some("True"),
        custom_fields: entry
            .strings
            .iter()
            .filter(|s| {
                !matches!(
                    s.key.as_str(),
                    "Title" | "UserName" | "Password" | "URL" | "Notes" | "TOTP Seed" | "otp"
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
                group_uuid: None,
                custom_fields: None,
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
            notes: None, tags: None, group_uuid: None, custom_fields: Some(fields),
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
}

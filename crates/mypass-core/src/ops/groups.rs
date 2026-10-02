/// Pure vault operations on groups: no Tauri, no locking, no I/O.
/// Callers (Tauri commands, wasm bindings) own the lock/save around these.
use super::entries;
use crate::xml::{self, Group, KeePassFile};

#[derive(serde::Serialize, Clone)]
pub struct GroupInfo {
    pub uuid: String,
    pub name: String,
    pub parent_uuid: Option<String>,
    pub icon: Option<String>,
    pub children: Vec<GroupInfo>,
    pub entry_count: usize,
    pub is_expanded: bool,
    pub created: Option<String>,
    pub modified: Option<String>,
}

pub fn list(kf: &KeePassFile) -> Vec<GroupInfo> {
    vec![group_to_info(&kf.root.group)]
}

pub fn create(
    kf: &mut KeePassFile,
    name: &str,
    parent_uuid: Option<&str>,
) -> Result<GroupInfo, String> {
    let new_group = Group::new(name);

    let parent = if let Some(guid) = parent_uuid {
        entries::find_group_mut(&mut kf.root.group, guid)?
    } else {
        &mut kf.root.group
    };

    let info = group_to_info(&new_group);
    parent.groups.push(new_group);

    Ok(info)
}

pub fn update(
    kf: &mut KeePassFile,
    uuid: &str,
    name: Option<String>,
    icon_id: Option<String>,
    is_expanded: Option<bool>,
) -> Result<GroupInfo, String> {
    let group = entries::find_group_mut(&mut kf.root.group, uuid)?;

    if let Some(n) = name {
        group.name = n;
    }
    if let Some(icon) = icon_id {
        group.icon_id = Some(icon);
    }
    if let Some(expanded) = is_expanded {
        group.is_expanded = Some(if expanded { "True".to_string() } else { "False".to_string() });
    }

    Ok(group_to_info(group))
}

pub fn delete(kf: &mut KeePassFile, uuid: &str) -> Result<(), String> {
    // Cannot delete root
    if kf.root.group.uuid == uuid {
        return Err("Cannot delete the root group".to_string());
    }

    // La fusion ne supprime jamais un groupe (v1) : sans tombstone pour
    // chaque entrée du sous-arbre, elles ressusciteraient au prochain merge.
    let target = entries::find_group_mut(&mut kf.root.group, uuid)?;
    let mut tombstone_uuids: Vec<String> =
        entries::collect_entries(target).into_iter().map(|e| e.uuid).collect();
    tombstone_uuids.push(uuid.to_string());

    remove_group(&mut kf.root.group, uuid)?;

    kf.root
        .deleted_objects
        .items
        .extend(tombstone_uuids.iter().map(|u| xml::DeletedObject::now(u)));

    Ok(())
}

pub fn move_entry(kf: &mut KeePassFile, entry_uuid: &str, group_uuid: &str) -> Result<(), String> {
    let mut moved = entries::find_entry(&kf.root.group, entry_uuid).cloned()?;
    // Valider la cible avant de retirer l'original : une cible inconnue ne
    // doit jamais faire disparaître l'entrée.
    entries::find_group(&kf.root.group, group_uuid)?;
    entries::remove_entry_from_group(&mut kf.root.group, entry_uuid)?;
    moved.times.touch(); // folder move must become visible to the merge
    entries::find_group_mut(&mut kf.root.group, group_uuid)?
        .entries
        .push(moved);
    Ok(())
}

// =============================================================================
// Helpers
// =============================================================================

fn group_to_info(group: &Group) -> GroupInfo {
    GroupInfo {
        uuid: group.uuid.clone(),
        name: group.name.clone(),
        parent_uuid: None, // Would need to track parent during traversal
        icon: group.icon_id.clone(),
        children: group.groups.iter().map(group_to_info).collect(),
        entry_count: group.entries.len(),
        is_expanded: group.is_expanded.as_deref() == Some("True"),
        created: group.times.creation_time.clone(),
        modified: group.times.last_modification_time.clone(),
    }
}

fn remove_group(parent: &mut Group, uuid: &str) -> Result<(), String> {
    if parent.groups.iter().any(|g| g.uuid == uuid) {
        parent.groups.retain(|g| g.uuid != uuid);
        return Ok(());
    }
    for child in &mut parent.groups {
        if remove_group(child, uuid).is_ok() {
            return Ok(());
        }
    }
    Err(format!("Group not found: {uuid}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_update_list_roundtrip() {
        let mut kf = xml::KeePassFile::new("Test Vault");

        let created = create(&mut kf, "Work", None).unwrap();
        assert_eq!(list(&kf)[0].children.len(), 1);

        let updated = update(&mut kf, &created.uuid, Some("Work2".to_string()), None, Some(true)).unwrap();
        assert_eq!(updated.name, "Work2");
        assert!(updated.is_expanded);
    }

    #[test]
    fn delete_group_tombstones_subtree_entries_and_group() {
        let mut kf = xml::KeePassFile::new("Test Vault");
        let sub = create(&mut kf, "Work", None).unwrap();

        let entry_uuid = {
            let target = entries::find_group_mut(&mut kf.root.group, &sub.uuid).unwrap();
            let entry = xml::Entry::new("Site", "user", "pass", "https://x.io");
            let uuid = entry.uuid.clone();
            target.entries.push(entry);
            uuid
        };

        delete(&mut kf, &sub.uuid).unwrap();

        assert!(entries::find_group(&kf.root.group, &sub.uuid).is_err());
        let tombstoned: Vec<&String> =
            kf.root.deleted_objects.items.iter().map(|d| &d.uuid).collect();
        assert!(tombstoned.contains(&&entry_uuid));
        assert!(tombstoned.contains(&&sub.uuid));
        assert_eq!(kf.root.deleted_objects.items.len(), 2);
    }

    #[test]
    fn update_group_at_depth_two_targets_the_right_group() {
        let mut kf = xml::KeePassFile::new("Test Vault");
        let g1 = create(&mut kf, "Parent", None).unwrap();
        let g2 = create(&mut kf, "Child", Some(&g1.uuid)).unwrap();

        let updated = update(&mut kf, &g2.uuid, Some("Renamed".to_string()), None, None).unwrap();

        assert_eq!(updated.uuid, g2.uuid);
        assert_eq!(updated.name, "Renamed");
        let parent = entries::find_group(&kf.root.group, &g1.uuid).unwrap();
        assert_eq!(parent.name, "Parent");
        let child = entries::find_group(&kf.root.group, &g2.uuid).unwrap();
        assert_eq!(child.name, "Renamed");
    }

    #[test]
    fn delete_group_at_depth_two_tombstones_only_its_subtree() {
        let mut kf = xml::KeePassFile::new("Test Vault");
        let g1 = create(&mut kf, "Parent", None).unwrap();
        let g2 = create(&mut kf, "Child", Some(&g1.uuid)).unwrap();

        let e1_uuid = {
            let parent = entries::find_group_mut(&mut kf.root.group, &g1.uuid).unwrap();
            let e1 = xml::Entry::new("Parent Site", "user", "pass", "https://x.io");
            let uuid = e1.uuid.clone();
            parent.entries.push(e1);
            uuid
        };
        let e2_uuid = {
            let child = entries::find_group_mut(&mut kf.root.group, &g2.uuid).unwrap();
            let e2 = xml::Entry::new("Child Site", "user", "pass", "https://x.io");
            let uuid = e2.uuid.clone();
            child.entries.push(e2);
            uuid
        };

        delete(&mut kf, &g2.uuid).unwrap();

        assert!(entries::find_group(&kf.root.group, &g2.uuid).is_err());
        assert!(entries::find_group(&kf.root.group, &g1.uuid).is_ok());
        assert!(entries::find_entry(&kf.root.group, &e1_uuid).is_ok());

        let tombstoned: Vec<&String> =
            kf.root.deleted_objects.items.iter().map(|d| &d.uuid).collect();
        assert_eq!(kf.root.deleted_objects.items.len(), 2);
        assert!(tombstoned.contains(&&e2_uuid));
        assert!(tombstoned.contains(&&g2.uuid));
        assert!(!tombstoned.contains(&&e1_uuid));
    }

    #[test]
    fn delete_root_group_is_rejected() {
        let mut kf = xml::KeePassFile::new("Test Vault");
        let root_uuid = kf.root.group.uuid.clone();
        assert!(delete(&mut kf, &root_uuid).is_err());
    }

    #[test]
    fn move_entry_relocates_and_touches() {
        let mut kf = xml::KeePassFile::new("Test Vault");
        let dest = create(&mut kf, "Work", None).unwrap();

        let mut entry = xml::Entry::new("Site", "user", "pass", "https://x.io");
        entry.times.last_modification_time = Some("2020-01-01T00:00:00Z".to_string());
        let entry_uuid = entry.uuid.clone();
        kf.root.group.entries.push(entry);

        move_entry(&mut kf, &entry_uuid, &dest.uuid).unwrap();

        assert!(kf.root.group.entries.iter().all(|e| e.uuid != entry_uuid));
        let moved = entries::find_entry(&kf.root.group, &entry_uuid).unwrap();
        assert!(
            moved.times.last_modification_time.as_deref().unwrap() > "2020-01-01T00:00:00Z",
            "move_entry doit avancer le LMT (visibilité du move au merge)"
        );
    }

    #[test]
    fn move_entry_to_root_does_not_lose_the_entry() {
        let mut kf = xml::KeePassFile::new("Test Vault");
        let src = create(&mut kf, "Source", None).unwrap();
        let entry_uuid = {
            let g = entries::find_group_mut(&mut kf.root.group, &src.uuid).unwrap();
            let e = xml::Entry::new("Site", "user", "pass", "https://x.io");
            let uuid = e.uuid.clone();
            g.entries.push(e);
            uuid
        };
        let root_uuid = kf.root.group.uuid.clone();

        move_entry(&mut kf, &entry_uuid, &root_uuid).unwrap();

        assert_eq!(kf.root.group.entries.len(), 1, "l'entrée doit être à la racine");
        delete(&mut kf, &src.uuid).unwrap();
        assert!(entries::find_entry(&kf.root.group, &entry_uuid).is_ok(), "l'entrée déplacée doit survivre à la suppression du groupe source");
    }

    #[test]
    fn move_entry_to_unknown_target_errors_and_keeps_the_entry() {
        let mut kf = xml::KeePassFile::new("Test Vault");
        let entry = xml::Entry::new("Site", "user", "pass", "https://x.io");
        let uuid = entry.uuid.clone();
        kf.root.group.entries.push(entry);

        assert!(move_entry(&mut kf, &uuid, "groupe-inexistant").is_err());
        assert!(
            entries::find_entry(&kf.root.group, &uuid).is_ok(),
            "une cible inconnue ne doit jamais faire disparaître l'entrée"
        );
    }
}

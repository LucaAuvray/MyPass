/// Pure vault operations on groups: no Tauri, no locking, no I/O.
/// Callers (Tauri commands, wasm bindings) own the lock/save around these.
use super::entries;
use crate::xml::{self, Group, KeePassFile};

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GroupInfo {
    pub uuid: String,
    pub name: String,
    pub icon: Option<String>,
    pub children: Vec<GroupInfo>,
    /// Entries of this group and of all its subgroups.
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
    let new_group = Group::new(valid_name(name)?);

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
    let name = name.as_deref().map(valid_name).transpose()?.map(str::to_string);
    let group = entries::find_group_mut(&mut kf.root.group, uuid)?;

    // Name and icon are synced (LWW on the group's LMT); expanding is local UI state.
    if name.is_some() || icon_id.is_some() {
        group.times.touch();
    }
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

/// Deletes the folder only: its entries and subgroups move up to its parent.
pub fn delete(kf: &mut KeePassFile, uuid: &str) -> Result<(), String> {
    if kf.root.group.uuid == uuid {
        return Err("Cannot delete the root group".to_string());
    }

    // The moved content must look moved to the merge on the other devices.
    let target = entries::find_group_mut(&mut kf.root.group, uuid)?;
    let now = xml::Times::now().last_modification_time;
    for entry in &mut target.entries {
        entry.times.touch();
        entry.times.location_changed = now.clone();
    }
    for group in &mut target.groups {
        group.times.location_changed = now.clone();
    }

    dissolve(&mut kf.root.group, uuid);
    kf.root.deleted_objects.items.push(xml::DeletedObject::now(uuid));

    Ok(())
}

pub fn move_entry(kf: &mut KeePassFile, entry_uuid: &str, group_uuid: &str) -> Result<(), String> {
    let mut moved = entries::find_entry(&kf.root.group, entry_uuid).cloned()?;
    // Valider la cible avant de retirer l'original : une cible inconnue ne
    // doit jamais faire disparaître l'entrée.
    entries::find_group(&kf.root.group, group_uuid)?;
    entries::remove_entry_from_group(&mut kf.root.group, entry_uuid)?;
    moved.times.touch(); // folder move must become visible to the merge
    moved.times.location_changed = moved.times.last_modification_time.clone();
    entries::find_group_mut(&mut kf.root.group, group_uuid)?
        .entries
        .push(moved);
    Ok(())
}

/// Removes the group `uuid` found anywhere under `parent` and appends its
/// entries and subgroups to its own parent. Touches nothing: callers decide.
pub(crate) fn dissolve(parent: &mut Group, uuid: &str) -> bool {
    if let Some(i) = parent.groups.iter().position(|g| g.uuid == uuid) {
        let gone = parent.groups.remove(i);
        parent.entries.extend(gone.entries);
        parent.groups.extend(gone.groups);
        return true;
    }
    parent.groups.iter_mut().any(|g| dissolve(g, uuid))
}

// =============================================================================
// Helpers
// =============================================================================

fn valid_name(name: &str) -> Result<&str, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("GROUP_NAME_REQUIRED".to_string());
    }
    Ok(name)
}

fn group_to_info(group: &Group) -> GroupInfo {
    let children: Vec<GroupInfo> = group.groups.iter().map(group_to_info).collect();
    GroupInfo {
        uuid: group.uuid.clone(),
        name: group.name.clone(),
        icon: group.icon_id.clone(),
        entry_count: group.entries.len() + children.iter().map(|c| c.entry_count).sum::<usize>(),
        children,
        is_expanded: group.is_expanded.as_deref() == Some("True"),
        created: group.times.creation_time.clone(),
        modified: group.times.last_modification_time.clone(),
    }
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

    const OLD: &str = "2020-01-01T00:00:00Z";

    #[test]
    fn blank_names_are_refused() {
        let mut kf = xml::KeePassFile::new("Test Vault");
        assert_eq!(create(&mut kf, "   ", None).err().as_deref(), Some("GROUP_NAME_REQUIRED"));
        let g = create(&mut kf, "  Perso ", None).unwrap();
        assert_eq!(g.name, "Perso");
        assert_eq!(
            update(&mut kf, &g.uuid, Some(" ".into()), None, None).err().as_deref(),
            Some("GROUP_NAME_REQUIRED")
        );
        assert_eq!(entries::find_group(&kf.root.group, &g.uuid).unwrap().name, "Perso");
    }

    #[test]
    fn rename_advances_group_lmt_but_expanding_does_not() {
        let mut kf = xml::KeePassFile::new("Test Vault");
        let g = create(&mut kf, "Work", None).unwrap();
        entries::find_group_mut(&mut kf.root.group, &g.uuid).unwrap().times.last_modification_time =
            Some(OLD.into());
        let lmt = |kf: &KeePassFile| {
            entries::find_group(&kf.root.group, &g.uuid).unwrap().times.last_modification_time.clone().unwrap()
        };

        update(&mut kf, &g.uuid, None, None, Some(false)).unwrap();
        assert_eq!(lmt(&kf), OLD);

        update(&mut kf, &g.uuid, Some("Job".into()), None, None).unwrap();
        assert!(lmt(&kf).as_str() > OLD);
    }

    #[test]
    fn delete_moves_entries_and_subgroups_to_the_parent() {
        let mut kf = xml::KeePassFile::new("Test Vault");
        let parent = create(&mut kf, "Parent", None).unwrap();
        let child = create(&mut kf, "Child", Some(&parent.uuid)).unwrap();
        let grand = create(&mut kf, "Grand", Some(&child.uuid)).unwrap();
        let entry_uuid = {
            let g = entries::find_group_mut(&mut kf.root.group, &child.uuid).unwrap();
            let mut e = xml::Entry::new("Site", "user", "pass", "https://x.io");
            e.times.last_modification_time = Some(OLD.into());
            e.times.location_changed = Some(OLD.into());
            let uuid = e.uuid.clone();
            g.entries.push(e);
            g.groups[0].times.location_changed = Some(OLD.into());
            uuid
        };

        delete(&mut kf, &child.uuid).unwrap();

        assert!(entries::find_group(&kf.root.group, &child.uuid).is_err());
        let p = entries::find_group(&kf.root.group, &parent.uuid).unwrap();
        assert_eq!(p.entries.len(), 1);
        assert_eq!(p.entries[0].uuid, entry_uuid);
        assert!(p.entries[0].times.last_modification_time.as_deref().unwrap() > OLD);
        assert!(p.entries[0].times.location_changed.as_deref().unwrap() > OLD);
        assert_eq!(p.groups.len(), 1);
        assert_eq!(p.groups[0].uuid, grand.uuid);
        assert!(p.groups[0].times.location_changed.as_deref().unwrap() > OLD);
        let tombstoned: Vec<&str> = kf.root.deleted_objects.items.iter().map(|d| d.uuid.as_str()).collect();
        assert_eq!(tombstoned, vec![child.uuid.as_str()]);
    }

    #[test]
    fn delete_top_level_group_moves_content_to_root() {
        let mut kf = xml::KeePassFile::new("Test Vault");
        let work = create(&mut kf, "Work", None).unwrap();
        entries::find_group_mut(&mut kf.root.group, &work.uuid)
            .unwrap()
            .entries
            .push(xml::Entry::new("Site", "user", "pass", "https://x.io"));

        delete(&mut kf, &work.uuid).unwrap();

        assert_eq!(kf.root.group.entries.len(), 1);
        assert!(kf.root.group.groups.is_empty());
    }

    #[test]
    fn entry_count_includes_subgroups_and_json_is_camel_case() {
        let mut kf = xml::KeePassFile::new("Test Vault");
        let parent = create(&mut kf, "Parent", None).unwrap();
        let child = create(&mut kf, "Child", Some(&parent.uuid)).unwrap();
        for (group, n) in [(&parent.uuid, 1), (&child.uuid, 2)] {
            let g = entries::find_group_mut(&mut kf.root.group, group).unwrap();
            for _ in 0..n {
                g.entries.push(xml::Entry::new("Site", "user", "pass", "https://x.io"));
            }
        }

        let info = &list(&kf)[0].children[0];
        assert_eq!(info.entry_count, 3);
        let v = serde_json::to_value(info).unwrap();
        assert!(v.get("entryCount").is_some() && v.get("isExpanded").is_some());
        assert!(v.get("parentUuid").is_none() && v.get("parent_uuid").is_none());
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
        entry.times.location_changed = Some("2020-01-01T00:00:00Z".to_string());
        let entry_uuid = entry.uuid.clone();
        kf.root.group.entries.push(entry);

        move_entry(&mut kf, &entry_uuid, &dest.uuid).unwrap();

        assert!(kf.root.group.entries.iter().all(|e| e.uuid != entry_uuid));
        let moved = entries::find_entry(&kf.root.group, &entry_uuid).unwrap();
        assert!(
            moved.times.last_modification_time.as_deref().unwrap() > "2020-01-01T00:00:00Z",
            "move_entry doit avancer le LMT (visibilité du move au merge)"
        );
        assert!(moved.times.location_changed.as_deref().unwrap() > "2020-01-01T00:00:00Z");
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

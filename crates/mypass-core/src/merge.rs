/// Fusion KDBX entrée par entrée, sémantique KeePassXC simplifiée.
/// `merge(local, remote)` applique dans `local` tout ce que `remote` sait
/// de plus récent. La plus récente gagne (LastModificationTime, comparaison
/// lexicographique — format fixe garanti par xml::format_timestamp) ;
/// la perdante va dans l'historique de la gagnante ; les suppressions se
/// propagent via DeletedObjects si la tombstone est plus récente que la
/// dernière modification.
use crate::xml::{DeletedObject, Entry, Group, History, KeePassFile};
use std::collections::HashMap;

#[derive(Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeOutcome {
    pub entries_added: usize,
    pub entries_updated: usize,
    pub entries_deleted: usize,
    pub groups_added: usize,
    pub groups_updated: usize,
    pub groups_deleted: usize,
}

impl MergeOutcome {
    pub fn changed(&self) -> bool {
        self.entries_added
            + self.entries_updated
            + self.entries_deleted
            + self.groups_added
            + self.groups_updated
            + self.groups_deleted
            > 0
    }
}

fn lmt(e: &Entry) -> &str {
    e.times.last_modification_time.as_deref().unwrap_or("")
}

/// (uuid du groupe parent, clone de l'entrée) pour tout l'arbre.
fn collect_entries(group: &Group, out: &mut Vec<(String, Entry)>) {
    for e in &group.entries {
        out.push((group.uuid.clone(), e.clone()));
    }
    for g in &group.groups {
        collect_entries(g, out);
    }
}

fn find_group_mut<'a>(group: &'a mut Group, uuid: &str) -> Option<&'a mut Group> {
    if group.uuid == uuid {
        return Some(group);
    }
    group.groups.iter_mut().find_map(|g| find_group_mut(g, uuid))
}

fn find_entry_mut<'a>(group: &'a mut Group, uuid: &str) -> Option<&'a mut Entry> {
    if let Some(e) = group.entries.iter_mut().find(|e| e.uuid == uuid) {
        return Some(e);
    }
    group.groups.iter_mut().find_map(|g| find_entry_mut(g, uuid))
}

fn remove_entry(group: &mut Group, uuid: &str) -> bool {
    let before = group.entries.len();
    group.entries.retain(|e| e.uuid != uuid);
    if group.entries.len() < before {
        return true;
    }
    group.groups.iter_mut().any(|g| remove_entry(g, uuid))
}

/// Tombstone la plus récente pour cet uuid, s'il y en a une.
fn tombstone<'a>(kf: &'a KeePassFile, uuid: &str) -> Option<&'a str> {
    kf.root
        .deleted_objects
        .items
        .iter()
        .filter(|d| d.uuid == uuid)
        .map(|d| d.deletion_time.as_str())
        .max()
}

fn glmt(g: &Group) -> &str {
    g.times.last_modification_time.as_deref().unwrap_or("")
}

/// Sous-groupes de `group` (lui exclu), à toute profondeur.
fn subgroups<'a>(group: &'a Group, out: &mut Vec<&'a Group>) {
    for g in &group.groups {
        out.push(g);
        subgroups(g, out);
    }
}

/// Parcourt l'arbre distant (racine exclue) avec, pour chaque groupe, son
/// parent local effectif. Groupe connu : nom, icône et notes du plus récent.
/// Groupe inconnu : ajouté vide (les entrées arrivent par la passe entrées),
/// sauf s'il a été supprimé ici après sa dernière modification — il n'est
/// alors pas recréé et ses sous-groupes vont au parent survivant le plus proche.
/// `redirect` reçoit, pour chaque groupe non recréé, le parent local qui le
/// remplace : ses entrées y vont, comme chez l'autre qui le dissout.
/// ponytail: folder position is not merged (no UI moves folders); a folder deleted on one
/// device and renamed on another before sync survives, its subfolders and entries may then
/// sit in different places per device.
fn merge_groups(
    local_root: &mut Group,
    local_dead: &[DeletedObject],
    remote_group: &Group,
    local_parent: &str,
    redirect: &mut HashMap<String, String>,
    outcome: &mut MergeOutcome,
) {
    for child in &remote_group.groups {
        let deleted_here = || {
            local_dead.iter().any(|d| d.uuid == child.uuid && d.deletion_time.as_str() >= glmt(child))
        };
        let next_parent = match find_group_mut(local_root, &child.uuid) {
            Some(known) => {
                if glmt(child) > glmt(known) {
                    known.name = child.name.clone();
                    known.icon_id = child.icon_id.clone();
                    known.notes = child.notes.clone();
                    known.times = child.times.clone();
                    outcome.groups_updated += 1;
                }
                child.uuid.as_str()
            }
            None if deleted_here() => {
                redirect.insert(child.uuid.clone(), local_parent.to_string());
                local_parent
            }
            None => {
                let mut shell = child.clone();
                shell.groups = vec![];
                shell.entries = vec![];
                match find_group_mut(local_root, local_parent) {
                    Some(parent) => parent.groups.push(shell),
                    None => local_root.groups.push(shell),
                }
                outcome.groups_added += 1;
                child.uuid.as_str()
            }
        };
        merge_groups(local_root, local_dead, child, next_parent, redirect, outcome);
    }
}

/// Range l'entrée `uuid` dans le groupe `target` si elle est ailleurs
/// (rien si `target` n'existe pas ici).
fn relocate_entry(root: &mut Group, uuid: &str, target: &str) {
    let already_there = find_group_mut(root, target).map(|g| g.entries.iter().any(|e| e.uuid == uuid));
    if already_there != Some(false) {
        return;
    }
    let Some(entry) = find_entry_mut(root, uuid).map(|e| e.clone()) else { return };
    remove_entry(root, uuid);
    find_group_mut(root, target).expect("checked above").entries.push(entry);
}

pub fn merge(local: &mut KeePassFile, remote: &KeePassFile) -> MergeOutcome {
    let mut outcome = MergeOutcome::default();
    let local_root_uuid = local.root.group.uuid.clone();
    let mut redirect = HashMap::new();

    // 1. Groupes (avant les entrées, pour qu'elles aient leur parent).
    merge_groups(
        &mut local.root.group,
        &local.root.deleted_objects.items,
        &remote.root.group,
        &local_root_uuid,
        &mut redirect,
        &mut outcome,
    );

    // Le parent distant d'une entrée, vu d'ici : la racine distante = la nôtre,
    // un dossier supprimé ici = le parent qui le remplace.
    let local_parent = |remote_parent: &str| {
        if remote_parent == remote.root.group.uuid {
            local_root_uuid.clone()
        } else {
            redirect.get(remote_parent).cloned().unwrap_or_else(|| remote_parent.to_string())
        }
    };

    // 2. Entrées du remote : mise à jour LWW ou ajout.
    let mut remote_entries = Vec::new();
    collect_entries(&remote.root.group, &mut remote_entries);
    for (parent_uuid, remote_entry) in &remote_entries {
        match find_entry_mut(&mut local.root.group, &remote_entry.uuid) {
            Some(local_entry) => {
                if lmt(remote_entry) > lmt(local_entry) {
                    let mut loser = local_entry.clone();
                    loser.history = None;
                    let mut winner = remote_entry.clone();
                    winner
                        .history
                        .get_or_insert_with(|| History { entries: vec![] })
                        .entries
                        .push(loser);
                    *local_entry = winner;
                    outcome.entries_updated += 1;
                    // La version gagnante apporte aussi son dossier.
                    relocate_entry(&mut local.root.group, &remote_entry.uuid, &local_parent(parent_uuid));
                }
                // égalité ou local plus récent → local conservé tel quel
            }
            None => {
                let blocked = tombstone(local, &remote_entry.uuid)
                    .is_some_and(|t| t >= lmt(remote_entry));
                if !blocked {
                    // Le parent existe (passe 1 l'a créé, ou le remplace) ; sinon racine.
                    let wanted = local_parent(parent_uuid);
                    let target_uuid = if find_group_mut(&mut local.root.group, &wanted).is_some() {
                        wanted
                    } else {
                        local_root_uuid.clone()
                    };
                    let target = find_group_mut(&mut local.root.group, &target_uuid)
                        .expect("groupe cible garanti");
                    target.entries.push(remote_entry.clone());
                    outcome.entries_added += 1;
                }
            }
        }
    }

    // 3. Suppressions : entrées locales absentes du remote avec tombstone
    //    remote plus récente que leur dernière modification.
    let remote_uuids: std::collections::HashSet<&str> =
        remote_entries.iter().map(|(_, e)| e.uuid.as_str()).collect();
    let mut local_entries = Vec::new();
    collect_entries(&local.root.group, &mut local_entries);
    for (_, local_entry) in &local_entries {
        if !remote_uuids.contains(local_entry.uuid.as_str()) {
            let doomed = tombstone(remote, &local_entry.uuid)
                .is_some_and(|t| t >= lmt(local_entry));
            if doomed && remove_entry(&mut local.root.group, &local_entry.uuid) {
                outcome.entries_deleted += 1;
            }
        }
    }

    // 4. Dossiers supprimés là-bas après leur dernière modification ici :
    //    retirés, leur contenu restant remonte au parent (rien n'est perdu).
    let mut remote_groups = Vec::new();
    subgroups(&remote.root.group, &mut remote_groups);
    let remote_group_uuids: std::collections::HashSet<&str> =
        remote_groups.iter().map(|g| g.uuid.as_str()).collect();
    let mut local_groups = Vec::new();
    subgroups(&local.root.group, &mut local_groups);
    let doomed: Vec<String> = local_groups
        .iter()
        .filter(|g| !remote_group_uuids.contains(g.uuid.as_str()))
        .filter(|g| tombstone(remote, &g.uuid).is_some_and(|t| t >= glmt(g)))
        .map(|g| g.uuid.clone())
        .collect();
    for uuid in &doomed {
        if crate::ops::groups::dissolve(&mut local.root.group, uuid) {
            outcome.groups_deleted += 1;
        }
    }

    // 5. Union des tombstones (max deletion_time par uuid) — sans compter
    //    comme changement : c'est du métadonnées de propagation.
    for d in &remote.root.deleted_objects.items {
        let existing = local
            .root
            .deleted_objects
            .items
            .iter_mut()
            .find(|l| l.uuid == d.uuid);
        match existing {
            Some(l) if l.deletion_time >= d.deletion_time => {}
            Some(l) => l.deletion_time = d.deletion_time.clone(),
            None => local.root.deleted_objects.items.push(DeletedObject {
                uuid: d.uuid.clone(),
                deletion_time: d.deletion_time.clone(),
            }),
        }
    }

    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml::{DeletedObject, Entry, Group, KeePassFile};

    /// Coffre avec une entrée au timestamp contrôlé.
    fn vault_with(entries: &[(&str, &str, &str)]) -> KeePassFile {
        // (uuid, title, last_modification_time)
        let mut kf = KeePassFile::new("Test");
        for (uuid, title, lmt) in entries {
            let mut e = Entry::new(title, "user", "pass", "https://ex.com");
            e.uuid = uuid.to_string();
            e.times.last_modification_time = Some(lmt.to_string());
            kf.root.group.entries.push(e);
        }
        kf
    }

    fn titles(kf: &KeePassFile) -> Vec<String> {
        kf.root.group.entries.iter().map(|e| e.title().to_string()).collect()
    }

    #[test]
    fn additions_from_both_sides_are_united() {
        let mut local = vault_with(&[("u1", "local-only", "2026-07-10T10:00:00Z")]);
        let remote = vault_with(&[("u2", "remote-only", "2026-07-10T11:00:00Z")]);
        let out = merge(&mut local, &remote);
        assert_eq!(out.entries_added, 1);
        assert!(out.changed());
        let mut t = titles(&local);
        t.sort();
        assert_eq!(t, vec!["local-only", "remote-only"]);
    }

    #[test]
    fn newer_remote_wins_and_loser_goes_to_history() {
        let mut local = vault_with(&[("u1", "old-title", "2026-07-10T10:00:00Z")]);
        let remote = vault_with(&[("u1", "new-title", "2026-07-10T12:00:00Z")]);
        let out = merge(&mut local, &remote);
        assert_eq!(out.entries_updated, 1);
        let e = &local.root.group.entries[0];
        assert_eq!(e.title(), "new-title");
        let hist = e.history.as_ref().expect("perdant conservé en historique");
        assert_eq!(hist.entries.len(), 1);
        assert_eq!(hist.entries[0].title(), "old-title");
    }

    #[test]
    fn newer_local_is_kept_untouched() {
        let mut local = vault_with(&[("u1", "fresh", "2026-07-10T12:00:00Z")]);
        let remote = vault_with(&[("u1", "stale", "2026-07-10T10:00:00Z")]);
        let out = merge(&mut local, &remote);
        assert!(!out.changed());
        let e = &local.root.group.entries[0];
        assert_eq!(e.title(), "fresh");
        assert!(e.history.is_none(), "pas d'historique parasite");
    }

    #[test]
    fn remote_tombstone_deletes_older_local_entry() {
        let mut local = vault_with(&[("u1", "doomed", "2026-07-10T10:00:00Z")]);
        let remote_kf = {
            let mut kf = vault_with(&[]);
            kf.root.deleted_objects.items.push(DeletedObject {
                uuid: "u1".to_string(),
                deletion_time: "2026-07-10T11:00:00Z".to_string(),
            });
            kf
        };
        let out = merge(&mut local, &remote_kf);
        assert_eq!(out.entries_deleted, 1);
        assert!(titles(&local).is_empty());
        // la tombstone est reprise localement (propagation aux autres appareils)
        assert!(local.root.deleted_objects.items.iter().any(|d| d.uuid == "u1"));
    }

    #[test]
    fn modification_newer_than_tombstone_survives() {
        let mut local = vault_with(&[("u1", "survivor", "2026-07-10T12:00:00Z")]);
        let remote_kf = {
            let mut kf = vault_with(&[]);
            kf.root.deleted_objects.items.push(DeletedObject {
                uuid: "u1".to_string(),
                deletion_time: "2026-07-10T11:00:00Z".to_string(),
            });
            kf
        };
        let out = merge(&mut local, &remote_kf);
        assert_eq!(out.entries_deleted, 0);
        assert_eq!(titles(&local), vec!["survivor"]);
    }

    #[test]
    fn tombstone_blocks_reimport_of_deleted_remote_entry() {
        // local a supprimé u1 (tombstone récente) ; remote l'a encore (version ancienne)
        let mut local = vault_with(&[]);
        local.root.deleted_objects.items.push(DeletedObject {
            uuid: "u1".to_string(),
            deletion_time: "2026-07-10T12:00:00Z".to_string(),
        });
        let remote = vault_with(&[("u1", "zombie", "2026-07-10T10:00:00Z")]);
        let out = merge(&mut local, &remote);
        assert!(!out.changed());
        assert!(titles(&local).is_empty(), "l'entrée supprimée ne doit pas revenir");
    }

    #[test]
    fn new_remote_group_is_added_with_its_entries() {
        let mut local = vault_with(&[]);
        let mut remote = vault_with(&[]);
        let mut g = Group::new("Travail");
        g.uuid = "g1".to_string();
        let mut e = Entry::new("in-group", "user", "pass", "");
        e.uuid = "u9".to_string();
        e.times.last_modification_time = Some("2026-07-10T10:00:00Z".to_string());
        g.entries.push(e);
        remote.root.group.groups.push(g);
        let out = merge(&mut local, &remote);
        assert_eq!(out.groups_added, 1);
        assert_eq!(out.entries_added, 1);
        let lg = &local.root.group.groups[0];
        assert_eq!((lg.uuid.as_str(), lg.name.as_str()), ("g1", "Travail"));
        assert_eq!(lg.entries.len(), 1);
        assert_eq!(lg.entries[0].title(), "in-group");
    }

    #[test]
    fn merge_is_idempotent() {
        let mut local = vault_with(&[("u1", "a", "2026-07-10T10:00:00Z")]);
        let remote = vault_with(&[
            ("u1", "a2", "2026-07-10T11:00:00Z"),
            ("u2", "b", "2026-07-10T09:00:00Z"),
        ]);
        assert!(merge(&mut local, &remote).changed());
        assert!(!merge(&mut local, &remote).changed(), "2e merge = no-op");
    }

    const T10: &str = "2026-07-10T10:00:00Z";
    const T11: &str = "2026-07-10T11:00:00Z";
    const T12: &str = "2026-07-10T12:00:00Z";

    fn folder(uuid: &str, name: &str, lmt: &str) -> Group {
        let mut g = Group::new(name);
        g.uuid = uuid.to_string();
        g.times.last_modification_time = Some(lmt.to_string());
        g
    }

    fn entry(uuid: &str, lmt: &str) -> Entry {
        let mut e = Entry::new(uuid, "user", "pass", "");
        e.uuid = uuid.to_string();
        e.times.last_modification_time = Some(lmt.to_string());
        e
    }

    fn tomb(uuid: &str, t: &str) -> DeletedObject {
        DeletedObject { uuid: uuid.to_string(), deletion_time: t.to_string() }
    }

    fn group_uuids(g: &Group) -> Vec<&str> {
        g.groups.iter().map(|g| g.uuid.as_str()).collect()
    }

    fn entry_uuids(g: &Group) -> Vec<&str> {
        g.entries.iter().map(|e| e.uuid.as_str()).collect()
    }

    #[test]
    fn newer_remote_folder_name_wins() {
        let mut local = vault_with(&[]);
        local.root.group.groups.push(folder("g1", "Old", T10));
        let mut remote = vault_with(&[]);
        remote.root.group.groups.push(folder("g1", "New", T11));
        let out = merge(&mut local, &remote);
        assert_eq!(local.root.group.groups[0].name, "New");
        assert_eq!(out.groups_updated, 1);
        assert!(out.changed());
    }

    #[test]
    fn older_remote_folder_name_is_ignored() {
        let mut local = vault_with(&[]);
        local.root.group.groups.push(folder("g1", "Mine", T11));
        let mut remote = vault_with(&[]);
        remote.root.group.groups.push(folder("g1", "Theirs", T10));
        let out = merge(&mut local, &remote);
        assert_eq!(local.root.group.groups[0].name, "Mine");
        assert_eq!(out.groups_updated, 0);
        assert!(!out.changed());
    }

    #[test]
    fn remote_folder_deletion_moves_local_content_to_parent() {
        let mut local = vault_with(&[]);
        let mut g1 = folder("g1", "Perso", T10);
        g1.entries.push(entry("u1", T10));
        g1.groups.push(folder("g2", "Banque", T10));
        local.root.group.groups.push(g1);
        let mut remote = vault_with(&[]);
        remote.root.group.entries.push(entry("u1", T11));
        remote.root.group.groups.push(folder("g2", "Banque", T10));
        remote.root.deleted_objects.items.push(tomb("g1", T11));
        let out = merge(&mut local, &remote);
        assert_eq!(group_uuids(&local.root.group), vec!["g2"]);
        assert_eq!(entry_uuids(&local.root.group), vec!["u1"]);
        assert_eq!(out.groups_deleted, 1);
    }

    #[test]
    fn entry_added_elsewhere_in_a_deleted_folder_survives() {
        let mut local = vault_with(&[]);
        let mut g1 = folder("g1", "Perso", T10);
        g1.entries.push(entry("u5", T12));
        local.root.group.groups.push(g1);
        let mut remote = vault_with(&[]);
        remote.root.deleted_objects.items.push(tomb("g1", T11));
        let out = merge(&mut local, &remote);
        assert!(group_uuids(&local.root.group).is_empty());
        assert_eq!(entry_uuids(&local.root.group), vec!["u5"]);
        assert_eq!((out.entries_deleted, out.groups_deleted), (0, 1));
        assert!(out.changed(), "a folder deletion alone must be pushed");
    }

    #[test]
    fn locally_deleted_folder_is_not_recreated() {
        let mut local = vault_with(&[]);
        local.root.deleted_objects.items.push(tomb("g1", T11));
        let mut remote = vault_with(&[]);
        let mut g1 = folder("g1", "Perso", T10);
        g1.groups.push(folder("g3", "Sub", T10));
        remote.root.group.groups.push(g1);
        let out = merge(&mut local, &remote);
        assert_eq!(group_uuids(&local.root.group), vec!["g3"]);
        assert_eq!(out.groups_added, 1);
    }

    #[test]
    fn folder_renamed_after_its_deletion_survives() {
        // Deleted here, renamed later over there: the rename wins.
        let mut local = vault_with(&[]);
        local.root.deleted_objects.items.push(tomb("g1", T11));
        let mut remote = vault_with(&[]);
        remote.root.group.groups.push(folder("g1", "Renamed", T12));
        let out = merge(&mut local, &remote);
        assert_eq!(group_uuids(&local.root.group), vec!["g1"]);
        assert_eq!(out.groups_added, 1);

        // Renamed here after it was deleted over there: kept.
        let mut local = vault_with(&[]);
        local.root.group.groups.push(folder("g1", "Renamed", T12));
        let mut remote = vault_with(&[]);
        remote.root.deleted_objects.items.push(tomb("g1", T11));
        let out = merge(&mut local, &remote);
        assert_eq!(group_uuids(&local.root.group), vec!["g1"]);
        assert_eq!(out.groups_deleted, 0);
    }

    #[test]
    fn newer_remote_entry_brings_its_folder() {
        let mut local = vault_with(&[]);
        local.root.group.entries.push(entry("u1", T10));
        let mut g1 = folder("g1", "Perso", T10);
        g1.entries.push(entry("u2", T10));
        local.root.group.groups.push(g1);
        let mut remote = vault_with(&[]);
        remote.root.group.entries.push(entry("u2", T11));
        let mut g1 = folder("g1", "Perso", T10);
        g1.entries.push(entry("u1", T11));
        remote.root.group.groups.push(g1);
        let out = merge(&mut local, &remote);
        assert_eq!(entry_uuids(&local.root.group.groups[0]), vec!["u1"]);
        assert_eq!(entry_uuids(&local.root.group), vec!["u2"]);
        assert_eq!(out.entries_updated, 2);
    }

    #[test]
    fn merge_with_folder_changes_is_idempotent() {
        let mut local = vault_with(&[("u2", "b", T10)]);
        let mut g2 = folder("g2", "Old2", T10);
        g2.entries.push(entry("u1", T10));
        local.root.group.groups.push(folder("g1", "Old", T10));
        local.root.group.groups.push(g2);
        let mut remote = vault_with(&[]);
        remote.root.group.entries.push(entry("u1", T11));
        let mut g1 = folder("g1", "New", T11);
        g1.entries.push(entry("u2", T11));
        remote.root.group.groups.push(g1);
        remote.root.deleted_objects.items.push(tomb("g2", T11));

        assert!(merge(&mut local, &remote).changed());
        assert_eq!(group_uuids(&local.root.group), vec!["g1"]);
        assert_eq!(local.root.group.groups[0].name, "New");
        assert_eq!(entry_uuids(&local.root.group.groups[0]), vec!["u2"]);
        assert_eq!(entry_uuids(&local.root.group), vec!["u1"]);
        assert!(!merge(&mut local, &remote).changed(), "2nd merge = no-op");
    }

    /// Uuid of the folder holding entry `uuid`.
    fn parent_of(kf: &KeePassFile, uuid: &str) -> String {
        let mut all = Vec::new();
        collect_entries(&kf.root.group, &mut all);
        all.into_iter().find(|(_, e)| e.uuid == uuid).map(|(p, _)| p).expect("entry exists")
    }

    #[test]
    fn edit_made_before_a_folder_deletion_elsewhere_survives() {
        // Both devices: root > g1 { u1 "old" @T10 }.
        let mut a = vault_with(&[]);
        let mut g1 = folder("g1", "Perso", T10);
        let mut old = Entry::new("old", "user", "pass", "");
        old.uuid = "u1".to_string();
        old.times.last_modification_time = Some(T10.to_string());
        g1.entries.push(old);
        a.root.group.groups.push(g1);
        let mut b = a.clone();
        // B edits u1 (offline), then A deletes the folder (later).
        let mut edited = Entry::new("NEW", "user", "pass", "");
        edited.uuid = "u1".to_string();
        edited.times.last_modification_time = Some(T11.to_string());
        b.root.group.groups[0].entries[0] = edited;
        crate::ops::groups::delete(&mut a, "g1").unwrap();

        merge(&mut b, &a);
        merge(&mut a, &b);

        for kf in [&a, &b] {
            assert_eq!(kf.root.group.entries.len(), 1);
            assert_eq!(kf.root.group.entries[0].title(), "NEW", "the newest edit must stay visible");
            assert!(kf.root.group.groups.is_empty());
        }
    }

    #[test]
    fn new_entry_in_a_folder_deleted_here_lands_in_the_same_place_on_both_devices() {
        // Both devices: root > work > f.
        let mut a = vault_with(&[]);
        let mut work = folder("work", "Work", T10);
        work.groups.push(folder("f", "F", T10));
        a.root.group.groups.push(work);
        let mut b = a.clone();
        // B adds n to f (offline); A deletes f.
        b.root.group.groups[0].groups[0].entries.push(entry("n", T11));
        crate::ops::groups::delete(&mut a, "f").unwrap();

        merge(&mut a, &b);
        merge(&mut b, &a);

        assert_eq!(parent_of(&a, "n"), "work", "here: the deleted folder's parent");
        assert_eq!(parent_of(&b, "n"), "work", "there: dissolved into the same parent");
        assert!(!merge(&mut a, &b).changed() && !merge(&mut b, &a).changed(), "converged");
    }
}

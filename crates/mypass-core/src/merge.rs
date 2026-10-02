/// Fusion KDBX entrée par entrée, sémantique KeePassXC simplifiée.
/// `merge(local, remote)` applique dans `local` tout ce que `remote` sait
/// de plus récent. La plus récente gagne (LastModificationTime, comparaison
/// lexicographique — format fixe garanti par xml::format_timestamp) ;
/// la perdante va dans l'historique de la gagnante ; les suppressions se
/// propagent via DeletedObjects si la tombstone est plus récente que la
/// dernière modification.
use crate::xml::{DeletedObject, Entry, Group, History, KeePassFile};

#[derive(Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeOutcome {
    pub entries_added: usize,
    pub entries_updated: usize,
    pub entries_deleted: usize,
    pub groups_added: usize,
}

impl MergeOutcome {
    pub fn changed(&self) -> bool {
        self.entries_added + self.entries_updated + self.entries_deleted + self.groups_added > 0
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

/// Ajoute les groupes de `remote` inconnus de `local` (coquilles vides,
/// les entrées arrivent par la passe entrées). Parent introuvable → racine.
/// // ponytail: les groupes ne sont jamais supprimés par la fusion en v1 —
/// // seules leurs entrées le sont ; un groupe vidé reste (suppression de
/// // groupe = cas rare, à traiter si le besoin réel apparaît).
fn add_missing_groups(
    local_root: &mut Group,
    remote_group: &Group,
    remote_parent_uuid: &str,
    outcome: &mut MergeOutcome,
) {
    if find_group_mut(local_root, &remote_group.uuid).is_none() {
        let mut shell = remote_group.clone();
        shell.groups = vec![];
        shell.entries = vec![];
        let parent = find_group_mut(local_root, remote_parent_uuid);
        let target = match parent {
            Some(p) => p,
            None => local_root,
        };
        target.groups.push(shell);
        outcome.groups_added += 1;
    }
    for child in &remote_group.groups {
        add_missing_groups(local_root, child, &remote_group.uuid, outcome);
    }
}

pub fn merge(local: &mut KeePassFile, remote: &KeePassFile) -> MergeOutcome {
    let mut outcome = MergeOutcome::default();

    // 1. Groupes inconnus (avant les entrées, pour qu'elles aient leur parent).
    for child in &remote.root.group.groups {
        add_missing_groups(&mut local.root.group, child, &remote.root.group.uuid, &mut outcome);
    }

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
                }
                // égalité ou local plus récent → local conservé tel quel
            }
            None => {
                let blocked = tombstone(local, &remote_entry.uuid)
                    .is_some_and(|t| t >= lmt(remote_entry));
                if !blocked {
                    // Le parent existe forcément si le remote l'avait (passe 1),
                    // sinon racine (uuid parent = racine remote, différente de la
                    // racine locale).
                    let root_uuid = local.root.group.uuid.clone();
                    let target_uuid = if find_group_mut(&mut local.root.group, parent_uuid).is_some() {
                        parent_uuid.clone()
                    } else {
                        root_uuid
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

    // 4. Union des tombstones (max deletion_time par uuid) — sans compter
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
}

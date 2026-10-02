# Sync desktop — Implementation Plan (sous-projet 3)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** L'app desktop Tauri se synchronise avec `mypass-server` (déployé sur `http://100.64.46.117:8787`) : pull+merge au déverrouillage, push après modification, résolution de conflit automatique par fusion KDBX entrée par entrée.

**Architecture:** Trois couches. (1) `mypass-core` gagne la correction du timestamp (`chrono_now` retournait une date figée 2024-01-01 — bloquant pour la fusion), des API sur octets (`read_database_bytes`/`write_database_bytes`) et le module `merge.rs` (LWW par `LastModificationTime`, perdants dans l'historique d'entrée, suppressions via `DeletedObjects`). (2) `src-tauri/src/commands/sync.rs` : config `%APPDATA%\MyPass\sync.json` (URL+token+etag), moteur GET→merge→PUT avec boucle 409, statut en mémoire. (3) Frontend : vue `SyncView` (réglages+statut), déclenchement après déverrouillage + boucle périodique 60 s, indicateur « non synchronisé ».

**Tech Stack:** Rust (mypass-core sans nouvelle dépendance ; src-tauri réutilise `reqwest` déjà présent), React/TanStack Query/i18next côté front.

## Global Constraints

- Spec de référence : `docs/superpowers/specs/2026-07-09-sync-backend-design.md` (sections « Flux de données » et « 4. Module de sync desktop »).
- **Contrat serveur acté** (revue finale sous-projet 2) : l'ETag reçu est ré-envoyé **verbatim** dans `If-Match` (jamais reformaté) ; sur 409 → re-GET + re-merge + re-PUT (max 3 tentatives) ; sur 5xx au PUT → l'état serveur est inconnu → re-GET avant tout retry ; premier PUT (serveur 404) = **sans** `If-Match`.
- Le serveur de prod tourne : `http://100.64.46.117:8787` (Tailscale), token détenu par Luca. Les tests automatisés n'y touchent JAMAIS (tests unitaires uniquement ; le E2E manuel de la Task 6 est le seul à parler au vrai serveur).
- Fusion : la plus récente gagne par comparaison **lexicographique** des `LastModificationTime` (format fixe `AAAA-MM-JJThh:mm:ssZ` → ordre lexicographique = ordre chronologique) ; égalité ou local plus récent → local conservé ; l'entrée perdante (débarrassée de son propre historique) est ajoutée à `history` de la gagnante.
- Suppression vs modification : une tombstone (`DeletedObject.deletion_time`) ne supprime que si `deletion_time >= LastModificationTime` de l'entrée ; sinon la modification gagne et l'entrée survit.
- Les groupes ne sont jamais supprimés par la fusion en v1 (`// ponytail:` requis dans le code). Les entrées orphelines (groupe parent inconnu localement) vont dans le groupe racine.
- Le token n'est jamais renvoyé au frontend (`get_sync_config` expose `has_token: bool`), jamais loggé. Stocké dans `%APPDATA%\MyPass\sync.json` (même convention que `commands/browser.rs::config_path()`) — plaintext assumé v1, `// ponytail:` avec upgrade path (DPAPI/keychain).
- Hors-ligne = silencieux : statut `offline`, aucune erreur bloquante, l'app fonctionne comme avant.
- **Déviation assumée vs la lettre du spec** (« à chaque sauvegarde : PUT ») : le push est déclenché par la boucle périodique 60 s + le déverrouillage + le bouton manuel, PAS par un hook dans chaque commande qui sauvegarde. Raison : toutes les commandes (entries, groups, browser…) sauvegardent en tenant le lock `DbState` — y accrocher un PUT réseau imposerait de l'async sous lock. La boucle converge vers le même état en ≤ 60 s, suffisant pour un usage mono-utilisateur.
- Toute commande Tauri nouvelle DOIT être enregistrée dans `tauri::generate_handler![]` de `src-tauri/src/lib.rs` ET avoir un cas mock dans `src/lib/tauri.ts` (le dev navigateur ne doit pas casser).
- Strings UI via i18next (`src/i18n/en.json` + `fr.json`), jamais en dur.
- `std::sync::Mutex<DbState>` ne se tient PAS à travers un `await` : verrouiller → extraire/muter → relâcher → I/O réseau → re-verrouiller.
- Fait connu, assumé : les entrées créées avant ce plan portent des timestamps bogués `2024-01-01Thh:mm:ssZ` (bug corrigé en Task 1) — elles seront considérées « anciennes » par la fusion. Sans conséquence pour un coffre mono-utilisateur qui converge.
- Non-régression : `cargo test` dans `crates/mypass-core` et `src-tauri`, `cargo clippy` sans nouveau warning, `npm run lint` et `npm run build` verts.

---

### Task 1: Corriger `chrono_now()` (mypass-core)

**Files:**
- Modify: `crates/mypass-core/src/xml.rs` (fonction `chrono_now()` ~ligne 371, + module de tests en fin de fichier)

**Interfaces:**
- Consumes: rien.
- Produces: `chrono_now()` (privée, inchangée en visibilité) retourne le VRAI instant UTC au format `AAAA-MM-JJThh:mm:ssZ`. Nouvelle fonction privée testable `format_timestamp(unix_secs: u64) -> String`.

- [ ] **Step 1: Écrire les tests qui échouent**

Ajouter en fin de `crates/mypass-core/src/xml.rs` :

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_timestamp_known_vectors() {
        assert_eq!(format_timestamp(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_timestamp(86_400), "1970-01-02T00:00:00Z");
        // 29 février 2000 (année bissextile, divisible par 400)
        assert_eq!(format_timestamp(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(format_timestamp(1_704_067_200), "2024-01-01T00:00:00Z");
    }

    #[test]
    fn chrono_now_is_not_frozen_in_2024() {
        let now = chrono_now();
        assert!(now > "2026-01-01".to_string(), "date figée: {now}");
        assert_eq!(now.len(), "2026-07-10T12:00:00Z".len());
        assert!(now.ends_with('Z'));
    }
}
```

- [ ] **Step 2: Vérifier l'échec**

```powershell
cd crates/mypass-core
cargo test xml::tests
```

Expected: FAIL — `format_timestamp` n'existe pas (erreur de compilation).

- [ ] **Step 3: Implémenter**

Remplacer intégralement `chrono_now()` par :

```rust
/// Get current timestamp in KDBX format (ISO 8601 UTC).
fn chrono_now() -> String {
    use std::time::SystemTime;
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format_timestamp(secs)
}

/// Unix → "AAAA-MM-JJThh:mm:ssZ". Algorithme civil-from-days (Howard Hinnant),
/// évite une dépendance chrono. Format à longueur fixe : la comparaison
/// lexicographique de deux timestamps == comparaison chronologique (merge.rs).
fn format_timestamp(unix_secs: u64) -> String {
    let days = (unix_secs / 86_400) as i64;
    let tod = unix_secs % 86_400;
    let (h, min, s) = (tod / 3600, (tod % 3600) / 60, tod % 60);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{min:02}:{s:02}Z")
}
```

- [ ] **Step 4: Vérifier que tout passe (crate + consommateurs)**

```powershell
cd crates/mypass-core
cargo test
cd ../../src-tauri
cargo test
```

Expected: PASS partout (7 tests core après ajout, 28 src-tauri).

- [ ] **Step 5: Commit**

```powershell
git add crates/mypass-core/src/xml.rs
git commit -m "fix(core): chrono_now returned a frozen 2024-01-01 date

Real civil-from-days conversion; fixed-width ISO 8601 so lexicographic
comparison equals chronological order, which the KDBX merge relies on.

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 2: API sur octets dans reader/writer (mypass-core)

**Files:**
- Modify: `crates/mypass-core/src/reader.rs`
- Modify: `crates/mypass-core/src/writer.rs`
- Test: `#[cfg(test)]` ajouté dans `writer.rs`

**Interfaces:**
- Consumes: rien.
- Produces:
  - `reader::read_database_bytes(data: &[u8], password: &str, keyfile_data: Option<&[u8]>) -> Result<DatabaseReadResult, String>` — corps actuel de `read_database` sans le `fs::read` ; `read_database(path, ...)` devient `fs::read` + délégation.
  - `writer::write_database_bytes(keepass_file: &KeePassFile, password: &str, keyfile_data: Option<&[u8]>, cipher: Cipher, kdf: &KdfParams) -> Result<Vec<u8>, String>` — corps actuel sans l'écriture disque, retourne le buffer ; `write_database(path, ...)` devient délégation + `fs::write`.
  - Signatures fichiers INCHANGÉES (aucun appelant modifié).

- [ ] **Step 1: Écrire le test qui échoue**

En fin de `crates/mypass-core/src/writer.rs` :

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader;

    #[test]
    fn bytes_roundtrip_without_disk() {
        let kf = KeePassFile::new("RoundTrip");
        let kdf = KdfParams::default();
        let bytes =
            write_database_bytes(&kf, "motdepasse", None, Cipher::ChaCha20, &kdf).unwrap();
        assert_eq!(&bytes[0..4], &0x03D9A29Au32.to_le_bytes());
        let result = reader::read_database_bytes(&bytes, "motdepasse", None).unwrap();
        assert_eq!(result.keepass_file.meta.database_name, "RoundTrip");
        assert!(
            reader::read_database_bytes(&bytes, "mauvais", None).is_err(),
            "mauvais mot de passe doit échouer"
        );
    }
}
```

(Import nécessaire en tête du module de test : `use crate::xml::KeePassFile;` si non couvert par `super::*`.)

- [ ] **Step 2: Vérifier l'échec**

```powershell
cd crates/mypass-core
cargo test writer::tests
```

Expected: FAIL — `write_database_bytes`/`read_database_bytes` n'existent pas.

- [ ] **Step 3: Refactorer reader.rs**

```rust
pub fn read_database(
    path: &Path,
    password: &str,
    keyfile_data: Option<&[u8]>,
) -> Result<DatabaseReadResult, String> {
    let data = fs::read(path).map_err(|e| format!("Failed to read file: {e}"))?;
    read_database_bytes(&data, password, keyfile_data)
}

pub fn read_database_bytes(
    data: &[u8],
    password: &str,
    keyfile_data: Option<&[u8]>,
) -> Result<DatabaseReadResult, String> {
    let (header, encrypted_payload) = parse_header(data)?;
    // ... reste du corps actuel de read_database, inchangé ...
}
```

- [ ] **Step 4: Refactorer writer.rs**

```rust
pub fn write_database(
    path: &Path,
    keepass_file: &KeePassFile,
    password: &str,
    keyfile_data: Option<&[u8]>,
    cipher: Cipher,
    kdf: &KdfParams,
) -> Result<(), String> {
    let output = write_database_bytes(keepass_file, password, keyfile_data, cipher, kdf)?;
    fs::write(path, output).map_err(|e| format!("Failed to write file: {e}"))
}

pub fn write_database_bytes(
    keepass_file: &KeePassFile,
    password: &str,
    keyfile_data: Option<&[u8]>,
    cipher: Cipher,
    kdf: &KdfParams,
) -> Result<Vec<u8>, String> {
    // ... corps actuel jusqu'à la construction complète de `output`,
    //     puis `Ok(output)` au lieu de l'écriture disque ...
}
```

- [ ] **Step 5: Vérifier que tout passe**

```powershell
cd crates/mypass-core
cargo test
cargo check --target wasm32-unknown-unknown
cd ../../src-tauri
cargo test
```

Expected: PASS partout (8 tests core), wasm OK — les nouvelles fonctions octets sont d'ailleurs exactement ce que le sous-projet 4 utilisera en WASM.

- [ ] **Step 6: Commit**

```powershell
git add crates/mypass-core/src/reader.rs crates/mypass-core/src/writer.rs
git commit -m "feat(core): byte-based read/write APIs, file functions delegate

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 3: Fusion KDBX (`crates/mypass-core/src/merge.rs`)

Le cœur du sous-projet. Sémantique alignée sur KeePassXC, simplifiée (voir Global Constraints).

**Files:**
- Create: `crates/mypass-core/src/merge.rs`
- Modify: `crates/mypass-core/src/lib.rs` (ajouter `pub mod merge;`)
- Test: `#[cfg(test)]` dans `merge.rs`

**Interfaces:**
- Consumes: `xml::{KeePassFile, Group, Entry, DeletedObject, History}` (structures existantes, toutes `Clone`).
- Produces:
  - `merge::merge(local: &mut KeePassFile, remote: &KeePassFile) -> MergeOutcome` — fusionne `remote` DANS `local`.
  - `MergeOutcome { entries_added, entries_updated, entries_deleted, groups_added: usize }` + `fn changed(&self) -> bool`.
  - Usage bidirectionnel par le moteur de sync : `merge(local, remote)` = pull ; `merge(remote_clone, local)` sur un clone = « le serveur a-t-il besoin de nos changements ? » (si `changed()` → push).

- [ ] **Step 1: Écrire les tests qui échouent**

`merge.rs` avec uniquement le module de tests (l'implémentation vient au Step 3) :

```rust
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
```

Note : `Entry::title()` existe déjà (helper `get_field("Title")` dans xml.rs).

- [ ] **Step 2: Déclarer le module et vérifier l'échec**

Ajouter `pub mod merge;` dans `crates/mypass-core/src/lib.rs` (ordre alphabétique : entre `keys` et `reader`).

```powershell
cd crates/mypass-core
cargo test merge
```

Expected: FAIL — `merge` et `MergeOutcome` non définis.

- [ ] **Step 3: Implémenter la fusion (en tête de `merge.rs`, avant le module de tests)**

```rust
/// Fusion KDBX entrée par entrée, sémantique KeePassXC simplifiée.
/// `merge(local, remote)` applique dans `local` tout ce que `remote` sait
/// de plus récent. La plus récente gagne (LastModificationTime, comparaison
/// lexicographique — format fixe garanti par xml::format_timestamp) ;
/// la perdante va dans l'historique de la gagnante ; les suppressions se
/// propagent via DeletedObjects si la tombstone est plus récente que la
/// dernière modification.
use crate::xml::{DeletedObject, Entry, Group, History, KeePassFile};

#[derive(Debug, Default, PartialEq)]
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
```

Attention piège : dans la passe 4, ne PAS incrémenter l'outcome — sinon `merge_is_idempotent` échoue (l'union des tombstones re-tournerait `changed()==true` alors que rien ne change). Si le test d'idempotence échoue quand même : vérifier que la passe 2 n'ajoute pas d'historique quand les timestamps sont égaux (règle : strictement supérieur).

- [ ] **Step 4: Vérifier que tout passe**

```powershell
cd crates/mypass-core
cargo test
cargo clippy
cargo check --target wasm32-unknown-unknown
```

Expected: PASS — 16 tests core (8 existants + 8 merge), clippy sans nouveau warning, wasm OK (merge = pur calcul, aucune I/O).

- [ ] **Step 5: Commit**

```powershell
git add crates/mypass-core/src/merge.rs crates/mypass-core/src/lib.rs
git commit -m "feat(core): KDBX merge (LWW by LastModificationTime, tombstones, history)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 4: Moteur de sync côté Tauri (`commands/sync.rs`)

**Files:**
- Create: `src-tauri/src/commands/sync.rs`
- Modify: `src-tauri/src/commands/mod.rs` (ajouter `pub mod sync;`)
- Modify: `src-tauri/src/lib.rs` (`.manage(SyncRuntime::default())` + 4 commandes dans `generate_handler![]`)
- Test: `#[cfg(test)]` dans `sync.rs` (config uniquement — le moteur HTTP est couvert par le E2E Task 6)

**Interfaces:**
- Consumes: `DbState` (+ son `save()`), `kdbx::reader::read_database_bytes`, `kdbx::writer::write_database_bytes`, `kdbx::merge::merge` (Tasks 2-3), `reqwest` (déjà en dépendance).
- Produces (commandes enregistrées, consommées par la Task 5) :
  - `get_sync_config() -> { server_url: String, enabled: bool, has_token: bool }` (JSON camelCase via serde rename)
  - `set_sync_config(server_url: String, token: Option<String>, enabled: bool)` — `token: None` conserve le token existant (permet de modifier l'URL sans re-saisir le token)
  - `sync_now() -> SyncStatus` — exécute une synchronisation complète
  - `get_sync_status() -> SyncStatus` avec `SyncStatus { state, detail, last_sync, server_version }`, `state ∈ {"not_configured","idle","syncing","synced","offline","error"}`
  - Type partagé `SyncRuntime = Arc<Mutex<SyncStatus>>` managé par Tauri.

- [ ] **Step 1: Écrire les tests qui échouent (config)**

Dans le futur `sync.rs` (module de tests) :

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_roundtrip_and_token_preservation() {
        let dir = std::env::temp_dir().join(format!("mypass-sync-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sync.json");

        let cfg = SyncConfig {
            server_url: "http://example:8787".into(),
            token: "secret".into(),
            enabled: true,
            last_etag: Some("\"3\"".into()),
        };
        store_config_at(&path, &cfg).unwrap();
        let loaded = load_config_at(&path);
        assert_eq!(loaded.server_url, "http://example:8787");
        assert_eq!(loaded.token, "secret");
        assert_eq!(loaded.last_etag.as_deref(), Some("\"3\""));

        // fichier absent → défaut silencieux
        let missing = load_config_at(&dir.join("nope.json"));
        assert!(!missing.enabled);
        assert!(missing.token.is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }
}
```

- [ ] **Step 2: Vérifier l'échec**

```powershell
cd src-tauri
cargo test sync
```

Expected: FAIL — module/types inexistants.

- [ ] **Step 3: Implémenter `sync.rs`**

```rust
/// Synchronisation avec mypass-server : GET → merge → PUT, boucle 409.
/// Contrat serveur (voir plan serveur) : ETag ré-envoyé VERBATIM dans
/// If-Match ; 5xx au PUT = état inconnu → re-GET avant retry ; serveur
/// vide (404) → premier PUT sans If-Match.
use crate::commands::database::DbState;
use crate::kdbx::{self, merge};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::State;

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct SyncConfig {
    pub server_url: String,
    // ponytail: token en clair dans %APPDATA% (comme l'association
    // keepassxc-browser) — passer par DPAPI/keychain si ça devient un vrai risque.
    pub token: String,
    pub enabled: bool,
    #[serde(default)]
    pub last_etag: Option<String>,
}

#[derive(Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub state: String, // not_configured | idle | syncing | synced | offline | error
    pub detail: Option<String>,
    pub last_sync: Option<String>,
    pub server_version: Option<String>,
}

pub type SyncRuntime = Arc<Mutex<SyncStatus>>;

fn config_path() -> Option<PathBuf> {
    let appdata = std::env::var("APPDATA").ok()?;
    Some(PathBuf::from(appdata).join("MyPass").join("sync.json"))
}

fn load_config_at(path: &Path) -> SyncConfig {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn store_config_at(path: &Path, cfg: &SyncConfig) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

pub fn load_config() -> SyncConfig {
    config_path().as_deref().map(load_config_at).unwrap_or_default()
}

fn store_config(cfg: &SyncConfig) -> Result<(), String> {
    store_config_at(&config_path().ok_or("APPDATA introuvable")?, cfg)
}

fn set_status(runtime: &SyncRuntime, state: &str, detail: Option<String>) {
    let mut s = runtime.lock().unwrap();
    s.state = state.to_string();
    s.detail = detail;
}

#[tauri::command]
pub async fn get_sync_config() -> Result<serde_json::Value, String> {
    let cfg = load_config();
    Ok(serde_json::json!({
        "serverUrl": cfg.server_url,
        "enabled": cfg.enabled,
        "hasToken": !cfg.token.is_empty(),
    }))
}

#[tauri::command]
pub async fn set_sync_config(
    server_url: String,
    token: Option<String>,
    enabled: bool,
) -> Result<(), String> {
    let mut cfg = load_config();
    // URL changée → l'etag mémorisé ne veut plus rien dire
    if cfg.server_url != server_url {
        cfg.last_etag = None;
    }
    cfg.server_url = server_url.trim().trim_end_matches('/').to_string();
    if let Some(t) = token {
        if !t.trim().is_empty() {
            cfg.token = t.trim().to_string();
        }
    }
    cfg.enabled = enabled;
    store_config(&cfg)
}

#[tauri::command]
pub async fn get_sync_status(runtime: State<'_, SyncRuntime>) -> Result<SyncStatus, String> {
    Ok(runtime.lock().unwrap().clone())
}

#[tauri::command]
pub async fn sync_now(
    db: State<'_, Arc<Mutex<DbState>>>,
    runtime: State<'_, SyncRuntime>,
) -> Result<SyncStatus, String> {
    let result = run_sync(db.inner().clone(), runtime.inner().clone()).await;
    let status = runtime.lock().unwrap().clone();
    result.map(|_| status)
}

async fn run_sync(db: Arc<Mutex<DbState>>, runtime: SyncRuntime) -> Result<(), String> {
    let mut cfg = load_config();
    if !cfg.enabled || cfg.server_url.is_empty() || cfg.token.is_empty() {
        set_status(&runtime, "not_configured", None);
        return Err("sync non configurée".into());
    }
    if !db.lock().unwrap().is_open {
        return Err("aucun coffre ouvert".into());
    }
    set_status(&runtime, "syncing", None);
    let client = reqwest::Client::new();
    let url = format!("{}/api/vault", cfg.server_url);

    for _attempt in 0..3 {
        // ---- GET (jamais de If-Match : on veut l'état courant) ----
        let resp = match client.get(&url).bearer_auth(&cfg.token).send().await {
            Ok(r) => r,
            Err(e) => {
                set_status(&runtime, "offline", Some(e.to_string()));
                return Err(format!("serveur injoignable: {e}"));
            }
        };
        let (remote_etag, remote_bytes): (Option<String>, Option<Vec<u8>>) =
            match resp.status().as_u16() {
                200 => {
                    let etag = resp
                        .headers()
                        .get("etag")
                        .and_then(|v| v.to_str().ok())
                        .map(String::from);
                    let bytes = resp.bytes().await.map_err(|e| e.to_string())?.to_vec();
                    (etag, Some(bytes))
                }
                404 => (None, None),
                401 => {
                    set_status(&runtime, "error", Some("token refusé (401)".into()));
                    return Err("token refusé".into());
                }
                s => {
                    set_status(&runtime, "error", Some(format!("GET {s}")));
                    return Err(format!("GET {s}"));
                }
            };

        // ---- Merge + décision de push, sous le lock, sans await ----
        let push_bytes: Option<Vec<u8>> = {
            let mut db = db.lock().unwrap();
            if !db.is_open || db.keepass_file.is_none() {
                return Err("coffre fermé pendant la sync".into());
            }
            let password = String::from_utf8(
                db.password_hash.as_ref().ok_or("mot de passe non disponible")?.clone(),
            )
            .map_err(|_| "encodage mot de passe")?;
            let keyfile = db.keyfile_data.clone();

            match &remote_bytes {
                Some(bytes) => {
                    let remote =
                        kdbx::reader::read_database_bytes(bytes, &password, keyfile.as_deref())
                            .map_err(|e| {
                                set_status(
                                    &runtime,
                                    "error",
                                    Some("coffre distant illisible (mot de passe différent ?)".into()),
                                );
                                format!("déchiffrement distant: {e}")
                            })?;
                    let kf = db.keepass_file.as_mut().unwrap();
                    let pulled = merge::merge(kf, &remote.keepass_file);
                    if pulled.changed() {
                        db.save()?;
                    }
                    // Le serveur a-t-il besoin de nos changements ?
                    let mut remote_view = remote.keepass_file.clone();
                    let to_push =
                        merge::merge(&mut remote_view, db.keepass_file.as_ref().unwrap());
                    if to_push.changed() {
                        Some(kdbx::writer::write_database_bytes(
                            db.keepass_file.as_ref().unwrap(),
                            &password,
                            keyfile.as_deref(),
                            db.cipher,
                            &db.kdf,
                        )?)
                    } else {
                        None
                    }
                }
                None => Some(kdbx::writer::write_database_bytes(
                    db.keepass_file.as_ref().unwrap(),
                    &password,
                    keyfile.as_deref(),
                    db.cipher,
                    &db.kdf,
                )?),
            }
        };

        // ---- PUT si nécessaire ----
        let Some(body) = push_bytes else {
            finish_synced(&runtime, &mut cfg, remote_etag)?;
            return Ok(());
        };
        let mut req = client.put(&url).bearer_auth(&cfg.token).body(body);
        if let Some(etag) = &remote_etag {
            req = req.header("If-Match", etag); // écho VERBATIM
        }
        let resp = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                set_status(&runtime, "offline", Some(e.to_string()));
                return Err(format!("PUT: {e}"));
            }
        };
        match resp.status().as_u16() {
            200 => {
                let new_etag = resp
                    .headers()
                    .get("etag")
                    .and_then(|v| v.to_str().ok())
                    .map(String::from);
                finish_synced(&runtime, &mut cfg, new_etag)?;
                return Ok(());
            }
            // 409 : version bougée entre GET et PUT → re-GET + re-merge.
            // 5xx : état serveur inconnu → même traitement (re-GET d'abord).
            409 | 500..=599 => continue,
            401 => {
                set_status(&runtime, "error", Some("token refusé (401)".into()));
                return Err("token refusé".into());
            }
            s => {
                set_status(&runtime, "error", Some(format!("PUT {s}")));
                return Err(format!("PUT {s}"));
            }
        }
    }
    set_status(&runtime, "error", Some("conflit persistant (3 tentatives)".into()));
    Err("conflit persistant".into())
}

fn finish_synced(
    runtime: &SyncRuntime,
    cfg: &mut SyncConfig,
    etag: Option<String>,
) -> Result<(), String> {
    cfg.last_etag = etag.clone();
    store_config(cfg)?;
    let mut s = runtime.lock().unwrap();
    s.state = "synced".into();
    s.detail = None;
    s.last_sync = Some(now_iso());
    s.server_version = etag.map(|e| e.trim_matches('"').to_string());
    Ok(())
}

fn now_iso() -> String {
    // Affichage uniquement (statut UI) — pas utilisé par la fusion.
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    format!("{secs}")
}
```

Note pour l'implémenteur : `now_iso()` retournant l'epoch en secondes est volontairement fruste — le front formate avec `new Date(Number(lastSync) * 1000).toLocaleTimeString()`. Ne pas réimplémenter un formateur de date côté Rust.

- [ ] **Step 4: Enregistrer module + commandes + état managé**

`src-tauri/src/commands/mod.rs` : ajouter `pub mod sync;`.

`src-tauri/src/lib.rs` :
- près du `.manage(Arc::new(Mutex::new(DbState::default())))` existant, ajouter :
  ```rust
  .manage(commands::sync::SyncRuntime::default())
  ```
- dans `tauri::generate_handler![...]`, ajouter :
  ```rust
  commands::sync::get_sync_config,
  commands::sync::set_sync_config,
  commands::sync::get_sync_status,
  commands::sync::sync_now,
  ```

- [ ] **Step 5: Vérifier**

```powershell
cd src-tauri
cargo test
cargo clippy
```

Expected: PASS (29 tests : 28 + 1 sync), clippy sans nouveau warning.

- [ ] **Step 6: Commit**

```powershell
git add src-tauri/src/commands/sync.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs
git commit -m "feat(tauri): sync engine — GET/merge/PUT with 409 retry loop

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 5: Frontend — vue Sync, déclencheurs, indicateur

**Files:**
- Modify: `src/lib/tauri.ts` (cas mock pour les 4 commandes)
- Create: `src/views/SyncView.tsx`
- Modify: `src/App.tsx` (route `/sync`)
- Modify: `src/components/layout/AppSidebar.tsx` (entrée de nav, même motif que l'entrée existante vers `/browser`)
- Modify: `src/views/UnlockView.tsx` (déclenchement post-déverrouillage)
- Modify: `src/components/layout/AppShell.tsx` (boucle périodique 60 s + indicateur d'état)
- Modify: `src/i18n/en.json`, `src/i18n/fr.json`

**Interfaces:**
- Consumes: commandes Task 4 via le helper existant `tauriCommand<T>(command, args)` (`src/lib/tauri.ts:417`).
- Produces: route `/sync`, hook `useSyncStatus()` (TanStack Query, `refetchInterval: 5000`), déclencheurs de sync.

- [ ] **Step 1: Mocks dans `src/lib/tauri.ts`**

Dans le `switch` de `createMockInvoke()` (suivre le style des cas `is_ssh_agent_enabled`/`toggle_ssh_agent`), ajouter :

```ts
case "get_sync_config":
  return { serverUrl: "", enabled: false, hasToken: false } as T;
case "set_sync_config":
  return null as T;
case "get_sync_status":
case "sync_now":
  return { state: "not_configured", detail: null, lastSync: null, serverVersion: null } as T;
```

- [ ] **Step 2: Vue `src/views/SyncView.tsx`**

Modeler sur `BrowserIntegrationView.tsx` (mêmes primitives shadcn : Card, Input, Switch, Button — réutiliser celles déjà importées ailleurs, ne pas en ajouter de nouvelles via CLI pour cette vue). Contenu :

```tsx
/** Réglages et état de la synchronisation serveur. */
import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { tauriCommand } from "@/lib/tauri";

type SyncConfig = { serverUrl: string; enabled: boolean; hasToken: boolean };
export type SyncStatus = {
  state: string;
  detail: string | null;
  lastSync: string | null;
  serverVersion: string | null;
};

export function useSyncStatus() {
  return useQuery({
    queryKey: ["sync-status"],
    queryFn: () => tauriCommand<SyncStatus>("get_sync_status"),
    refetchInterval: 5000,
  });
}

export function SyncView() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const config = useQuery({
    queryKey: ["sync-config"],
    queryFn: () => tauriCommand<SyncConfig>("get_sync_config"),
  });
  const status = useSyncStatus();
  const [serverUrl, setServerUrl] = useState<string | null>(null);
  const [token, setToken] = useState("");

  const save = useMutation({
    mutationFn: (enabled: boolean) =>
      tauriCommand("set_sync_config", {
        serverUrl: serverUrl ?? config.data?.serverUrl ?? "",
        token: token || null,
        enabled,
      }),
    onSuccess: () => {
      setToken("");
      qc.invalidateQueries({ queryKey: ["sync-config"] });
    },
  });
  const syncNow = useMutation({
    mutationFn: () => tauriCommand<SyncStatus>("sync_now"),
    onSettled: () => qc.invalidateQueries({ queryKey: ["sync-status"] }),
  });

  // Rendu : carte réglages (URL, token en type="password" avec placeholder
  // t("sync.tokenKept") si hasToken, switch enabled → save.mutate(checked)),
  // bouton t("sync.syncNow") → syncNow.mutate() (disabled si status syncing),
  // ligne d'état : t(`sync.state.${status.data?.state ?? "idle"}`) + detail
  // + version serveur + heure dernière sync.
  // Suivre la mise en page de BrowserIntegrationView.
  ...
}
```

(Le corps JSX exact suit la structure de `BrowserIntegrationView` — l'implémenteur la lit et calque ; les éléments obligatoires sont ceux listés en commentaire.)

- [ ] **Step 3: Route + navigation**

`src/App.tsx` : `import { SyncView } from "./views/SyncView";` + `<Route path="/sync" element={<SyncView />} />` (à côté de la route `/browser`, App.tsx:24).

`src/components/layout/AppSidebar.tsx` : entrée de nav vers `/sync`, icône `RefreshCw` de lucide-react, label `t("nav.sync")` — même motif que l'entrée existante qui navigue vers `/browser`.

- [ ] **Step 4: Déclencheurs**

`src/views/UnlockView.tsx` : au succès du déverrouillage (là où la navigation post-unlock se fait), ajouter un fire-and-forget :

```ts
void tauriCommand("sync_now").catch(() => {});
```

`src/components/layout/AppShell.tsx` : boucle périodique tant que l'app est déverrouillée :

```tsx
useEffect(() => {
  const id = setInterval(() => {
    void tauriCommand("sync_now").catch(() => {});
  }, 60_000);
  return () => clearInterval(id);
}, []);
```

et l'indicateur : consommer `useSyncStatus()` ; si `state === "offline"` ou `"error"`, afficher un badge discret (point ambre + `t("sync.notSynced")`, `title={detail}`) dans le header/footer du shell. Aucun rendu si la sync n'est pas configurée (`not_configured`) ou OK.

- [ ] **Step 5: i18n**

`en.json` (et traduction française dans `fr.json`) — clés à ajouter :

```json
"nav": { "sync": "Sync" },
"sync": {
  "title": "Server sync",
  "serverUrl": "Server URL",
  "token": "Access token",
  "tokenKept": "Token saved — leave empty to keep it",
  "enable": "Enable sync",
  "syncNow": "Sync now",
  "notSynced": "Not synced",
  "lastSync": "Last sync",
  "serverVersion": "Server version",
  "state": {
    "not_configured": "Not configured",
    "idle": "Idle",
    "syncing": "Syncing…",
    "synced": "Synced",
    "offline": "Server unreachable",
    "error": "Error"
  }
}
```

(Fusionner dans les objets `nav`/racine existants, ne pas dupliquer les clés.)

- [ ] **Step 6: Vérifier**

```powershell
npm run lint
npm run build
npm run dev   # vérif manuelle rapide en mock : la vue /sync s'affiche, état "Not configured"
```

Expected: lint 0 warning, build vert, vue fonctionnelle en mock.

- [ ] **Step 7: Commit**

```powershell
git add src/lib/tauri.ts src/views/SyncView.tsx src/App.tsx src/components/layout/AppSidebar.tsx src/components/layout/AppShell.tsx src/views/UnlockView.tsx src/i18n/en.json src/i18n/fr.json
git commit -m "feat(ui): sync settings view, status indicator, unlock + periodic triggers

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 6: E2E manuel contre le serveur de prod

**Files:** aucun (vérification). Consigner les résultats dans le rapport de tâche.

Pré-requis : `npm run tauri dev` (vrai backend Rust), le token du serveur (détenu par Luca — la tâche s'exécute AVEC lui), et `curl` pour observer côté serveur.

- [ ] **Step 1: Configuration** — ouvrir `/sync` dans l'app, saisir `http://100.64.46.117:8787` + le token, activer, « Synchroniser maintenant ». Attendu : état « Synchronisé », version serveur affichée. Vérifier côté serveur que la version a AVANCÉ d'exactement 1 si l'app a poussé (fusion coffre local + v1 déjà sur le serveur), sinon inchangée :

```powershell
curl.exe -s -H "Authorization: Bearer <TOKEN>" http://100.64.46.117:8787/api/vault/versions
```

- [ ] **Step 2: Push après modification** — créer une entrée `e2e-sync-test` dans l'app, attendre ≤ 60 s (boucle) ou cliquer « Synchroniser maintenant ». Attendu : version serveur +1.

- [ ] **Step 3: Conflit (409)** — bumper artificiellement la version serveur pour périmer l'etag local :

```powershell
curl.exe -s -o vault.tmp -H "Authorization: Bearer <TOKEN>" http://100.64.46.117:8787/api/vault
curl.exe -s -w "%{http_code}" -X PUT -H "Authorization: Bearer <TOKEN>" -H "If-Match: \"<VERSION_COURANTE>\"" --data-binary "@vault.tmp" http://100.64.46.117:8787/api/vault
```

puis modifier une entrée dans l'app et synchroniser. Attendu : la sync converge sans erreur (boucle 409 → re-GET → merge → re-PUT), version finale = bump + 1, aucune donnée perdue.

- [ ] **Step 4: Hors-ligne silencieux** — couper Tailscale (ou débrancher le réseau), modifier une entrée, synchroniser. Attendu : badge « Non synchronisé », app pleinement utilisable, aucune popup d'erreur. Reconnecter → sync suivante verte, modification poussée.

- [ ] **Step 5: Pull** — supprimer l'entrée `e2e-sync-test` dans l'app, sync (version +1). Puis restaurer une ancienne version pour simuler un deuxième appareil en retard :

```powershell
curl.exe -s -o old.tmp -H "Authorization: Bearer <TOKEN>" http://100.64.46.117:8787/api/vault/versions/<VERSION_AVANT_SUPPRESSION>
curl.exe -s -X PUT -H "Authorization: Bearer <TOKEN>" -H "If-Match: \"<VERSION_COURANTE>\"" --data-binary "@old.tmp" http://100.64.46.117:8787/api/vault
```

sync dans l'app. Attendu : l'entrée `e2e-sync-test` NE revient PAS (tombstone plus récente que sa dernière modification) — c'est le test « delete vs modify » en conditions réelles.

- [ ] **Step 6: Nettoyage** — supprimer `vault.tmp`/`old.tmp`, vérifier `curl .../api/vault/versions` cohérent, consigner tous les résultats.

---

## Vérification finale du sous-projet

```powershell
cd crates/mypass-core && cargo test && cargo clippy && cargo check --target wasm32-unknown-unknown
cd ../../src-tauri && cargo test && cargo clippy
cd .. && npm run lint && npm run build
```

Critère de sortie (spec, « Découpage », point 3) : sync desktop fonctionnelle contre le serveur de prod, fusion validée par les 8 tests unitaires + le E2E manuel (conflit, hors-ligne, suppression). Restent hors scope : app web/PWA (sous-projet 4).

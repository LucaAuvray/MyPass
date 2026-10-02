/// Stockage versionné du coffre : blobs opaques `vault.v{n}.kdbx` + `index.json`.
/// Écritures atomiques (tmp + rename). Le serveur ne déchiffre jamais rien.
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Écrit `data` dans `tmp`, force la synchronisation disque, puis renomme
/// vers `dest`. Le `sync_all` protège contre une coupure de courant (pas
/// seulement un crash process) : sans lui, le rename peut être persisté
/// avant les données du fichier.
fn write_atomic(tmp: &Path, dest: &Path, data: &[u8]) -> io::Result<()> {
    let mut f = fs::File::create(tmp)?;
    f.write_all(data)?;
    f.sync_all()?;
    fs::rename(tmp, dest)
}

pub const RETENTION: u64 = 50;

#[derive(Serialize, Deserialize)]
struct Index {
    current: u64,
}

pub struct VaultStore {
    dir: PathBuf,
}

impl VaultStore {
    pub fn new(dir: impl Into<PathBuf>) -> io::Result<Self> {
        let dir = dir.into();
        fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    fn index_path(&self) -> PathBuf {
        self.dir.join("index.json")
    }

    fn vault_path(&self, version: u64) -> PathBuf {
        self.dir.join(format!("vault.v{version}.kdbx"))
    }

    pub fn current_version(&self) -> io::Result<Option<u64>> {
        match fs::read_to_string(self.index_path()) {
            Ok(s) => {
                let idx: Index = serde_json::from_str(&s)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                Ok(Some(idx.current))
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn read_current(&self) -> io::Result<Option<(u64, Vec<u8>)>> {
        match self.current_version()? {
            Some(v) => Ok(self.read_version(v)?.map(|data| (v, data))),
            None => Ok(None),
        }
    }

    pub fn read_version(&self, version: u64) -> io::Result<Option<Vec<u8>>> {
        match fs::read(self.vault_path(version)) {
            Ok(data) => Ok(Some(data)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn write_new_version(&self, data: &[u8]) -> io::Result<u64> {
        let next = self.current_version()?.unwrap_or(0) + 1;

        // Blob d'abord, index ensuite : si on crashe entre les deux,
        // l'index pointe toujours sur une version complète.
        let vault_tmp = self.dir.join(format!("vault.v{next}.kdbx.tmp"));
        write_atomic(&vault_tmp, &self.vault_path(next), data)?;

        // Crash entre les deux renames : blob v{next} orphelin, listé dans
        // l'historique mais current=n ; auto-réparé au prochain PUT (même
        // nom de fichier réécrit).
        let index_tmp = self.dir.join("index.json.tmp");
        let index_data = serde_json::to_string(&Index { current: next })?;
        write_atomic(&index_tmp, &self.index_path(), index_data.as_bytes())?;

        // Le PUT est déjà committé (blob + index renommés) : une erreur de
        // prune ne doit pas se traduire par un 500 côté client.
        if let Err(e) = self.prune(next) {
            tracing::warn!("prune après écriture v{next}: {e}");
        }
        Ok(next)
    }

    fn prune(&self, current: u64) -> io::Result<()> {
        for (version, _) in self.list_versions()? {
            if current.saturating_sub(version) >= RETENTION {
                fs::remove_file(self.vault_path(version))?;
            }
        }
        Ok(())
    }

    pub fn list_versions(&self) -> io::Result<Vec<(u64, u64)>> {
        let mut out = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(v) = name
                .strip_prefix("vault.v")
                .and_then(|s| s.strip_suffix(".kdbx"))
                .and_then(|s| s.parse::<u64>().ok())
            {
                out.push((v, entry.metadata()?.len()));
            }
        }
        out.sort_unstable();
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn empty_store_has_no_version() {
        let dir = tempdir().unwrap();
        let s = VaultStore::new(dir.path().join("v")).unwrap();
        assert_eq!(s.current_version().unwrap(), None);
        assert!(s.read_current().unwrap().is_none());
        assert!(s.list_versions().unwrap().is_empty());
    }

    #[test]
    fn write_then_read_roundtrip_and_versions_increment() {
        let dir = tempdir().unwrap();
        let s = VaultStore::new(dir.path().join("v")).unwrap();
        assert_eq!(s.write_new_version(b"one").unwrap(), 1);
        assert_eq!(s.write_new_version(b"two").unwrap(), 2);
        assert_eq!(s.current_version().unwrap(), Some(2));
        let (v, data) = s.read_current().unwrap().unwrap();
        assert_eq!((v, data.as_slice()), (2, b"two".as_slice()));
        assert_eq!(s.read_version(1).unwrap().unwrap(), b"one");
        assert!(s.read_version(99).unwrap().is_none());
    }

    #[test]
    fn list_versions_reports_sizes_ascending() {
        let dir = tempdir().unwrap();
        let s = VaultStore::new(dir.path().join("v")).unwrap();
        s.write_new_version(b"a").unwrap();
        s.write_new_version(b"bb").unwrap();
        assert_eq!(s.list_versions().unwrap(), vec![(1, 1), (2, 2)]);
    }

    #[test]
    fn retention_prunes_old_versions() {
        let dir = tempdir().unwrap();
        let s = VaultStore::new(dir.path().join("v")).unwrap();
        for _ in 0..(RETENTION + 3) {
            s.write_new_version(b"x").unwrap();
        }
        let versions = s.list_versions().unwrap();
        assert_eq!(versions.len(), RETENTION as usize);
        assert_eq!(versions.first().unwrap().0, 4); // 1..=3 élaguées
        assert_eq!(versions.last().unwrap().0, RETENTION + 3);
        // les fichiers élagués sont bien partis du disque
        assert!(s.read_version(3).unwrap().is_none());
    }

    #[test]
    fn no_stray_tmp_files_after_write() {
        let dir = tempdir().unwrap();
        let s = VaultStore::new(dir.path().join("v")).unwrap();
        s.write_new_version(b"data").unwrap();
        let stray: Vec<_> = std::fs::read_dir(dir.path().join("v"))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|n| n.ends_with(".tmp"))
            .collect();
        assert!(stray.is_empty(), "tmp restants: {stray:?}");
    }
}

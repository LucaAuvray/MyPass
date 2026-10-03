/// Import / export formats, both ways, shared by desktop and wasm: MyPass JSON
/// (lossless) and CSV (read by header names, so Google, Apple, KeePassXC and
/// Bitwarden exports land in the right fields).
use crate::ops::entries::{apply_custom_fields, find_entry_mut, set_string_field, set_string_field_protected};
use crate::totp::{entry_otp_uri, normalize};
use crate::xml::{Entry, Group, KeePassFile};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// One entry as it travels: MyPass JSON file, IPC and wasm all use this shape.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ImportedEntry {
    /// Folder path from the root, segments joined by `/`; `""` is the root.
    pub group: String,
    pub title: String,
    pub username: String,
    pub password: String,
    pub url: String,
    pub notes: String,
    pub tags: Vec<String>,
    /// `otpauth://` URI, or `""`.
    pub totp: String,
    /// Every other text field of the entry (card, identity, SSH, passkey…).
    pub custom_fields: BTreeMap<String, String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ExportFormat {
    Json,
    Csv,
}

impl ExportFormat {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "json" => Ok(Self::Json),
            "csv" => Ok(Self::Csv),
            _ => Err("UNKNOWN_FORMAT".to_string()),
        }
    }
}

/// Fields carried by ImportedEntry's own members rather than by customFields.
const STANDARD_KEYS: [&str; 8] = ["Title", "UserName", "Password", "URL", "Notes", "otp", "TOTP Seed", "TOTP Settings"];
const CSV_HEADER: [&str; 8] = ["Group", "Title", "Username", "Password", "URL", "Notes", "TOTP", "Tags"];

#[derive(Serialize, Deserialize)]
struct ExportFile {
    format: String,
    version: u32,
    entries: Vec<ImportedEntry>,
}

/// Export the whole tree (or only `uuids` when not empty), recycle bin excluded.
pub fn export(kf: &KeePassFile, format: ExportFormat, uuids: &[String]) -> Result<String, String> {
    let mut entries = Vec::new();
    collect(&kf.root.group, "", kf.meta.recycle_bin_uuid.as_deref(), uuids, &mut entries);
    match format {
        ExportFormat::Json => serde_json::to_string_pretty(&ExportFile {
            format: "mypass".to_string(),
            version: 1,
            entries,
        })
        .map_err(|e| e.to_string()),
        ExportFormat::Csv => {
            let mut w = csv::Writer::from_writer(Vec::new());
            w.write_record(CSV_HEADER).map_err(|e| e.to_string())?;
            for e in &entries {
                let tags = e.tags.join(",");
                w.write_record([&e.group, &e.title, &e.username, &e.password, &e.url, &e.notes, &e.totp, &tags])
                    .map_err(|e| e.to_string())?;
            }
            let bytes = w.into_inner().map_err(|e| e.to_string())?;
            String::from_utf8(bytes).map_err(|e| e.to_string())
        }
    }
}

fn collect(group: &Group, path: &str, bin: Option<&str>, uuids: &[String], out: &mut Vec<ImportedEntry>) {
    for e in &group.entries {
        if uuids.is_empty() || uuids.contains(&e.uuid) {
            out.push(to_imported(e, path));
        }
    }
    for child in &group.groups {
        if Some(child.uuid.as_str()) == bin {
            continue;
        }
        let child_path = if path.is_empty() { child.name.clone() } else { format!("{path}/{}", child.name) };
        collect(child, &child_path, bin, uuids, out);
    }
}

fn to_imported(e: &Entry, group: &str) -> ImportedEntry {
    ImportedEntry {
        group: group.to_string(),
        title: e.title().to_string(),
        username: e.username().to_string(),
        password: e.password().to_string(),
        url: e.url().to_string(),
        notes: e.notes().to_string(),
        tags: split_tags(e.tags.as_deref().unwrap_or("")),
        totp: entry_otp_uri(e).unwrap_or_default(),
        custom_fields: e
            .strings
            .iter()
            .filter(|s| !STANDARD_KEYS.contains(&s.key.as_str()))
            .map(|s| (s.key.clone(), s.value.content.clone()))
            .collect(),
    }
}

fn split_tags(s: &str) -> Vec<String> {
    s.split([',', ';']).map(str::trim).filter(|t| !t.is_empty()).map(String::from).collect()
}

/// Read a MyPass JSON export (content starting with `{`) or a CSV with a header row.
pub fn parse_import(content: &str) -> Result<Vec<ImportedEntry>, String> {
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    let raw = if content.trim_start().starts_with('{') { parse_json(content)? } else { parse_csv(content)? };
    let entries: Vec<ImportedEntry> = raw.into_iter().filter_map(tidy).collect();
    if entries.is_empty() {
        return Err("IMPORT_EMPTY".to_string());
    }
    Ok(entries)
}

/// Drop rows with nothing in them, fall back to the URL as title, normalize TOTP.
fn tidy(mut e: ImportedEntry) -> Option<ImportedEntry> {
    if e.title.is_empty() && e.username.is_empty() && e.password.is_empty() && e.url.is_empty() {
        return None;
    }
    if e.title.is_empty() {
        e.title = e.url.clone();
    }
    e.totp = normalize(&e.totp, &e.title);
    Some(e)
}

fn parse_json(content: &str) -> Result<Vec<ImportedEntry>, String> {
    let file: ExportFile = serde_json::from_str(content).map_err(|_| "JSON_NOT_MYPASS".to_string())?;
    if file.format != "mypass" || file.version != 1 {
        return Err("JSON_NOT_MYPASS".to_string());
    }
    Ok(file.entries)
}

fn parse_csv(content: &str) -> Result<Vec<ImportedEntry>, String> {
    let first = content.lines().next().unwrap_or("");
    let delimiter = if first.matches(';').count() > first.matches(',').count() { b';' } else { b',' };
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .delimiter(delimiter)
        .from_reader(content.as_bytes());
    let headers: Vec<String> = reader
        .headers()
        .map_err(|_| "CSV_NO_HEADER".to_string())?
        .iter()
        .map(|h| h.trim().to_lowercase())
        .collect();
    let col = |names: &[&str]| headers.iter().position(|h| names.contains(&h.as_str()));
    let title = col(&["title", "name", "account"]);
    let username = col(&["username", "user name", "login", "login_username", "user"]);
    let password = col(&["password", "login_password"]);
    let url = col(&["url", "uri", "website", "web site", "login_uri"]);
    let notes = col(&["notes", "note", "comments", "extra"]);
    let totp = col(&["totp", "otp", "otpauth", "login_totp"]);
    let group = col(&["group", "folder"]);
    let tags = col(&["tags"]);
    if title.is_none() && username.is_none() && password.is_none() && url.is_none() {
        return Err("CSV_NO_HEADER".to_string());
    }

    let mut out = Vec::new();
    for record in reader.records() {
        let Ok(record) = record else { continue };
        let get = |c: Option<usize>| c.and_then(|i| record.get(i)).unwrap_or("").to_string();
        out.push(ImportedEntry {
            group: get(group),
            title: get(title),
            username: get(username),
            password: get(password),
            url: get(url),
            notes: get(notes),
            tags: split_tags(&get(tags)),
            totp: get(totp),
            custom_fields: BTreeMap::new(),
        });
    }
    Ok(out)
}


/// An entry after deduplication: `replaceUuid` names the vault entry it overwrites.
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedImport {
    #[serde(flatten)]
    pub entry: ImportedEntry,
    #[serde(default)]
    pub replace_uuid: Option<String>,
}

#[derive(Serialize, Debug, PartialEq, Default)]
pub struct ImportResult {
    pub imported: usize,
    pub updated: usize,
    pub skipped: usize,
}

/// Write imported entries: over their vault twin when `replaceUuid` still
/// exists, otherwise as new entries (always new UUIDs) in their folder.
pub fn apply_import(kf: &mut KeePassFile, entries: Vec<ResolvedImport>) -> Result<ImportResult, String> {
    let mut result = ImportResult::default();
    for ResolvedImport { entry, replace_uuid } in entries {
        if entry.title.is_empty() && entry.username.is_empty() && entry.password.is_empty() && entry.url.is_empty() {
            result.skipped += 1;
            continue;
        }
        if let Some(existing) = replace_uuid.as_deref().and_then(|u| find_entry_mut(&mut kf.root.group, u).ok()) {
            write_fields(existing, &entry);
            existing.times.touch();
            result.updated += 1;
        } else {
            let mut created = Entry::new(&entry.title, &entry.username, &entry.password, &entry.url);
            write_fields(&mut created, &entry);
            ensure_group(&mut kf.root.group, &entry.group).entries.push(created);
            result.imported += 1;
        }
    }
    Ok(result)
}

/// Only non-empty values are written: an empty value in the file never erases
/// what a vault entry has (a Google CSV cannot carry TOTP, tags or folders).
fn write_fields(target: &mut Entry, e: &ImportedEntry) {
    for (key, value) in [("Title", &e.title), ("UserName", &e.username), ("URL", &e.url), ("Notes", &e.notes)] {
        if !value.is_empty() {
            set_string_field(target, key, value);
        }
    }
    if !e.password.is_empty() {
        set_string_field_protected(target, "Password", &e.password);
    }
    if !e.tags.is_empty() {
        target.tags = Some(e.tags.join(","));
    }
    let otp = normalize(&e.totp, &e.title);
    if !otp.is_empty() {
        set_string_field_protected(target, "otp", &otp);
    }
    // apply_custom_fields deletes a key given an empty value: keep those out.
    let fields: HashMap<String, String> =
        e.custom_fields.iter().filter(|(_, v)| !v.is_empty()).map(|(k, v)| (k.clone(), v.clone())).collect();
    apply_custom_fields(target, &fields);
}

/// The group at `path` under `root`, each missing segment created once.
fn ensure_group<'a>(root: &'a mut Group, path: &str) -> &'a mut Group {
    let mut group = root;
    for name in path.split('/').map(str::trim).filter(|s| !s.is_empty()) {
        let i = match group.groups.iter().position(|g| g.name == name) {
            Some(i) => i,
            None => {
                group.groups.push(Group::new(name));
                group.groups.len() - 1
            }
        };
        group = &mut group.groups[i];
    }
    group
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::entries::set_string_field;
    use crate::xml::{Entry, Group, KeePassFile};

    fn one(content: &str) -> ImportedEntry {
        let mut v = parse_import(content).unwrap();
        assert_eq!(v.len(), 1, "expected one entry, got {v:?}");
        v.remove(0)
    }

    #[test]
    fn csv_google_headers() {
        let e = one("name,url,username,password,note\nGmail,https://mail.google.com,me@x.io,pw1,hello");
        assert_eq!(e.title, "Gmail");
        assert_eq!(e.url, "https://mail.google.com");
        assert_eq!(e.username, "me@x.io");
        assert_eq!(e.password, "pw1");
        assert_eq!(e.notes, "hello");
    }

    #[test]
    fn csv_apple_headers_with_otpauth() {
        let e = one("Title,URL,Username,Password,Notes,OTPAuth\nA,https://a.io,u,p,,otpauth://totp/A?secret=JBSWY3DPEHPK3PXP");
        assert_eq!(e.totp, "otpauth://totp/A?secret=JBSWY3DPEHPK3PXP");
        assert_eq!(e.title, "A");
    }

    #[test]
    fn csv_keepassxc_headers_with_group() {
        let e = one("Group,Title,Username,Password,URL,Notes,TOTP,Icon,Last Modified,Created\nPerso/Banque,Banque,me,pw,https://b.fr,,,0,2026-01-01,2026-01-01");
        assert_eq!(e.group, "Perso/Banque");
        assert_eq!(e.title, "Banque");
        assert_eq!(e.url, "https://b.fr");
    }

    #[test]
    fn csv_bitwarden_headers() {
        let e = one("folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp\nTravail,,login,Jira,,,0,https://jira.io,bob,pw,JBSWY3DPEHPK3PXP");
        assert_eq!(e.title, "Jira");
        assert_eq!(e.url, "https://jira.io");
        assert_eq!(e.username, "bob");
        assert_eq!(e.password, "pw");
        assert_eq!(e.group, "Travail");
        assert_eq!(e.totp, "otpauth://totp/Jira?secret=JBSWY3DPEHPK3PXP");
    }

    #[test]
    fn csv_quoted_multiline_and_escaped_quotes() {
        let e = one("name,url,username,password,note\n\"Dit \"\"bonjour\"\"\",https://a.io,u,p,\"ligne 1\nligne 2\"");
        assert_eq!(e.title, "Dit \"bonjour\"");
        assert_eq!(e.notes, "ligne 1\nligne 2");
    }

    #[test]
    fn csv_bom_semicolon_crlf() {
        let e = one("\u{feff}name;url;username;password\r\nA;https://a.io;u;p\r\n");
        assert_eq!(e.title, "A");
        assert_eq!(e.url, "https://a.io");
        assert_eq!(e.username, "u");
        assert_eq!(e.password, "p");
    }

    #[test]
    fn csv_unknown_headers_error() {
        assert_eq!(parse_import("foo,bar\n1,2").unwrap_err(), "CSV_NO_HEADER");
    }

    #[test]
    fn csv_header_only_is_empty() {
        assert_eq!(parse_import("name,url,username,password\n").unwrap_err(), "IMPORT_EMPTY");
    }

    #[test]
    fn json_not_mypass_errors() {
        for bad in [
            "{\"items\":[]}",
            "{\"format\":\"mypass\",\"version\":2,\"entries\":[]}",
            "{oops",
        ] {
            assert_eq!(parse_import(bad).unwrap_err(), "JSON_NOT_MYPASS", "input: {bad}");
        }
    }


    fn vault_with_one_in_perso() -> (KeePassFile, String) {
        let mut kf = KeePassFile::new("T");
        let mut perso = Group::new("Perso");
        let mut e = Entry::new("Site", "me", "pw", "https://s.io");
        set_string_field(&mut e, "otp", "otpauth://totp/Site?secret=AB");
        set_string_field(&mut e, "MyPass_Type", "card");
        e.tags = Some("a,b".to_string());
        let uuid = e.uuid.clone();
        perso.entries.push(e);
        kf.root.group.groups.push(perso);
        (kf, uuid)
    }

    #[test]
    fn export_csv_header_and_row() {
        let (kf, _) = vault_with_one_in_perso();
        let csv = export(&kf, ExportFormat::Csv, &[]).unwrap();
        let mut lines = csv.lines();
        assert_eq!(lines.next(), Some("Group,Title,Username,Password,URL,Notes,TOTP,Tags"));
        assert!(lines.next().unwrap().starts_with("Perso,"));
    }

    #[test]
    fn export_json_shape() {
        let (kf, _) = vault_with_one_in_perso();
        let v: serde_json::Value = serde_json::from_str(&export(&kf, ExportFormat::Json, &[]).unwrap()).unwrap();
        assert_eq!(v["format"], "mypass");
        assert_eq!(v["version"], 1);
        assert_eq!(v["entries"][0]["customFields"]["MyPass_Type"], "card");
        assert_eq!(v["entries"][0]["totp"], "otpauth://totp/Site?secret=AB");
        assert_eq!(v["entries"][0]["group"], "Perso");
        assert_eq!(v["entries"][0]["tags"], serde_json::json!(["a", "b"]));
        assert!(v["entries"][0]["customFields"].get("otp").is_none());
    }

    #[test]
    fn export_skips_recycle_bin() {
        let (mut kf, _) = vault_with_one_in_perso();
        let mut bin = Group::new("Recycle Bin");
        bin.entries.push(Entry::new("Old", "x", "y", "https://old.io"));
        kf.meta.recycle_bin_uuid = Some(bin.uuid.clone());
        kf.root.group.groups.push(bin);
        let json = export(&kf, ExportFormat::Json, &[]).unwrap();
        assert!(!json.contains("old.io"));
        assert!(json.contains("s.io"));
    }

    #[test]
    fn export_filters_by_uuid() {
        let (mut kf, uuid) = vault_with_one_in_perso();
        kf.root.group.entries.push(Entry::new("Other", "o", "o", "https://other.io"));
        let json = export(&kf, ExportFormat::Json, &[uuid]).unwrap();
        assert!(json.contains("s.io"));
        assert!(!json.contains("other.io"));
    }

    // ── apply_import ─────────────────────────────────────────────

    fn resolved(v: Vec<ImportedEntry>) -> Vec<ResolvedImport> {
        v.into_iter().map(|entry| ResolvedImport { entry, replace_uuid: None }).collect()
    }

    fn entry_with(title: &str, fields: &[(&str, &str)]) -> Entry {
        let mut e = Entry::new(title, &format!("{title}-user"), &format!("{title}-pw"), &format!("https://{title}.io"));
        for (k, v) in fields {
            set_string_field(&mut e, k, v);
        }
        e
    }

    /// Seven kinds of entries, nested folders, every field MyPass knows.
    fn rich_vault() -> KeePassFile {
        let mut kf = KeePassFile::new("A");
        let mut login = entry_with("banque", &[("otp", "otpauth://totp/banque?secret=JBSWY3DPEHPK3PXP"), ("Notes", "l1\nl2")]);
        login.tags = Some("perso,web".to_string());
        let mut banque = Group::new("Banque");
        banque.entries.push(login);
        let mut perso = Group::new("Perso");
        perso.groups.push(banque);
        perso.entries.push(entry_with("visa", &[("MyPass_Type", "card"), ("CC_Number", "4242424242424242"), ("CC_CVC", "123")]));
        kf.root.group.groups.push(perso);
        kf.root.group.entries.push(entry_with("moi", &[("MyPass_Type", "identity"), ("ID_FirstName", "Luca")]));
        kf.root.group.entries.push(entry_with("passeport", &[("MyPass_Type", "document"), ("DOC_Number", "X123")]));
        kf.root.group.entries.push(entry_with("serveur", &[("MyPass_Type", "ssh_key"), ("SSH_PrivateKey", "-----BEGIN KEY-----"), ("SSH_PublicKey", "ssh-ed25519 AAA")]));
        kf.root.group.entries.push(entry_with("github", &[("KPEX_PASSKEY_CREDENTIAL_ID", "cred-id")]));
        kf.root.group.entries.push(entry_with("racine", &[]));
        kf
    }

    fn exported_sorted(kf: &KeePassFile) -> Vec<ImportedEntry> {
        let mut v = parse_import(&export(kf, ExportFormat::Json, &[]).unwrap()).unwrap();
        v.sort_by(|a, b| a.title.cmp(&b.title));
        v
    }

    fn find_by_title<'a>(g: &'a Group, title: &str) -> Option<&'a Entry> {
        g.entries.iter().find(|e| e.title() == title).or_else(|| g.groups.iter().find_map(|c| find_by_title(c, title)))
    }

    fn raw<'a>(e: &'a Entry, key: &str) -> Option<&'a str> {
        e.strings.iter().find(|s| s.key == key).map(|s| s.value.content.as_str())
    }

    fn all_uuids(g: &Group, out: &mut Vec<String>) {
        out.extend(g.entries.iter().map(|e| e.uuid.clone()));
        g.groups.iter().for_each(|c| all_uuids(c, out));
    }

    #[test]
    fn roundtrip_json_keeps_every_field() {
        let a = rich_vault();
        let mut b = KeePassFile::new("B");
        let json = export(&a, ExportFormat::Json, &[]).unwrap();
        let result = apply_import(&mut b, resolved(parse_import(&json).unwrap())).unwrap();
        assert_eq!(result, ImportResult { imported: 7, updated: 0, skipped: 0 });
        assert_eq!(exported_sorted(&a), exported_sorted(&b));
        // Read B's raw fields too, so an export that drops a field cannot hide behind itself.
        let banque = find_by_title(&b.root.group, "banque").unwrap();
        assert_eq!(raw(banque, "otp"), Some("otpauth://totp/banque?secret=JBSWY3DPEHPK3PXP"));
        assert_eq!(raw(banque, "Notes"), Some("l1\nl2"));
        assert_eq!(banque.tags.as_deref(), Some("perso,web"));
        let perso = b.root.group.groups.iter().find(|g| g.name == "Perso").unwrap();
        assert!(perso.groups.iter().any(|g| g.name == "Banque" && g.entries.len() == 1));
        assert_eq!(raw(find_by_title(&b.root.group, "visa").unwrap(), "CC_Number"), Some("4242424242424242"));
        assert_eq!(raw(find_by_title(&b.root.group, "github").unwrap(), "KPEX_PASSKEY_CREDENTIAL_ID"), Some("cred-id"));
        assert_eq!(raw(find_by_title(&b.root.group, "serveur").unwrap(), "SSH_PrivateKey"), Some("-----BEGIN KEY-----"));
    }

    #[test]
    fn roundtrip_csv_keeps_its_columns() {
        let a = rich_vault();
        let mut c = KeePassFile::new("C");
        let csv = export(&a, ExportFormat::Csv, &[]).unwrap();
        apply_import(&mut c, resolved(parse_import(&csv).unwrap())).unwrap();
        let strip = |v: Vec<ImportedEntry>| -> Vec<ImportedEntry> {
            v.into_iter().map(|mut e| { e.custom_fields.clear(); e }).collect()
        };
        assert_eq!(strip(exported_sorted(&a)), strip(exported_sorted(&c)));
    }

    #[test]
    fn apply_import_creates_group_once() {
        let mut kf = KeePassFile::new("T");
        let mk = |t: &str| ImportedEntry { group: "Perso/Banque".into(), title: t.into(), ..Default::default() };
        apply_import(&mut kf, resolved(vec![mk("a"), mk("b")])).unwrap();
        assert_eq!(kf.root.group.groups.len(), 1);
        let perso = &kf.root.group.groups[0];
        assert_eq!(perso.name, "Perso");
        assert_eq!(perso.groups.len(), 1);
        assert_eq!(perso.groups[0].name, "Banque");
        assert_eq!(perso.groups[0].entries.len(), 2);
    }

    #[test]
    fn apply_import_replace_updates_in_place() {
        let mut kf = KeePassFile::new("T");
        let mut e = Entry::new("Site", "me", "old", "https://s.io");
        e.times.last_modification_time = Some("2020-01-01T00:00:00Z".to_string());
        let uuid = e.uuid.clone();
        kf.root.group.entries.push(e);
        let incoming = ImportedEntry { title: "Site".into(), username: "me".into(), password: "new".into(), url: "https://s.io".into(), ..Default::default() };
        let result = apply_import(&mut kf, vec![ResolvedImport { entry: incoming, replace_uuid: Some(uuid.clone()) }]).unwrap();
        assert_eq!(result, ImportResult { imported: 0, updated: 1, skipped: 0 });
        assert_eq!(kf.root.group.entries.len(), 1);
        let e = &kf.root.group.entries[0];
        assert_eq!(e.uuid, uuid);
        assert_eq!(e.password(), "new");
        assert!(e.times.last_modification_time.as_deref().unwrap() > "2020-01-01T00:00:00Z");
    }

    #[test]
    fn apply_import_unknown_replace_creates() {
        let mut kf = KeePassFile::new("T");
        let incoming = ImportedEntry { title: "Site".into(), ..Default::default() };
        let result = apply_import(&mut kf, vec![ResolvedImport { entry: incoming, replace_uuid: Some("gone".into()) }]).unwrap();
        assert_eq!(result, ImportResult { imported: 1, updated: 0, skipped: 0 });
        assert_eq!(kf.root.group.entries.len(), 1);
    }

    #[test]
    fn apply_import_never_reuses_uuids() {
        let a = rich_vault();
        let mut b = KeePassFile::new("B");
        apply_import(&mut b, resolved(parse_import(&export(&a, ExportFormat::Json, &[]).unwrap()).unwrap())).unwrap();
        let (mut ua, mut ub) = (Vec::new(), Vec::new());
        all_uuids(&a.root.group, &mut ua);
        all_uuids(&b.root.group, &mut ub);
        assert_eq!(ub.len(), 7);
        assert!(ub.iter().all(|u| !ua.contains(u)));
    }

    /// "Keep the imported one" from a CSV without TOTP, tags or notes (Google)
    /// must not wipe what the vault entry has and the file cannot carry.
    #[test]
    fn apply_import_replace_keeps_what_the_file_lacks() {
        let mut kf = KeePassFile::new("T");
        let mut e = entry_with("site", &[("otp", "otpauth://totp/site?secret=AB"), ("Notes", "garde-moi")]);
        e.tags = Some("perso".to_string());
        let uuid = e.uuid.clone();
        kf.root.group.entries.push(e);
        let incoming = ImportedEntry { title: "site".into(), username: "site-user".into(), password: "new".into(), url: "https://site.io".into(), ..Default::default() };
        apply_import(&mut kf, vec![ResolvedImport { entry: incoming, replace_uuid: Some(uuid) }]).unwrap();
        let e = &kf.root.group.entries[0];
        assert_eq!(e.password(), "new");
        assert_eq!(raw(e, "otp"), Some("otpauth://totp/site?secret=AB"));
        assert_eq!(e.notes(), "garde-moi");
        assert_eq!(e.tags.as_deref(), Some("perso"));
    }

    #[test]
    fn roundtrip_from_totp_seed() {
        let mut a = KeePassFile::new("A");
        a.root.group.entries.push(entry_with("keepass2", &[("TOTP Seed", "jbsw y3dp ehpk 3pxp")]));
        let mut b = KeePassFile::new("B");
        apply_import(&mut b, resolved(parse_import(&export(&a, ExportFormat::Json, &[]).unwrap()).unwrap())).unwrap();
        let e = &b.root.group.entries[0];
        assert_eq!(raw(e, "otp"), Some("otpauth://totp/keepass2?secret=JBSWY3DPEHPK3PXP"));
    }

    #[test]
    fn export_keeps_legacy_totp_settings() {
        let mut kf = KeePassFile::new("T");
        kf.root.group.entries.push(entry_with("keepass2", &[("TOTP Seed", "JBSWY3DPEHPK3PXP"), ("TOTP Settings", "60;8")]));
        let v: serde_json::Value = serde_json::from_str(&export(&kf, ExportFormat::Json, &[]).unwrap()).unwrap();
        assert_eq!(v["entries"][0]["totp"], "otpauth://totp/keepass2?secret=JBSWY3DPEHPK3PXP&period=60&digits=8");
        assert!(v["entries"][0]["customFields"].get("TOTP Settings").is_none());
    }

    #[test]
    fn apply_import_skips_empty() {
        let mut kf = KeePassFile::new("T");
        let result = apply_import(&mut kf, resolved(vec![ImportedEntry::default()])).unwrap();
        assert_eq!(result, ImportResult { imported: 0, updated: 0, skipped: 1 });
        assert!(kf.root.group.entries.is_empty());
    }
}

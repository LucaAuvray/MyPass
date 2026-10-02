/// Import / export formats, both ways, shared by desktop and wasm: MyPass JSON
/// (lossless) and CSV (read by header names, so Google, Apple, KeePassXC and
/// Bitwarden exports land in the right fields).
use crate::xml::{Entry, Group, KeePassFile};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
const STANDARD_KEYS: [&str; 7] = ["Title", "UserName", "Password", "URL", "Notes", "otp", "TOTP Seed"];
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
    let field = |key: &str| e.strings.iter().find(|s| s.key == key).map(|s| s.value.content.clone());
    ImportedEntry {
        group: group.to_string(),
        title: e.title().to_string(),
        username: e.username().to_string(),
        password: e.password().to_string(),
        url: e.url().to_string(),
        notes: e.notes().to_string(),
        tags: split_tags(e.tags.as_deref().unwrap_or("")),
        totp: field("otp").or_else(|| field("TOTP Seed")).unwrap_or_default(),
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
    e.totp = normalize_totp(&e.totp, &e.title);
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

/// TOTP as stored in the `otp` field (KeePassXC convention): an `otpauth://`
/// URI is kept as is, a bare secret is wrapped into one.
pub(crate) fn normalize_totp(value: &str, title: &str) -> String {
    let v = value.trim();
    if v.is_empty() {
        return String::new();
    }
    if v.get(..10).is_some_and(|p| p.eq_ignore_ascii_case("otpauth://")) {
        return v.to_string();
    }
    let secret: String = v.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_uppercase();
    format!("otpauth://totp/{}?secret={secret}", percent_encode(title))
}

fn percent_encode(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
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

    #[test]
    fn totp_bare_secret_becomes_otpauth() {
        assert_eq!(
            normalize_totp("jbsw y3dp ehpk 3pxp", "Mon site"),
            "otpauth://totp/Mon%20site?secret=JBSWY3DPEHPK3PXP"
        );
        assert_eq!(normalize_totp("OTPAUTH://totp/X?secret=AB", "t"), "OTPAUTH://totp/X?secret=AB");
        assert_eq!(normalize_totp("  ", "t"), "");
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
}

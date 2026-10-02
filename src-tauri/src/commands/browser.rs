/// Tauri commands for browser integration using the KeePassXC-Browser protocol.
/// Communicates with browser extensions via Native Messaging (stdio) using NaCl box encryption.
use crate::commands::database::DbState;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::State;

/// Path to the on/off flag shared between the main app process and the
/// standalone Native Messaging Host process (`mypass --native-messaging`),
/// which is why this is a plain file rather than in-memory app state.
/// ponytail: hardcoded %APPDATA% path, matching the existing Windows-only
/// register-nhm.ps1 convention. Switch to Tauri's app_config_dir() if/when
/// macOS/Linux native messaging support is added.
fn config_path() -> Option<PathBuf> {
    let appdata = std::env::var("APPDATA").ok()?;
    Some(PathBuf::from(appdata).join("MyPass").join("browser_settings.json"))
}

/// Whether browser integration is enabled. Defaults to disabled (opt-in)
/// until the user turns it on, since it opens an IPC channel that can read
/// stored passwords.
pub fn is_enabled() -> bool {
    let Some(path) = config_path() else { return false };
    let Ok(content) = std::fs::read_to_string(path) else { return false };
    serde_json::from_str::<serde_json::Value>(&content)
        .ok()
        .and_then(|v| v.get("enabled").and_then(|e| e.as_bool()))
        .unwrap_or(false)
}

pub fn set_enabled(enabled: bool) -> Result<(), String> {
    let path = config_path().ok_or("Could not resolve config directory (%APPDATA%)")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, serde_json::json!({ "enabled": enabled }).to_string())
        .map_err(|e| e.to_string())
}

/// Registry key name browsers use to look up the host, matching
/// `register-nhm.ps1`'s `com.mypass.mypass_browser` registry entries.
const NM_HOST_NAME: &str = "com.mypass.mypass_browser";

/// (Re)generate the two files `register-nhm.ps1` points browsers at:
/// - a small wrapper script that launches the *current* build of this app
///   with `--native-messaging` (a Native Messaging manifest's "path" can't
///   carry CLI arguments, so something has to inject that flag), and
/// - the Native Messaging Host manifest itself, declaring which extensions
///   are allowed to launch it.
///
/// Safe to call on every startup: it only (re)writes inert files under
/// `%APPDATA%\MyPass`, it does not touch the registry — that one-time step
/// stays in `register-nhm.ps1`, run manually by the user.
pub fn ensure_native_messaging_manifest() {
    let Ok(appdata) = std::env::var("APPDATA") else { return };
    let dir = PathBuf::from(appdata).join("MyPass");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let Ok(exe_path) = std::env::current_exe() else { return };

    let wrapper_path = dir.join("mypass-nhm.bat");
    let wrapper = format!("@echo off\r\n\"{}\" --native-messaging\r\n", exe_path.display());
    if std::fs::write(&wrapper_path, wrapper).is_err() {
        return;
    }

    // Includes both the upstream KeePassXC-Browser extension (this project
    // reuses it rather than shipping its own, see
    // plans/browser-integration-plan.md) and MyPass's own extension/, whose
    // manifest.json now pins a "key" so `chrome://extensions` gives it this
    // same ID every time it's loaded unpacked, regardless of machine or path.
    let manifest = serde_json::json!({
        "name": NM_HOST_NAME,
        "description": "MyPass integration with native messaging support",
        "path": wrapper_path.to_string_lossy(),
        "type": "stdio",
        "allowed_origins": [
            "chrome-extension://pdffhmdngciaglkoonimfcmckehcpafo/",
            "chrome-extension://oboonakemofpalcgghocfoadofidjkkk/",
            "chrome-extension://bmeobbbilliigohcbhomfphecoocnnda/",
            // The vendored keepassxc-browser/ clone loaded unpacked from this
            // repo's path — its manifest has no "key", so Chrome derives this
            // ID from the folder path (changes if the repo moves).
            "chrome-extension://cdfcdponhpejgnglmjapmgimbmfpacgj/"
        ],
        "allowed_extensions": ["keepassxc-browser@keepassxc.org"]
    });
    let _ = std::fs::write(dir.join("mypass_browser_manifest.json"), manifest.to_string());
}

#[derive(serde::Serialize, Clone)]
pub struct BrowserStatus {
    pub connected: bool,
    pub browsers: Vec<BrowserInfo>,
    pub extension_version: String,
    pub protocol_version: String,
}

#[derive(serde::Serialize, Clone)]
pub struct BrowserInfo {
    pub name: String,
    pub connected: bool,
    pub associated: bool,
    pub last_seen: String,
}

/// Get the current browser integration status: which browsers have actually
/// associated with this vault (read from KDBX custom data, the same store
/// `native_messaging::handle_associate` writes to) and whether any of them
/// currently has a live connection to the local bridge.
#[tauri::command]
pub async fn get_browser_status(
    state: State<'_, Arc<Mutex<DbState>>>,
) -> Result<BrowserStatus, String> {
    let connected = crate::native_messaging::active_connection_count() > 0;

    let browsers = {
        let db = state.lock().map_err(|e| format!("Lock error: {e}"))?;
        match db.keepass_file.as_ref() {
            Some(kf) => kf
                .meta
                .custom_data
                .items
                .iter()
                .filter_map(|i| i.key.strip_prefix("Browser_"))
                .map(|id| BrowserInfo {
                    name: id.to_string(),
                    connected,
                    associated: true,
                    last_seen: crate::native_messaging::last_seen(id),
                })
                .collect(),
            None => Vec::new(),
        }
    };

    Ok(BrowserStatus {
        connected,
        browsers,
        extension_version: "1.10.3".to_string(),
        protocol_version: "2.0".to_string(),
    })
}

/// Check if browser integration is enabled.
#[tauri::command]
pub async fn is_browser_integration_enabled() -> Result<bool, String> {
    Ok(is_enabled())
}

/// Toggle browser integration on/off.
#[tauri::command]
pub async fn toggle_browser_integration(enabled: bool) -> Result<bool, String> {
    set_enabled(enabled)?;
    Ok(enabled)
}

/// Handle credential retrieval for browser (get-logins).
/// Called by the native messaging bridge.
pub fn handle_get_logins(
    db: &crate::kdbx::xml::KeePassFile,
    url: &str,
) -> Result<Vec<serde_json::Value>, String> {
    let entries = collect_all_entries(&db.root.group);
    let matching: Vec<serde_json::Value> = entries
        .iter()
        .filter(|e| url_matches(e, url))
        .map(|e| {
            serde_json::json!({
                "uuid": e.uuid,
                "login": e.username(),
                "name": e.title(),
                "password": e.password(),
                "url": e.url(),
            })
        })
        .collect();

    Ok(matching)
}

/// Identités et cartes exposées au navigateur pour le remplissage de
/// formulaires (action get-identities). Les documents sont volontairement
/// exclus : aucun usage autofill, surface d'exposition en moins.
pub fn handle_get_identities(db: &crate::kdbx::xml::KeePassFile) -> Vec<serde_json::Value> {
    collect_all_entries(&db.root.group)
        .iter()
        .filter_map(|e| {
            let item_type = e.get_field("MyPass_Type")?;
            if item_type != "identity" && item_type != "card" {
                return None;
            }
            let fields: serde_json::Map<String, serde_json::Value> = e
                .strings
                .iter()
                .filter(|s| s.key.starts_with("ID_") || s.key.starts_with("CC_"))
                .map(|s| (s.key.clone(), serde_json::Value::String(s.value.content.clone())))
                .collect();
            Some(serde_json::json!({
                "uuid": e.uuid,
                "title": e.title(),
                "type": item_type,
                "fields": fields,
            }))
        })
        .collect()
}

/// Handle credential storage for browser (set-login).
pub fn handle_set_login(
    kf: &mut crate::kdbx::xml::KeePassFile,
    url: &str,
    login: &str,
    password: &str,
    uuid: Option<&str>,
) -> Result<(), String> {
    if let Some(existing_uuid) = uuid {
        if let Some(entry) = find_entry_mut(&mut kf.root.group, existing_uuid) {
            set_string_field(entry, "UserName", login);
            set_string_field_protected(entry, "Password", password);
            set_string_field(entry, "URL", url);
            entry.times.touch();
            return Ok(());
        }
    }

    // Create new entry
    let entry = crate::kdbx::xml::Entry::new(url, login, password, url);
    kf.root.group.entries.push(entry);
    Ok(())
}

// =============================================================================
// Helpers
// =============================================================================

fn url_matches(entry: &crate::kdbx::xml::Entry, target_url: &str) -> bool {
    let entry_url = entry.url().to_lowercase();
    let target = target_url.to_lowercase();

    // Exact match
    if entry_url == target { return true; }

    // Domain match
    if let (Some(entry_domain), Some(target_domain)) = (extract_domain(&entry_url), extract_domain(&target)) {
        if entry_domain == target_domain { return true; }
    }

    // Contains match
    if entry_url.contains(&target) || target.contains(&entry_url) { return true; }

    false
}

fn extract_domain(url: &str) -> Option<String> {
    let url = if !url.contains("://") { format!("https://{}", url) } else { url.to_string() };
    url::Url::parse(&url).ok().map(|u| {
        u.host_str().unwrap_or("").trim_start_matches("www.").to_string()
    })
}

pub(crate) fn collect_all_entries(group: &crate::kdbx::xml::Group) -> Vec<&crate::kdbx::xml::Entry> {
    let mut result: Vec<&crate::kdbx::xml::Entry> = group.entries.iter().collect();
    for child in &group.groups {
        result.extend(collect_all_entries(child));
    }
    result
}

fn find_entry_mut<'a>(
    group: &'a mut crate::kdbx::xml::Group,
    uuid: &str,
) -> Option<&'a mut crate::kdbx::xml::Entry> {
    if let Some(entry) = group.entries.iter_mut().find(|e| e.uuid == uuid) {
        return Some(entry);
    }
    for child in &mut group.groups {
        if let Some(found) = find_entry_mut(child, uuid) {
            return Some(found);
        }
    }
    None
}

fn set_string_field(entry: &mut crate::kdbx::xml::Entry, key: &str, value: &str) {
    if let Some(s) = entry.strings.iter_mut().find(|s| s.key == key) {
        s.value.content = value.to_string();
    } else {
        entry.strings.push(crate::kdbx::xml::EntryString {
            key: key.to_string(),
            value: crate::kdbx::xml::Value {
                content: value.to_string(),
                protected: None,
            },
        });
    }
}

fn set_string_field_protected(entry: &mut crate::kdbx::xml::Entry, key: &str, value: &str) {
    if let Some(s) = entry.strings.iter_mut().find(|s| s.key == key) {
        s.value.content = value.to_string();
        s.value.protected = Some("True".to_string());
    } else {
        entry.strings.push(crate::kdbx::xml::EntryString {
            key: key.to_string(),
            value: crate::kdbx::xml::Value {
                content: value.to_string(),
                protected: Some("True".to_string()),
            },
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kdbx::ops::entries::apply_custom_fields;
    use crate::kdbx::xml::{Entry, KeePassFile};
    use std::collections::HashMap;

    fn entry_with_type(title: &str, item_type: &str, extra: &[(&str, &str)]) -> Entry {
        let mut e = Entry::new(title, "", "", "");
        let mut fields = HashMap::new();
        fields.insert("MyPass_Type".to_string(), item_type.to_string());
        for (k, v) in extra {
            fields.insert(k.to_string(), v.to_string());
        }
        apply_custom_fields(&mut e, &fields);
        e
    }

    #[test]
    fn get_identities_returns_only_identities_and_cards() {
        let mut kf = KeePassFile::new("test");
        kf.root.group.entries.push(Entry::new("Login GitHub", "dev", "pw", "https://github.com"));
        kf.root.group.entries.push(entry_with_type("Identité perso", "identity", &[("ID_FirstName", "Luca"), ("ID_Email", "luca@example.com")]));
        kf.root.group.entries.push(entry_with_type("Visa perso", "card", &[("CC_Number", "4242424242424242"), ("CC_Holder", "Luca Auvray")]));
        kf.root.group.entries.push(entry_with_type("Passeport", "document", &[("DOC_Number", "12AB34567")]));

        let result = handle_get_identities(&kf);

        assert_eq!(result.len(), 2);
        let types: Vec<&str> = result.iter().map(|v| v["type"].as_str().unwrap()).collect();
        assert!(types.contains(&"identity") && types.contains(&"card"));

        let card = result.iter().find(|v| v["type"] == "card").unwrap();
        assert_eq!(card["title"], "Visa perso");
        assert_eq!(card["fields"]["CC_Number"], "4242424242424242");
        // Aucun document ne doit fuiter
        assert!(result.iter().all(|v| v["type"] != "document"));
    }
}

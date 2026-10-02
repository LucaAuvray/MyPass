/// Tauri commands for importing and exporting passwords.
use crate::commands::database::DbState;
use crate::kdbx::xml::{Entry, Group};
use std::sync::{Arc, Mutex};
use tauri::State;

#[derive(serde::Serialize)]
pub struct ImportPreviewEntry {
    pub title: String,
    pub username: String,
    pub url: String,
    pub group: String,
    pub has_password: bool,
    pub has_totp: bool,
    pub is_duplicate: bool,
}

#[derive(serde::Serialize)]
pub struct ImportResult {
    pub imported: usize,
    pub skipped: usize,
    pub duplicates: usize,
    pub errors: Vec<String>,
}

#[derive(serde::Deserialize)]
pub struct CsvColumnMapping {
    pub title: usize,
    pub username: usize,
    pub password: usize,
    pub url: usize,
    pub notes: usize,
    // Received but not written to the vault yet (data loss on import) — roadmap sub-project 5.
    #[allow(dead_code)]
    pub totp: usize,
    pub group: usize,
    pub delimiter: String,
    pub has_header: bool,
}

/// Preview CSV import without committing.
#[tauri::command]
pub async fn preview_csv_import(
    path: String,
    mapping: CsvColumnMapping,
) -> Result<Vec<ImportPreviewEntry>, String> {
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read file: {e}"))?;

    let delimiter = if mapping.delimiter.is_empty() { b',' } else { mapping.delimiter.as_bytes()[0] };
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(mapping.has_header)
        .from_reader(content.as_bytes());

    let mut entries = Vec::new();

    for result in reader.records() {
        let record = result.map_err(|e| format!("CSV parse error: {e}"))?;

        let title = record.get(mapping.title).unwrap_or("").to_string();
        let username = record.get(mapping.username).unwrap_or("").to_string();
        let password = record.get(mapping.password).unwrap_or("").to_string();
        let url = record.get(mapping.url).unwrap_or("").to_string();

        entries.push(ImportPreviewEntry {
            title: if title.is_empty() { "Untitled".to_string() } else { title },
            username,
            url,
            group: record.get(mapping.group).unwrap_or("Imported").to_string(),
            has_password: !password.is_empty(),
            has_totp: false,
            is_duplicate: false,
        });
    }

    Ok(entries)
}

/// Import CSV entries into the open database.
#[tauri::command]
pub async fn import_csv(
    state: State<'_, Arc<Mutex<DbState>>>,
    path: String,
    mapping: CsvColumnMapping,
) -> Result<ImportResult, String> {
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read file: {e}"))?;

    let delimiter = if mapping.delimiter.is_empty() { b',' } else { mapping.delimiter.as_bytes()[0] };
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(mapping.has_header)
        .from_reader(content.as_bytes());

    let mut db = state.lock().map_err(|e| format!("Lock error: {e}"))?;
    let kf = db.keepass_file.as_mut().ok_or("No open database")?;

    let mut imported = 0;
    let mut skipped = 0;
    let mut errors = Vec::new();

    for result in reader.records() {
        match result {
            Ok(record) => {
                let title = record.get(mapping.title).unwrap_or("Untitled").to_string();
                let username = record.get(mapping.username).unwrap_or("").to_string();
                let password = record.get(mapping.password).unwrap_or("").to_string();
                let url = record.get(mapping.url).unwrap_or("").to_string();
                let notes = record.get(mapping.notes).unwrap_or("").to_string();

                if title.is_empty() && username.is_empty() {
                    skipped += 1;
                    continue;
                }

                let entry = Entry::new(&title, &username, &password, &url);
                if !notes.is_empty() {
                    // Add notes field
                }

                kf.root.group.entries.push(entry);
                imported += 1;
            }
            Err(e) => {
                errors.push(format!("CSV row error: {e}"));
                skipped += 1;
            }
        }
    }

    db.save()?;

    Ok(ImportResult {
        imported,
        skipped,
        duplicates: 0,
        errors,
    })
}

/// Import 1Password 1PUX file.
#[tauri::command]
pub async fn import_1password(
    state: State<'_, Arc<Mutex<DbState>>>,
    path: String,
) -> Result<ImportResult, String> {
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read file: {e}"))?;

    // 1PUX is a ZIP containing JSON files. For MVP, handle the simple unencrypted format.
    let data: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Invalid 1PUX JSON: {e}"))?;

    let mut db = state.lock().map_err(|e| format!("Lock error: {e}"))?;
    let kf = db.keepass_file.as_mut().ok_or("No open database")?;

    let mut imported = 0;
    let mut skipped = 0;

    if let Some(items) = data["items"].as_array() {
        for item in items {
            let title = item["title"].as_str().unwrap_or("Untitled").to_string();
            let username = item["username"].as_str().unwrap_or("").to_string();
            let password = item["password"].as_str().unwrap_or("").to_string();
            let url = item["url"].as_str().unwrap_or("").to_string();

            let entry = Entry::new(&title, &username, &password, &url);
            kf.root.group.entries.push(entry);
            imported += 1;
        }
    } else {
        skipped += 1;
    }

    db.save()?;

    Ok(ImportResult { imported, skipped, duplicates: 0, errors: vec![] })
}

/// Import Bitwarden JSON export.
#[tauri::command]
pub async fn import_bitwarden(
    state: State<'_, Arc<Mutex<DbState>>>,
    path: String,
) -> Result<ImportResult, String> {
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read file: {e}"))?;

    let data: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Invalid Bitwarden JSON: {e}"))?;

    let mut db = state.lock().map_err(|e| format!("Lock error: {e}"))?;
    let kf = db.keepass_file.as_mut().ok_or("No open database")?;

    let mut imported = 0;
    let mut skipped = 0;

    // Bitwarden format: { "items": [ { "name": "...", "login": { "username": "...", "password": "...", "uris": [...] } } ] }
    let items = if let Some(encrypted) = data["encrypted"].as_bool() {
        if encrypted {
            return Err("Encrypted Bitwarden exports are not yet supported. Please export unencrypted.".to_string());
        }
        data["items"].as_array()
    } else {
        data["items"].as_array()
    };

    if let Some(items) = items {
        for item in items {
            let title = item["name"].as_str().unwrap_or("Untitled").to_string();
            let username = item["login"]["username"].as_str().unwrap_or("").to_string();
            let password = item["login"]["password"].as_str().unwrap_or("").to_string();
            let url = item["login"]["uris"]
                .as_array()
                .and_then(|uris| uris.first())
                .and_then(|u| u["uri"].as_str())
                .unwrap_or("")
                .to_string();

            let entry = Entry::new(&title, &username, &password, &url);
            kf.root.group.entries.push(entry);
            imported += 1;
        }
    } else {
        skipped += 1;
    }

    db.save()?;

    Ok(ImportResult { imported, skipped, duplicates: 0, errors: vec![] })
}

/// Import Google Password Manager CSV.
#[tauri::command]
pub async fn import_google(
    state: State<'_, Arc<Mutex<DbState>>>,
    path: String,
) -> Result<ImportResult, String> {
    // Google exports: name,url,username,password
    let mapping = CsvColumnMapping {
        title: 0,
        username: 2,
        password: 3,
        url: 1,
        notes: 4,
        totp: 5,
        group: 6,
        delimiter: ",".to_string(),
        has_header: true,
    };

    import_csv_internal(state, &path, &mapping)
}

/// Import Apple Passwords CSV.
#[tauri::command]
pub async fn import_apple(
    state: State<'_, Arc<Mutex<DbState>>>,
    path: String,
) -> Result<ImportResult, String> {
    // Apple exports: Title,URL,Username,Password,Notes,OTPAuth
    let mapping = CsvColumnMapping {
        title: 0,
        username: 2,
        password: 3,
        url: 1,
        notes: 4,
        totp: 5,
        group: 6,
        delimiter: ",".to_string(),
        has_header: true,
    };

    import_csv_internal(state, &path, &mapping)
}

/// Import Proton Pass JSON.
#[tauri::command]
pub async fn import_protonpass(
    state: State<'_, Arc<Mutex<DbState>>>,
    path: String,
) -> Result<ImportResult, String> {
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read file: {e}"))?;

    let data: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Invalid Proton Pass JSON: {e}"))?;

    let mut db = state.lock().map_err(|e| format!("Lock error: {e}"))?;
    let kf = db.keepass_file.as_mut().ok_or("No open database")?;

    let mut imported = 0;

    if let Some(vaults) = data["vaults"].as_object() {
        for (_, vault) in vaults {
            if let Some(items) = vault["items"].as_array() {
                for item in items {
                    let data = &item["data"];
                    let title = data["metadata"]["name"].as_str().unwrap_or("Untitled").to_string();
                    let username = data["content"]["username"].as_str().unwrap_or("").to_string();
                    let password = data["content"]["password"].as_str().unwrap_or("").to_string();
                    let url = data["content"]["urls"]
                        .as_array()
                        .and_then(|u| u.first())
                        .unwrap_or(&serde_json::Value::String(String::new()))
                        .as_str()
                        .unwrap_or("")
                        .to_string();

                    let entry = Entry::new(&title, &username, &password, &url);
                    kf.root.group.entries.push(entry);
                    imported += 1;
                }
            }
        }
    }

    db.save()?;

    Ok(ImportResult { imported, skipped: 0, duplicates: 0, errors: vec![] })
}

// =============================================================================
// Export
// =============================================================================

/// Export entries to CSV.
#[tauri::command]
pub async fn export_csv(
    state: State<'_, Arc<Mutex<DbState>>>,
    path: String,
    uuids: Vec<String>,
) -> Result<(), String> {
    let db = state.lock().map_err(|e| format!("Lock error: {e}"))?;
    let kf = db.keepass_file.as_ref().ok_or("No open database")?;

    let entries: Vec<&Entry> = if uuids.is_empty() {
        collect_all_entries(&kf.root.group)
    } else {
        collect_all_entries(&kf.root.group)
            .into_iter()
            .filter(|e| uuids.contains(&e.uuid))
            .collect()
    };

    let mut wtr = csv::Writer::from_path(&path)
        .map_err(|e| format!("Failed to create CSV: {e}"))?;

    wtr.write_record(&["Title", "Username", "Password", "URL", "Notes", "TOTP"])
        .map_err(|e| format!("CSV write error: {e}"))?;

    for entry in &entries {
        wtr.write_record(&[
            entry.title(),
            entry.username(),
            entry.password(),
            entry.url(),
            entry.notes(),
            "",
        ])
        .map_err(|e| format!("CSV write error: {e}"))?;
    }

    wtr.flush().map_err(|e| format!("CSV flush error: {e}"))?;

    Ok(())
}

/// Export entries to JSON.
#[tauri::command]
pub async fn export_json(
    state: State<'_, Arc<Mutex<DbState>>>,
    path: String,
    uuids: Vec<String>,
) -> Result<(), String> {
    let db = state.lock().map_err(|e| format!("Lock error: {e}"))?;
    let kf = db.keepass_file.as_ref().ok_or("No open database")?;

    let entries: Vec<serde_json::Value> = collect_all_entries(&kf.root.group)
        .into_iter()
        .filter(|e| uuids.is_empty() || uuids.contains(&e.uuid))
        .map(|e| {
            serde_json::json!({
                "title": e.title(),
                "username": e.username(),
                "password": e.password(),
                "url": e.url(),
                "notes": e.notes(),
                "uuid": e.uuid,
            })
        })
        .collect();

    let json = serde_json::to_string_pretty(&entries)
        .map_err(|e| format!("JSON serialization error: {e}"))?;

    std::fs::write(&path, json)
        .map_err(|e| format!("Failed to write file: {e}"))?;

    Ok(())
}

// =============================================================================
// File reading (for client-side parsing)
// =============================================================================

/// Read file content as string for client-side CSV/JSON parsing.
#[tauri::command]
pub async fn read_file_content(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read file '{}': {}", path, e))
}

// =============================================================================
// Dedup-aware import & query
// =============================================================================

/// Dedup entry data returned for client-side comparison.
#[derive(serde::Serialize)]
pub struct DedupEntry {
    pub uuid: String,
    pub title: String,
    pub username: String,
    pub password: String,
    pub url: String,
    pub notes: String,
    pub tags: Vec<String>,
    #[serde(rename = "customFields")]
    pub custom_fields: std::collections::HashMap<String, String>,
    pub created: String,
    pub modified: String,
}

/// Resolved entry sent from frontend after dedup.
#[derive(serde::Deserialize)]
pub struct ResolvedEntry {
    pub title: String,
    pub username: String,
    pub password: String,
    pub url: String,
    pub notes: String,
    // Received but not written to the vault yet (data loss on import) — roadmap sub-project 5.
    #[allow(dead_code)]
    pub tags: Vec<String>,
    #[serde(rename = "customFields")]
    #[allow(dead_code)]
    pub custom_fields: std::collections::HashMap<String, String>,
    #[allow(dead_code)]
    pub totp: String,
}

/// Import resolved entries (after client-side dedup) into the open database.
#[tauri::command]
pub async fn import_entries(
    state: State<'_, Arc<Mutex<DbState>>>,
    entries: Vec<ResolvedEntry>,
) -> Result<ImportResult, String> {
    let mut db = state.lock().map_err(|e| format!("Lock error: {e}"))?;
    let kf = db.keepass_file.as_mut().ok_or("No open database")?;

    let mut imported = 0;
    let mut skipped = 0;

    for entry in &entries {
        if entry.title.is_empty() && entry.username.is_empty() {
            skipped += 1;
            continue;
        }

        let k_entry = {
            let mut e = Entry::new(&entry.title, &entry.username, &entry.password, &entry.url);
            // Set notes if present
            if !entry.notes.is_empty() {
                if let Some(s) = e.strings.iter_mut().find(|s| s.key == "Notes") {
                    s.value.content = entry.notes.clone();
                }
            }
            e
        };

        kf.root.group.entries.push(k_entry);
        imported += 1;
    }

    db.save()?;

    Ok(ImportResult { imported, skipped, duplicates: 0, errors: vec![] })
}

/// Return all entries with full data for client-side dedup comparison.
#[tauri::command]
pub async fn get_entries_for_dedup(
    state: State<'_, Arc<Mutex<DbState>>>,
) -> Result<Vec<DedupEntry>, String> {
    let db = state.lock().map_err(|e| format!("Lock error: {e}"))?;
    let kf = db.keepass_file.as_ref().ok_or("No open database")?;

    let entries = collect_all_entries(&kf.root.group);

    Ok(entries
        .iter()
        .map(|e| DedupEntry {
            uuid: e.uuid.clone(),
            title: e.title().to_string(),
            username: e.username().to_string(),
            password: e.password().to_string(),
            url: e.url().to_string(),
            notes: e.notes().to_string(),
            tags: e.tags.as_deref().unwrap_or("").split(',').filter(|s| !s.is_empty()).map(|s| s.trim().to_string()).collect(),
            custom_fields: std::collections::HashMap::new(),
            created: e.times.creation_time.clone().unwrap_or_default(),
            modified: e.times.last_modification_time.clone().unwrap_or_default(),
        })
        .collect())
}

// =============================================================================
// Helpers
// =============================================================================

fn import_csv_internal(
    state: State<'_, Arc<Mutex<DbState>>>,
    path: &str,
    mapping: &CsvColumnMapping,
) -> Result<ImportResult, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read file: {e}"))?;

    let delimiter = if mapping.delimiter.is_empty() { b',' } else { mapping.delimiter.as_bytes()[0] };
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(mapping.has_header)
        .from_reader(content.as_bytes());

    let mut db = state.lock().map_err(|e| format!("Lock error: {e}"))?;
    let kf = db.keepass_file.as_mut().ok_or("No open database")?;

    let mut imported = 0;
    let mut skipped = 0;

    for result in reader.records() {
        match result {
            Ok(record) => {
                let title = record.get(mapping.title).unwrap_or("Untitled").to_string();
                let username = record.get(mapping.username).unwrap_or("").to_string();
                let password = record.get(mapping.password).unwrap_or("").to_string();
                let url = record.get(mapping.url).unwrap_or("").to_string();

                if title.is_empty() && username.is_empty() {
                    skipped += 1;
                    continue;
                }

                kf.root.group.entries.push(Entry::new(&title, &username, &password, &url));
                imported += 1;
            }
            Err(_) => {
                skipped += 1;
            }
        }
    }

    db.save()?;

    Ok(ImportResult { imported, skipped, duplicates: 0, errors: vec![] })
}

fn collect_all_entries(group: &Group) -> Vec<&Entry> {
    let mut result: Vec<&Entry> = group.entries.iter().collect();
    for child in &group.groups {
        result.extend(collect_all_entries(child));
    }
    result
}

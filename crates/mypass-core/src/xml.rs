//! KDBX XML layer. Handles serialization and deserialization of
//! the XML-based inner format used by KDBX databases.
//!
//! KDBX format: [Outer Header] | [Encrypted XML payload]
//! The XML payload contains groups, entries, and metadata.

use serde::{Deserialize, Serialize};

/// Top-level KDBX XML document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeePassFile {
    #[serde(rename = "Meta")]
    pub meta: Meta,
    #[serde(rename = "Root")]
    pub root: Root,
}

/// Database metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meta {
    #[serde(rename = "Generator")]
    pub generator: String,
    #[serde(rename = "DatabaseName", default)]
    pub database_name: String,
    #[serde(rename = "DatabaseDescription", default)]
    pub database_description: String,
    #[serde(rename = "DatabaseNameChanged", default)]
    pub database_name_changed: String,
    #[serde(rename = "DatabaseDescriptionChanged", default)]
    pub database_description_changed: String,
    #[serde(rename = "SettingsChanged", default)]
    pub settings_changed: String,
    #[serde(rename = "MasterKeyChanged", default)]
    pub master_key_changed: String,
    #[serde(rename = "RecycleBinEnabled", default)]
    pub recycle_bin_enabled: Option<String>,
    #[serde(rename = "RecycleBinUUID", default)]
    pub recycle_bin_uuid: Option<String>,
    #[serde(rename = "HistoryMaxItems", default)]
    pub history_max_items: Option<String>,
    #[serde(rename = "HistoryMaxSize", default)]
    pub history_max_size: Option<String>,
    #[serde(rename = "CustomData", default)]
    pub custom_data: CustomData,
    #[serde(rename = "MemoryProtection", default)]
    pub memory_protection: MemoryProtection,
}

/// Custom data key-value pairs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CustomData {
    #[serde(rename = "Item", default)]
    pub items: Vec<CustomDataItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomDataItem {
    #[serde(rename = "Key")]
    pub key: String,
    #[serde(rename = "Value")]
    pub value: String,
}

/// Memory protection settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryProtection {
    #[serde(rename = "ProtectTitle", default)]
    pub protect_title: Option<String>,
    #[serde(rename = "ProtectUserName", default)]
    pub protect_user_name: Option<String>,
    #[serde(rename = "ProtectPassword", default)]
    pub protect_password: Option<String>,
    #[serde(rename = "ProtectURL", default)]
    pub protect_url: Option<String>,
    #[serde(rename = "ProtectNotes", default)]
    pub protect_notes: Option<String>,
}

/// Root element containing groups and deleted objects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Root {
    #[serde(rename = "Group")]
    pub group: Group,
    #[serde(rename = "DeletedObjects", default)]
    pub deleted_objects: DeletedObjects,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeletedObjects {
    #[serde(rename = "DeletedObject", default)]
    pub items: Vec<DeletedObject>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeletedObject {
    #[serde(rename = "UUID")]
    pub uuid: String,
    #[serde(rename = "DeletionTime")]
    pub deletion_time: String,
}

impl DeletedObject {
    pub fn now(uuid: &str) -> Self {
        Self { uuid: uuid.to_string(), deletion_time: chrono_now() }
    }
}

/// A group containing entries and nested groups.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    #[serde(rename = "UUID")]
    pub uuid: String,
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Notes", default)]
    pub notes: Option<String>,
    #[serde(rename = "IconID", default)]
    pub icon_id: Option<String>,
    #[serde(rename = "Times")]
    pub times: Times,
    #[serde(rename = "IsExpanded", default)]
    pub is_expanded: Option<String>,
    #[serde(rename = "DefaultAutoTypeSequence", default)]
    pub default_auto_type_sequence: Option<String>,
    #[serde(rename = "EnableAutoType", default)]
    pub enable_auto_type: Option<String>,
    #[serde(rename = "EnableSearching", default)]
    pub enable_searching: Option<String>,
    #[serde(rename = "LastTopVisibleEntry", default)]
    pub last_top_visible_entry: Option<String>,
    #[serde(rename = "Group", default)]
    pub groups: Vec<Group>,
    #[serde(rename = "Entry", default)]
    pub entries: Vec<Entry>,
}

/// Timestamps for entries and groups.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Times {
    #[serde(rename = "CreationTime", default)]
    pub creation_time: Option<String>,
    #[serde(rename = "LastModificationTime", default)]
    pub last_modification_time: Option<String>,
    #[serde(rename = "LastAccessTime", default)]
    pub last_access_time: Option<String>,
    #[serde(rename = "ExpiryTime", default)]
    pub expiry_time: Option<String>,
    #[serde(rename = "Expires", default)]
    pub expires: Option<String>,
    #[serde(rename = "UsageCount", default)]
    pub usage_count: Option<String>,
    #[serde(rename = "LocationChanged", default)]
    pub location_changed: Option<String>,
}

/// A password entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    #[serde(rename = "UUID")]
    pub uuid: String,
    #[serde(rename = "IconID", default)]
    pub icon_id: Option<String>,
    #[serde(rename = "ForegroundColor", default)]
    pub foreground_color: Option<String>,
    #[serde(rename = "BackgroundColor", default)]
    pub background_color: Option<String>,
    #[serde(rename = "OverrideURL", default)]
    pub override_url: Option<String>,
    #[serde(rename = "Tags", default)]
    pub tags: Option<String>,
    #[serde(rename = "String", default)]
    pub strings: Vec<EntryString>,
    #[serde(rename = "Binary", default)]
    pub binaries: Vec<Binary>,
    #[serde(rename = "AutoType", default)]
    pub auto_type: Option<AutoType>,
    #[serde(rename = "History", default)]
    pub history: Option<History>,
    #[serde(rename = "Times")]
    pub times: Times,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryString {
    #[serde(rename = "Key")]
    pub key: String,
    #[serde(rename = "Value")]
    pub value: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Value {
    #[serde(alias = "$value", alias = "$text")]
    pub content: String,
    #[serde(rename = "@Protected", default)]
    pub protected: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Binary {
    #[serde(rename = "Key")]
    pub key: String,
    #[serde(rename = "Value")]
    pub value: BinaryValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryValue {
    #[serde(rename = "@Ref")]
    pub r#ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoType {
    #[serde(rename = "Enabled", default)]
    pub enabled: Option<String>,
    #[serde(rename = "DataTransferObfuscation", default)]
    pub data_transfer_obfuscation: Option<String>,
    #[serde(rename = "DefaultSequence", default)]
    pub default_sequence: Option<String>,
    #[serde(rename = "Association", default)]
    pub associations: Vec<AutoTypeAssociation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoTypeAssociation {
    #[serde(rename = "Window")]
    pub window: String,
    #[serde(rename = "KeystrokeSequence")]
    pub keystroke_sequence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct History {
    #[serde(rename = "Entry", default)]
    pub entries: Vec<Entry>,
}

// =============================================================================
// Convenience constructors and helpers
// =============================================================================

impl KeePassFile {
    pub fn new(name: &str) -> Self {
        let now = chrono_now();
        Self {
            meta: Meta {
                generator: "MyPass".to_string(),
                database_name: name.to_string(),
                database_description: String::new(),
                database_name_changed: now.clone(),
                database_description_changed: now.clone(),
                settings_changed: now.clone(),
                master_key_changed: now.clone(),
                recycle_bin_enabled: Some("True".to_string()),
                recycle_bin_uuid: None,
                history_max_items: Some("-1".to_string()),
                history_max_size: Some("6291456".to_string()),
                custom_data: CustomData::default(),
                memory_protection: MemoryProtection::default(),
            },
            root: Root {
                group: Group::new("Root"),
                deleted_objects: DeletedObjects::default(),
            },
        }
    }
}

impl Group {
    pub fn new(name: &str) -> Self {
        Self {
            uuid: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            notes: None,
            icon_id: None,
            times: Times::now(),
            is_expanded: Some("True".to_string()),
            default_auto_type_sequence: None,
            enable_auto_type: None,
            enable_searching: None,
            last_top_visible_entry: None,
            groups: vec![],
            entries: vec![],
        }
    }
}

impl Times {
    pub fn now() -> Self {
        let now = chrono_now();
        Self {
            creation_time: Some(now.clone()),
            last_modification_time: Some(now.clone()),
            last_access_time: Some(now),
            expiry_time: None,
            expires: Some("False".to_string()),
            usage_count: Some("0".to_string()),
            location_changed: Some(chrono_now()),
        }
    }

    /// À appeler après toute mutation de contenu : la fusion LWW ne voit
    /// une modification que si LastModificationTime avance.
    pub fn touch(&mut self) {
        let now = chrono_now();
        self.last_modification_time = Some(now.clone());
        self.last_access_time = Some(now);
    }
}

impl Entry {
    pub fn new(title: &str, username: &str, password: &str, url: &str) -> Self {
        let uuid = uuid::Uuid::new_v4().to_string();
        Self {
            uuid: uuid.clone(),
            icon_id: None,
            foreground_color: None,
            background_color: None,
            override_url: None,
            tags: None,
            strings: vec![
                EntryString {
                    key: "Title".to_string(),
                    value: Value { content: title.to_string(), protected: None },
                },
                EntryString {
                    key: "UserName".to_string(),
                    value: Value { content: username.to_string(), protected: None },
                },
                EntryString {
                    key: "Password".to_string(),
                    value: Value { content: password.to_string(), protected: Some("True".to_string()) },
                },
                EntryString {
                    key: "URL".to_string(),
                    value: Value { content: url.to_string(), protected: None },
                },
                EntryString {
                    key: "Notes".to_string(),
                    value: Value { content: String::new(), protected: None },
                },
            ],
            binaries: vec![],
            auto_type: Some(AutoType {
                enabled: Some("True".to_string()),
                data_transfer_obfuscation: Some("0".to_string()),
                default_sequence: None,
                associations: vec![],
            }),
            history: None,
            times: Times::now(),
        }
    }

    pub fn get_field(&self, key: &str) -> Option<&str> {
        self.strings
            .iter()
            .find(|s| s.key == key)
            .map(|s| s.value.content.as_str())
    }

    pub fn title(&self) -> &str {
        self.get_field("Title").unwrap_or("")
    }

    pub fn username(&self) -> &str {
        self.get_field("UserName").unwrap_or("")
    }

    pub fn password(&self) -> &str {
        self.get_field("Password").unwrap_or("")
    }

    pub fn url(&self) -> &str {
        self.get_field("URL").unwrap_or("")
    }

    pub fn notes(&self) -> &str {
        self.get_field("Notes").unwrap_or("")
    }
}

/// Get current timestamp in KDBX format (ISO 8601 UTC).
fn chrono_now() -> String {
    format_timestamp(crate::time::unix_now())
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
        assert!(now.as_str() > "2026-01-01", "date figée: {now}");
        assert_eq!(now.len(), "2026-07-10T12:00:00Z".len());
        assert!(now.ends_with('Z'));
    }

    #[test]
    fn touch_advances_last_modification_time() {
        let mut entry = Entry::new("Titre", "user", "pass", "https://x.io");
        entry.times.last_modification_time = Some("2020-01-01T00:00:00Z".to_string());

        entry.times.touch();

        let lmt = entry.times.last_modification_time.as_deref().unwrap();
        assert!(lmt > "2020-01-01T00:00:00Z", "lmt n'a pas avancé: {lmt}");
        assert_eq!(lmt.len(), 20);
    }

    #[test]
    fn deleted_object_now_has_uuid_and_recent_time() {
        let obj = DeletedObject::now("some-uuid");
        assert_eq!(obj.uuid, "some-uuid");
        assert!(obj.deletion_time.as_str() > "2026-01-01");
        assert_eq!(obj.deletion_time.len(), 20);
    }
}

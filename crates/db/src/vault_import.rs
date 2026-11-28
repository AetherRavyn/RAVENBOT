//! Vault import from common password-manager exports.
//!
//! Import runs on the backend so exported passwords and notes never cross the
//! IPC boundary. Parsing produces transient `VaultImportItem`s; only metadata
//! is returned to the renderer, and the commit path stores each secret through
//! the same `VaultQueries::upsert` keychain path as a manually added item.

use crate::queries::{VaultItemMeta, VaultQueries};
use serde::Serialize;
use serde_json::{Map, Value};
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// The largest export file accepted. Password exports are small; a larger file
/// is more likely to be a mistake or an attempt to exhaust memory.
pub const MAX_IMPORT_BYTES: usize = 8 * 1024 * 1024;
/// The largest number of entries accepted from one export.
pub const MAX_IMPORT_ITEMS: usize = 2_000;
const MAX_SECRET_BYTES: usize = 16 * 1024;
const PREVIEW_LIMIT: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImportFormat {
    BitwardenJson,
    BitwardenCsv,
    OnePasswordCsv,
    Csv,
}

impl ImportFormat {
    fn label(self) -> &'static str {
        match self {
            Self::BitwardenJson => "Bitwarden JSON",
            Self::BitwardenCsv => "Bitwarden CSV",
            Self::OnePasswordCsv => "1Password CSV",
            Self::Csv => "CSV",
        }
    }
}

/// A parsed item. The secret stays in backend memory and is never serialized
/// to the renderer, which is why `Debug` is implemented manually.
struct VaultImportItem {
    kind: String,
    name: String,
    username: String,
    website: String,
    last4: Option<String>,
    secret: Value,
}

impl std::fmt::Debug for VaultImportItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VaultImportItem")
            .field("kind", &self.kind)
            .field("name", &self.name)
            .field("username", &self.username)
            .field("website", &self.website)
            .field("last4", &self.last4)
            .field("secret", &"<redacted>")
            .finish()
    }
}

#[derive(Debug)]
struct ParsedVaultImport {
    format: ImportFormat,
    items: Vec<VaultImportItem>,
    skipped: usize,
}

impl ParsedVaultImport {
    fn new(format: ImportFormat) -> Self {
        Self {
            format,
            items: Vec::new(),
            skipped: 0,
        }
    }
}

/// Metadata shown before the user confirms an import.
#[derive(Debug, Clone, Serialize)]
pub struct VaultImportPreviewItem {
    pub kind: String,
    pub name: String,
    pub username: String,
    pub website: String,
    pub duplicate: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct VaultImportPreview {
    pub format: String,
    pub total: usize,
    pub importable: usize,
    pub duplicates: usize,
    pub skipped: usize,
    pub items: Vec<VaultImportPreviewItem>,
    pub more: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct VaultImportResult {
    pub format: String,
    pub imported: usize,
    pub duplicates: usize,
    pub skipped: usize,
    pub failed: usize,
}

/// Parse an export without touching the database.
fn parse_vault_import(bytes: &[u8], filename: &str) -> Result<ParsedVaultImport, String> {
    if bytes.is_empty() {
        return Err("The selected export file is empty.".to_string());
    }
    if bytes.len() > MAX_IMPORT_BYTES {
        return Err(format!(
            "The export is larger than the {} MB import limit.",
            MAX_IMPORT_BYTES / (1024 * 1024)
        ));
    }
    if filename.to_lowercase().ends_with(".1pux") {
        return Err(
            "1Password 1PUX archives are not supported. Export a CSV file from 1Password instead."
                .to_string(),
        );
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| "The vault export must be UTF-8 text.".to_string())?;
    let text = text.trim_start_matches('\u{feff}').trim_start();
    if text.is_empty() {
        return Err("The selected export file is empty.".to_string());
    }
    if text.starts_with('{') || text.starts_with('[') {
        parse_bitwarden_json(text)
    } else {
        parse_csv_export(text)
    }
}

fn parse_bitwarden_json(text: &str) -> Result<ParsedVaultImport, String> {
    let root: Value = serde_json::from_str(text)
        .map_err(|_| "This is not a valid Bitwarden JSON export.".to_string())?;
    if root
        .get("encrypted")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err(
            "This Bitwarden export is encrypted. Export an unencrypted JSON file and try again."
                .to_string(),
        );
    }
    let entries = root
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(|| "This JSON file does not contain a Bitwarden `items` array.".to_string())?;
    if entries.len() > MAX_IMPORT_ITEMS {
        return Err(format!(
            "This export contains more than {MAX_IMPORT_ITEMS} items. Split it into smaller exports."
        ));
    }

    let mut parsed = ParsedVaultImport::new(ImportFormat::BitwardenJson);
    for entry in entries {
        if entry
            .get("deletedDate")
            .map(|value| !value.is_null())
            .unwrap_or(false)
        {
            parsed.skipped += 1;
            continue;
        }
        let item_type = entry.get("type").and_then(Value::as_u64).unwrap_or(0);
        let name = json_str(entry, &["name"]);
        let notes = json_str(entry, &["notes"]);
        let item = match item_type {
            1 => bitwarden_login(entry, name, notes),
            2 => {
                if notes.is_empty() {
                    None
                } else {
                    finish_item(
                        "contact",
                        name,
                        "",
                        "",
                        None,
                        serde_json::json!({ "note": notes }),
                    )
                }
            }
            3 => bitwarden_card(entry, name, notes),
            _ => None,
        };
        match item {
            Some(item) => parsed.items.push(item),
            None => parsed.skipped += 1,
        }
    }
    if parsed.items.is_empty() && parsed.skipped == 0 {
        return Err("The Bitwarden export contains no items.".to_string());
    }
    Ok(parsed)
}

fn bitwarden_login(entry: &Value, name: &str, notes: &str) -> Option<VaultImportItem> {
    let login = entry.get("login").unwrap_or(&Value::Null);
    let username = json_str(login, &["username"]);
    let password = json_str(login, &["password"]);
    if password.is_empty() {
        return None;
    }
    let totp = json_str(login, &["totp"]);
    let website = login
        .get("uris")
        .and_then(Value::as_array)
        .and_then(|uris| {
            uris.iter()
                .find_map(|uri| uri.get("uri").and_then(Value::as_str))
        })
        .unwrap_or("");
    let mut secret = Map::new();
    secret.insert("password".to_string(), Value::String(password.to_string()));
    if !totp.is_empty() {
        secret.insert("totp".to_string(), Value::String(totp.to_string()));
    }
    if !notes.is_empty() {
        secret.insert("notes".to_string(), Value::String(notes.to_string()));
    }
    finish_item(
        "login",
        name,
        username,
        website,
        None,
        Value::Object(secret),
    )
}

fn bitwarden_card(entry: &Value, name: &str, notes: &str) -> Option<VaultImportItem> {
    let card = entry.get("card").unwrap_or(&Value::Null);
    let number = json_str(card, &["number"]);
    if number.is_empty() {
        return if notes.is_empty() {
            None
        } else {
            finish_item(
                "contact",
                name,
                "",
                "",
                None,
                serde_json::json!({ "note": notes }),
            )
        };
    }
    let holder = json_str(card, &["cardholderName"]);
    let month = json_str(card, &["expMonth"]);
    let year = json_str(card, &["expYear"]);
    let code = json_str(card, &["code"]);
    let mut secret = Map::new();
    secret.insert("number".to_string(), Value::String(number.to_string()));
    if !holder.is_empty() {
        secret.insert("cardholder".to_string(), Value::String(holder.to_string()));
    }
    if !month.is_empty() || !year.is_empty() {
        secret.insert(
            "expiry".to_string(),
            Value::String(format!("{month}/{year}")),
        );
    }
    if !code.is_empty() {
        secret.insert("code".to_string(), Value::String(code.to_string()));
    }
    if !notes.is_empty() {
        secret.insert("notes".to_string(), Value::String(notes.to_string()));
    }
    let display_name = if name.trim().is_empty() { holder } else { name };
    finish_item(
        "card",
        display_name,
        holder,
        "",
        last4(number).as_deref(),
        Value::Object(secret),
    )
}

fn parse_csv_export(text: &str) -> Result<ParsedVaultImport, String> {
    let records = parse_csv_records(text)?;
    let Some(headers_row) = records.first() else {
        return Err("The CSV file has no header row.".to_string());
    };
    let headers = header_map(headers_row);
    if !headers.contains_key("name") && !headers.contains_key("title") {
        return Err("The CSV file has no name or title column.".to_string());
    }
    let format = detect_csv_format(&headers);
    let mut parsed = ParsedVaultImport::new(format);
    for row in records.iter().skip(1) {
        if row.iter().all(|value| value.trim().is_empty()) {
            continue;
        }
        match csv_row_item(row, &headers) {
            Some(item) => parsed.items.push(item),
            None => parsed.skipped += 1,
        }
    }
    if parsed.items.len() > MAX_IMPORT_ITEMS {
        return Err(format!(
            "This export contains more than {MAX_IMPORT_ITEMS} items. Split it into smaller exports."
        ));
    }
    if parsed.items.is_empty() && parsed.skipped == 0 {
        return Err("The CSV file has no data rows.".to_string());
    }
    Ok(parsed)
}

fn detect_csv_format(headers: &HashMap<String, usize>) -> ImportFormat {
    if headers.contains_key("login_uri") || headers.contains_key("login_password") {
        return ImportFormat::BitwardenCsv;
    }
    if headers.contains_key("title")
        && (headers.contains_key("website")
            || headers.contains_key("otpauth")
            || headers.contains_key("favorite")
            || headers.contains_key("archived"))
    {
        return ImportFormat::OnePasswordCsv;
    }
    ImportFormat::Csv
}

fn csv_row_item(row: &[String], headers: &HashMap<String, usize>) -> Option<VaultImportItem> {
    let name = cell(row, index(headers, &["name", "title"]));
    let username = cell(
        row,
        index(headers, &["login_username", "username", "user", "login"]),
    );
    let password = cell(row, index(headers, &["login_password", "password"]));
    let website = cell(row, index(headers, &["login_uri", "url", "website", "uri"]));
    let website = website.lines().next().unwrap_or("").trim();
    let notes = cell(row, index(headers, &["notes", "note"]));
    let otp = cell(
        row,
        index(
            headers,
            &["login_totp", "otp", "otpauth", "one-time password"],
        ),
    );
    let item_type = cell(row, index(headers, &["type"])).to_lowercase();
    let number = cell(
        row,
        index(headers, &["card_number", "number", "card number"]),
    );
    let holder = cell(
        row,
        index(
            headers,
            &["cardholder", "cardholder_name", "cardholder name", "holder"],
        ),
    );
    let expiry = cell(
        row,
        index(
            headers,
            &["card_expiry", "expiry", "expiration", "expiration date"],
        ),
    );
    let code = cell(
        row,
        index(headers, &["card_code", "cvv", "cvc", "security code"]),
    );
    let month = cell(row, index(headers, &["card_exp_month", "exp_month"]));
    let year = cell(row, index(headers, &["card_exp_year", "exp_year"]));
    let display_name = if name.trim().is_empty() { holder } else { name };

    let is_note = item_type.contains("note");
    let is_card = item_type == "card" || !number.trim().is_empty();
    if is_note {
        return if notes.trim().is_empty() {
            None
        } else {
            finish_item(
                "contact",
                name,
                "",
                "",
                None,
                serde_json::json!({ "note": notes }),
            )
        };
    }
    if is_card {
        if number.trim().is_empty() {
            return None;
        }
        let mut secret = Map::new();
        secret.insert("number".to_string(), Value::String(number.to_string()));
        if !holder.trim().is_empty() {
            secret.insert("cardholder".to_string(), Value::String(holder.to_string()));
        }
        let expiry = if expiry.trim().is_empty() {
            format!("{month}/{year}")
        } else {
            expiry.to_string()
        };
        if !expiry.trim().is_empty() && expiry != "/" {
            secret.insert("expiry".to_string(), Value::String(expiry));
        }
        if !code.trim().is_empty() {
            secret.insert("code".to_string(), Value::String(code.to_string()));
        }
        if !notes.trim().is_empty() {
            secret.insert("notes".to_string(), Value::String(notes.to_string()));
        }
        return finish_item(
            "card",
            display_name,
            holder,
            "",
            last4(number).as_deref(),
            Value::Object(secret),
        );
    }
    if !password.trim().is_empty() {
        let mut secret = Map::new();
        secret.insert("password".to_string(), Value::String(password.to_string()));
        if !otp.trim().is_empty() {
            secret.insert("totp".to_string(), Value::String(otp.to_string()));
        }
        if !notes.trim().is_empty() {
            secret.insert("notes".to_string(), Value::String(notes.to_string()));
        }
        return finish_item(
            "login",
            name,
            username,
            website,
            None,
            Value::Object(secret),
        );
    }
    if !notes.trim().is_empty() {
        return finish_item(
            "contact",
            name,
            "",
            "",
            None,
            serde_json::json!({ "note": notes }),
        );
    }
    None
}

fn finish_item(
    kind: &str,
    name: &str,
    username: &str,
    website: &str,
    last4: Option<&str>,
    secret: Value,
) -> Option<VaultImportItem> {
    if !matches!(kind, "login" | "card" | "contact") {
        return None;
    }
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 120 {
        return None;
    }
    let username = username.trim();
    if username.chars().count() > 300 {
        return None;
    }
    let website = website.trim();
    if website.chars().count() > 500 {
        return None;
    }
    if !secret.is_object() {
        return None;
    }
    if serde_json::to_vec(&secret).ok()?.len() > MAX_SECRET_BYTES {
        return None;
    }
    Some(VaultImportItem {
        kind: kind.to_string(),
        name: name.to_string(),
        username: username.to_string(),
        website: website.to_string(),
        last4: last4
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        secret,
    })
}

fn last4(number: &str) -> Option<String> {
    let digits: String = number
        .chars()
        .filter(|character| character.is_ascii_digit())
        .collect();
    if digits.len() >= 4 {
        Some(digits[digits.len() - 4..].to_string())
    } else {
        None
    }
}

fn json_str<'a>(value: &'a Value, keys: &[&str]) -> &'a str {
    for key in keys {
        if let Some(found) = value.get(*key).and_then(Value::as_str) {
            return found;
        }
    }
    ""
}

fn index(headers: &HashMap<String, usize>, aliases: &[&str]) -> Option<usize> {
    aliases
        .iter()
        .find_map(|alias| headers.get(*alias).copied())
}

fn cell(row: &[String], index: Option<usize>) -> &str {
    index
        .and_then(|index| row.get(index))
        .map(String::as_str)
        .unwrap_or("")
}

fn header_map(row: &[String]) -> HashMap<String, usize> {
    row.iter()
        .enumerate()
        .map(|(index, header)| (normalize_header(header), index))
        .collect()
}

fn normalize_header(value: &str) -> String {
    value.trim().trim_start_matches('\u{feff}').to_lowercase()
}

/// A small RFC 4180-style parser. The `csv` crate is not worth another
/// dependency for three fixed export shapes; this handles quoted commas,
/// doubled quotes, escaped newlines and CRLF input.
fn parse_csv_records(text: &str) -> Result<Vec<Vec<String>>, String> {
    let mut records = Vec::new();
    let mut record: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '"' if in_quotes => {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    in_quotes = false;
                }
            }
            '"' if field.is_empty() => in_quotes = true,
            ',' if !in_quotes => record.push(std::mem::take(&mut field)),
            '\r' if !in_quotes => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                record.push(std::mem::take(&mut field));
                if !(record.len() == 1 && record[0].is_empty()) {
                    records.push(std::mem::take(&mut record));
                } else {
                    record.clear();
                }
            }
            '\n' if !in_quotes => {
                record.push(std::mem::take(&mut field));
                if !(record.len() == 1 && record[0].is_empty()) {
                    records.push(std::mem::take(&mut record));
                } else {
                    record.clear();
                }
            }
            _ => field.push(character),
        }
    }
    if in_quotes {
        return Err("The CSV file has an unterminated quoted field.".to_string());
    }
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }
    Ok(records)
}

/// Metadata-only preview, including how many rows duplicate existing items.
pub async fn preview_vault_import(
    pool: &SqlitePool,
    bytes: &[u8],
    filename: &str,
) -> Result<VaultImportPreview, String> {
    let parsed = parse_vault_import(bytes, filename)?;
    let existing = VaultQueries::list(pool)
        .await
        .map_err(|error| error.to_string())?;
    let known: HashSet<String> = existing.iter().map(existing_key).collect();
    let mut seen: HashSet<String> = HashSet::new();
    let total_items = parsed.items.len();
    let mut duplicates = 0usize;
    let mut items = Vec::with_capacity(total_items.min(PREVIEW_LIMIT));
    for item in &parsed.items {
        let key = item_key(item);
        let duplicate = known.contains(&key) || !seen.insert(key);
        if duplicate {
            duplicates += 1;
        }
        if items.len() < PREVIEW_LIMIT {
            items.push(VaultImportPreviewItem {
                kind: item.kind.clone(),
                name: item.name.clone(),
                username: item.username.clone(),
                website: item.website.clone(),
                duplicate,
            });
        }
    }
    Ok(VaultImportPreview {
        format: parsed.format.label().to_string(),
        total: total_items + parsed.skipped,
        importable: total_items.saturating_sub(duplicates),
        duplicates,
        skipped: parsed.skipped,
        more: total_items > items.len(),
        items,
    })
}

/// Import every non-duplicate item. Duplicates are skipped, never overwritten.
pub async fn import_vault_items(
    pool: &SqlitePool,
    bytes: &[u8],
    filename: &str,
) -> Result<VaultImportResult, String> {
    let parsed = parse_vault_import(bytes, filename)?;
    if parsed.items.len() > MAX_IMPORT_ITEMS {
        return Err(format!(
            "This export contains more than {MAX_IMPORT_ITEMS} items. Split it into smaller exports."
        ));
    }
    let existing = VaultQueries::list(pool)
        .await
        .map_err(|error| error.to_string())?;
    let known: HashSet<String> = existing.iter().map(existing_key).collect();
    let mut seen: HashSet<String> = HashSet::new();
    let ParsedVaultImport {
        format,
        items,
        skipped,
    } = parsed;
    let mut imported = 0usize;
    let mut duplicates = 0usize;
    let mut failed = 0usize;
    for item in items {
        let key = item_key(&item);
        if known.contains(&key) || !seen.insert(key) {
            duplicates += 1;
            continue;
        }
        let id = Uuid::new_v4().to_string();
        let secret = match serde_json::to_string(&item.secret) {
            Ok(secret) => secret,
            Err(_) => {
                failed += 1;
                continue;
            }
        };
        match VaultQueries::upsert(
            pool,
            &id,
            &item.kind,
            &item.name,
            &item.username,
            &item.website,
            item.last4.as_deref(),
            &secret,
        )
        .await
        {
            Ok(()) => imported += 1,
            Err(_) => failed += 1,
        }
    }
    Ok(VaultImportResult {
        format: format.label().to_string(),
        imported,
        duplicates,
        skipped,
        failed,
    })
}

fn existing_key(item: &VaultItemMeta) -> String {
    item_key_parts(&item.kind, &item.name, &item.username, &item.website)
}

fn item_key(item: &VaultImportItem) -> String {
    item_key_parts(&item.kind, &item.name, &item.username, &item.website)
}

fn item_key_parts(kind: &str, name: &str, username: &str, website: &str) -> String {
    format!(
        "{}\u{1f}{}\u{1f}{}\u{1f}{}",
        kind.trim().to_lowercase(),
        name.trim().to_lowercase(),
        username.trim().to_lowercase(),
        normalize_website(website)
    )
}

fn normalize_website(website: &str) -> String {
    let mut value = website.trim().to_lowercase();
    for prefix in ["https://", "http://"] {
        if let Some(rest) = value.strip_prefix(prefix) {
            value = rest.to_string();
            break;
        }
    }
    if let Some(rest) = value.strip_prefix("www.") {
        value = rest.to_string();
    }
    value.trim_end_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn bitwarden_json() -> &'static str {
        r#"{
          "encrypted": false,
          "items": [
            {
              "type": 1,
              "name": "GitHub",
              "notes": "work account",
              "login": {
                "username": "ada",
                "password": "hunter2",
                "totp": "otpauth://totp/example",
                "uris": [{"uri": "https://github.com"}]
              }
            },
            {"type": 2, "name": "Recovery", "notes": "recovery codes"},
            {
              "type": 3,
              "name": "Travel card",
              "card": {
                "cardholderName": "Ada Lovelace",
                "number": "4111 1111 1111 1111",
                "expMonth": "12",
                "expYear": "2030",
                "code": "123"
              }
            },
            {"type": 4, "name": "Identity", "identity": {}},
            {"type": 1, "name": "Trashed", "deletedDate": "2026-01-01", "login": {"password": "x"}}
          ]
        }"#
    }

    fn one_password_csv() -> &'static str {
        "Title,Url,Username,Password,OTPAuth,Favorite,Archived,Tags,Notes\n\
         GitHub,https://github.com,ada,hunter2,otpauth://totp/x,false,false,work,\"line one, line two\"\n\
         Router,https://192.168.1.1,admin,admin123,,false,false,,\n"
    }

    fn bitwarden_csv() -> &'static str {
        "folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp\n\
         work,0,login,GitHub,note,,0,https://github.com,ada,hunter2,otpauth://totp/x\n\
         ,0,note,Recovery,recovery codes,,0,,,,\n"
    }

    #[test]
    fn bitwarden_json_imports_logins_notes_and_cards() {
        let parsed = parse_vault_import(bitwarden_json().as_bytes(), "vault.json").unwrap();
        assert_eq!(parsed.format, ImportFormat::BitwardenJson);
        assert_eq!(parsed.items.len(), 3);
        assert_eq!(parsed.skipped, 2);
        let login = &parsed.items[0];
        assert_eq!(login.kind, "login");
        assert_eq!(login.name, "GitHub");
        assert_eq!(login.username, "ada");
        assert_eq!(login.website, "https://github.com");
        assert_eq!(login.secret["password"], "hunter2");
        assert_eq!(login.secret["totp"], "otpauth://totp/example");
        let note = &parsed.items[1];
        assert_eq!(note.kind, "contact");
        assert_eq!(note.secret["note"], "recovery codes");
        let card = &parsed.items[2];
        assert_eq!(card.kind, "card");
        assert_eq!(card.last4.as_deref(), Some("1111"));
        assert_eq!(card.secret["cardholder"], "Ada Lovelace");
        assert_eq!(card.secret["expiry"], "12/2030");
    }

    #[test]
    fn encrypted_bitwarden_exports_are_refused() {
        let error =
            parse_vault_import(br#"{"encrypted": true, "items": []}"#, "vault.json").unwrap_err();
        assert!(error.contains("encrypted"));
    }

    #[test]
    fn one_password_csv_handles_quoted_fields_and_otp() {
        let parsed = parse_vault_import(one_password_csv().as_bytes(), "vault.csv").unwrap();
        assert_eq!(parsed.format, ImportFormat::OnePasswordCsv);
        assert_eq!(parsed.items.len(), 2);
        let login = &parsed.items[0];
        assert_eq!(login.name, "GitHub");
        assert_eq!(login.secret["password"], "hunter2");
        assert_eq!(login.secret["totp"], "otpauth://totp/x");
        assert_eq!(login.secret["notes"], "line one, line two");
    }

    #[test]
    fn bitwarden_csv_imports_logins_and_notes() {
        let parsed = parse_vault_import(bitwarden_csv().as_bytes(), "vault.csv").unwrap();
        assert_eq!(parsed.format, ImportFormat::BitwardenCsv);
        assert_eq!(parsed.items.len(), 2);
        assert_eq!(parsed.items[0].kind, "login");
        assert_eq!(parsed.items[0].secret["password"], "hunter2");
        assert_eq!(parsed.items[1].kind, "contact");
        assert_eq!(parsed.items[1].secret["note"], "recovery codes");
    }

    #[test]
    fn generic_csv_imports_cards_and_rejects_malformed_quotes() {
        let csv = "Name,Username,Password,Url,Card Number,Cardholder,Expiry\n\
                   Bank,ada,,https://bank.example,4111111111111111,Ada,12/30\n";
        let parsed = parse_vault_import(csv.as_bytes(), "vault.csv").unwrap();
        assert_eq!(parsed.items.len(), 1);
        assert_eq!(parsed.items[0].kind, "card");
        assert_eq!(parsed.items[0].last4.as_deref(), Some("1111"));
        assert!(
            parse_vault_import(b"Name,Password\n\"unterminated,x", "bad.csv")
                .unwrap_err()
                .contains("unterminated")
        );
    }

    #[test]
    fn csv_parser_handles_crlf_doubled_quotes_and_newlines() {
        let records = parse_csv_records(
            "A,B\r\n\"x, y\",\"he said \"\"hi\"\"\"\r\n\"line one\nline two\",z\r\n",
        )
        .unwrap();
        assert_eq!(
            records,
            vec![
                vec!["A".to_string(), "B".to_string()],
                vec!["x, y".to_string(), "he said \"hi\"".to_string()],
                vec!["line one\nline two".to_string(), "z".to_string()],
            ]
        );
    }

    #[test]
    fn one_password_1pux_is_refused_with_a_clear_instruction() {
        let error = parse_vault_import(b"PK\x03\x04", "vault.1pux").unwrap_err();
        assert!(error.contains("CSV"));
        assert!(error.contains("1PUX"));
    }

    #[test]
    fn duplicate_keys_ignore_scheme_case_and_trailing_slash() {
        assert_eq!(
            item_key_parts("Login", " GitHub ", "ADA", "https://www.GitHub.com/"),
            item_key_parts("login", "github", "ada", "github.com")
        );
    }

    #[tokio::test]
    async fn preview_and_commit_skip_existing_duplicates() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::secrets::force_fallback_for_tests();
        crate::secrets::use_test_vault_root(
            std::env::temp_dir().join(format!("ravenbot-import-secrets-{}", Uuid::new_v4())),
        );
        std::env::set_var("RAVENBOT_DISABLE_KEYCHAIN", "1");
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE vault_items (
                id TEXT PRIMARY KEY NOT NULL,
                kind TEXT NOT NULL,
                name TEXT NOT NULL,
                key_name TEXT,
                username TEXT NOT NULL DEFAULT '',
                website TEXT NOT NULL DEFAULT '',
                last4 TEXT,
                secret TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO vault_items (id, kind, name, username, website, secret, created_at, updated_at)
             VALUES ('existing', 'login', 'GitHub', 'ada', 'https://github.com/', '{\"password\":\"old\"}', 'now', 'now')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let preview = preview_vault_import(&pool, bitwarden_json().as_bytes(), "vault.json")
            .await
            .unwrap();
        assert_eq!(preview.format, "Bitwarden JSON");
        assert_eq!(
            preview.duplicates, 1,
            "the existing GitHub login must match"
        );
        assert_eq!(preview.importable, 2);
        assert_eq!(preview.skipped, 2);
        assert_eq!(preview.items.len(), 3);

        let result = import_vault_items(&pool, bitwarden_json().as_bytes(), "vault.json")
            .await
            .unwrap();
        assert_eq!(result.imported, 2);
        assert_eq!(result.duplicates, 1);
        assert_eq!(result.skipped, 2);
        assert_eq!(result.failed, 0);
        let items = VaultQueries::list(&pool).await.unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(
            items.iter().filter(|item| item.kind == "login").count(),
            1,
            "the duplicate must not be overwritten or double-imported"
        );
        std::env::remove_var("RAVENBOT_DISABLE_KEYCHAIN");
    }
}

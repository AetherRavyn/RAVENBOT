//! Owner-only files for API keys requested through the Vault.
//!
//! The Vault keeps the value encrypted. This module writes the copy an agent
//! can use without putting the value in its context: a `0600` file in a `0700`
//! directory under `~/RAVENBOT/keys/api-keys`. It never returns the value, and
//! the path is derived from a small, validated name so a request cannot escape
//! the directory or overwrite an unrelated file.
//!
//! The file is readable by the owner's user account, not by a single isolated
//! process. An agent that can run a shell in that account can read the file by
//! design; the point is that the value never has to enter the model's context.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// The longest API-key name accepted.
pub const MAX_NAME: usize = 64;

/// The largest value accepted, in bytes. API keys are short; this bound keeps
/// an accidental paste of a certificate or a file from becoming a key.
pub const MAX_VALUE_BYTES: usize = 8 * 1024;

/// Validate a machine-safe key name.
///
/// The name becomes a file name, so it is deliberately narrower than a label:
/// lowercase ASCII letters, digits, `.`, `_` and `-`, starting with a letter or
/// digit. Path separators, leading dots, trailing dots and Windows device names
/// are refused rather than normalised, so two requests can never collide by
/// accident.
pub fn validate_api_key_name(raw: &str) -> Result<&str, String> {
    let name = raw.trim();
    if name.is_empty() {
        return Err("An API key needs a name.".to_string());
    }
    if name.len() > MAX_NAME {
        return Err(format!(
            "The API key name is longer than {MAX_NAME} characters."
        ));
    }
    let mut chars = name.chars();
    let first = chars.next().unwrap_or_default();
    if !(first.is_ascii_lowercase() || first.is_ascii_digit()) {
        return Err("The API key name must start with a lowercase letter or a digit.".to_string());
    }
    if !name
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '.' | '_' | '-'))
    {
        return Err(
            "The API key name may only contain lowercase letters, digits, '.', '_' and '-'."
                .to_string(),
        );
    }
    if name.ends_with('.') {
        return Err("The API key name may not end with a dot.".to_string());
    }
    // Windows reserves the device name before the first dot, so `con.txt` is
    // just as unusable as `con`. The app is Linux-first, but a key created on
    // one platform should not produce a broken path on another.
    let base = name.split('.').next().unwrap_or(name);
    let reserved = matches!(base, "con" | "prn" | "aux" | "nul")
        || (base.len() == 4
            && (base.starts_with("com") || base.starts_with("lpt"))
            && base[3..].chars().all(|ch| ch.is_ascii_digit()));
    if reserved {
        return Err(format!("'{name}' is a reserved file name."));
    }
    Ok(name)
}

/// Validate and normalise a secret value.
///
/// Surrounding whitespace from a paste is removed; whitespace inside the value
/// is refused because a key is a single token. Control characters are refused
/// so a value cannot smuggle a newline into a later command or file.
pub fn validate_api_key_value(raw: &str) -> Result<&str, String> {
    let value = raw.trim();
    if value.is_empty() {
        return Err("The API key is empty.".to_string());
    }
    if value.len() > MAX_VALUE_BYTES {
        return Err(format!(
            "The API key is larger than {MAX_VALUE_BYTES} bytes."
        ));
    }
    if value
        .chars()
        .any(|ch| ch.is_whitespace() || ch.is_control())
    {
        return Err(
            "The API key contains whitespace or control characters. Paste it as one line."
                .to_string(),
        );
    }
    Ok(value)
}

/// The directory that holds agent-readable API-key files.
pub fn api_keys_dir() -> PathBuf {
    crate::paths::keys_dir().join("api-keys")
}

/// The file for a validated key name.
pub fn api_key_path(name: &str) -> Result<PathBuf, String> {
    let name = validate_api_key_name(name)?;
    Ok(api_keys_dir().join(name))
}

/// Write one key file through the RAVENBOT root.
pub fn write_api_key(name: &str, value: &str) -> Result<PathBuf, String> {
    write_api_key_in(&api_keys_dir(), name, value)
}

/// Write one key file under an explicit directory.
///
/// The directory is created with `0700`, an existing symlink where the
/// directory should be is refused, and the file is written to a fresh `0600`
/// temporary file and renamed over the destination. A failed write leaves the
/// previous file in place.
pub fn write_api_key_in(dir: &Path, name: &str, value: &str) -> Result<PathBuf, String> {
    let name = validate_api_key_name(name)?;
    let value = validate_api_key_value(value)?;
    prepare_private_dir(dir)?;
    let path = dir.join(name);
    let temp = dir.join(format!(".{name}.{}.tmp", uuid::Uuid::new_v4()));

    let result = (|| -> Result<(), String> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temp)
            .map_err(|error| format!("Could not create the API-key file: {error}"))?;
        file.write_all(value.as_bytes())
            .map_err(|error| format!("Could not write the API-key file: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("Could not finish the API-key file: {error}"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&temp, fs::Permissions::from_mode(0o600))
                .map_err(|error| format!("Could not protect the API-key file: {error}"))?;
        }
        drop(file);
        replace_file(&temp, &path)
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result.map(|()| path)
}

/// Remove the file for a validated name. Returns false when it was not there.
pub fn remove_api_key_file(name: &str) -> Result<bool, String> {
    remove_api_key_file_in(&api_keys_dir(), name)
}

/// Remove one key file under an explicit directory.
pub fn remove_api_key_file_in(dir: &Path, name: &str) -> Result<bool, String> {
    let name = validate_api_key_name(name)?;
    let path = dir.join(name);
    match fs::symlink_metadata(&path) {
        Ok(meta) if meta.is_dir() => {
            Err("The API-key path is a directory, not a file.".to_string())
        }
        Ok(_) => fs::remove_file(&path)
            .map(|()| true)
            .map_err(|error| format!("Could not remove the API-key file: {error}")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("Could not inspect the API-key file: {error}")),
    }
}

/// Whether a key file exists for a validated name.
pub fn api_key_file_exists(name: &str) -> Result<bool, String> {
    let path = api_key_path(name)?;
    Ok(fs::symlink_metadata(path).is_ok())
}

fn prepare_private_dir(dir: &Path) -> Result<(), String> {
    match fs::symlink_metadata(dir) {
        Ok(meta) => {
            if meta.file_type().is_symlink() || !meta.is_dir() {
                return Err(
                    "The API-key directory is a symlink or not a directory; refusing to write."
                        .to_string(),
                );
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(dir)
                .map_err(|error| format!("Could not create the API-key directory: {error}"))?;
        }
        Err(error) => return Err(format!("Could not inspect the API-key directory: {error}")),
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("Could not protect the API-key directory: {error}"))?;
    }
    Ok(())
}

fn replace_file(temp: &Path, path: &Path) -> Result<(), String> {
    match fs::rename(temp, path) {
        Ok(()) => Ok(()),
        // Windows will not replace an existing file in one rename. The
        // destination is a file we own in a private directory, so removing it
        // first is safe; on Unix this branch is normally unreachable.
        Err(first) if path.exists() => {
            fs::remove_file(path)
                .map_err(|error| format!("Could not replace the API-key file: {error}"))?;
            fs::rename(temp, path)
                .map_err(|_| format!("Could not replace the API-key file: {first}"))
        }
        Err(error) => Err(format!("Could not replace the API-key file: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("rb-api-{tag}-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn names_are_lowercase_and_cannot_escape_the_directory() {
        for name in ["openai", "anthropic", "github.token", "my_key-2", "a1"] {
            assert_eq!(validate_api_key_name(name).unwrap(), name);
        }
        let long = "a".repeat(MAX_NAME + 1);
        for name in [
            "",
            "OpenAI",
            "../secret",
            ".hidden",
            "a/b",
            "a b",
            "a.b.",
            "con",
            "con.txt",
            "com1",
            "com1.key",
            long.as_str(),
        ] {
            assert!(
                validate_api_key_name(name).is_err(),
                "{name:?} must be refused"
            );
        }
    }

    #[test]
    fn values_are_one_line_and_bounded() {
        assert_eq!(
            validate_api_key_value("  sk-test-123\n").unwrap(),
            "sk-test-123"
        );
        assert!(validate_api_key_value("sk-test 123").is_err());
        assert!(validate_api_key_value("sk-test\n123").is_err());
        assert!(validate_api_key_value("").is_err());
        assert!(validate_api_key_value(&"a".repeat(MAX_VALUE_BYTES + 1)).is_err());
    }

    #[test]
    fn a_written_key_is_private_and_replaced_atomically() {
        let dir = temp_dir("write");
        let path = write_api_key_in(&dir, "openai", "sk-first").expect("write");
        assert_eq!(fs::read_to_string(&path).unwrap(), "sk-first");
        let second = write_api_key_in(&dir, "openai", "sk-second").expect("replace");
        assert_eq!(second, path);
        assert_eq!(fs::read_to_string(&path).unwrap(), "sk-second");
        let entries: Vec<_> = fs::read_dir(&dir).unwrap().collect();
        assert_eq!(entries.len(), 1, "no temporary file may be left behind");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn removal_is_idempotent_and_does_not_touch_other_files() {
        let dir = temp_dir("remove");
        write_api_key_in(&dir, "openai", "sk-test").expect("write");
        write_api_key_in(&dir, "other", "sk-other").expect("write");
        assert!(remove_api_key_file_in(&dir, "openai").expect("remove"));
        assert!(!remove_api_key_file_in(&dir, "openai").expect("second remove"));
        assert!(dir.join("other").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_key_directory_is_refused() {
        use std::os::unix::fs::symlink;
        let real = temp_dir("real");
        let link = temp_dir("link");
        fs::create_dir_all(&real).unwrap();
        symlink(&real, &link).unwrap();
        let error = write_api_key_in(&link, "openai", "sk-test").expect_err("symlink");
        assert!(error.contains("symlink"));
        assert!(!real.join("openai").exists());
        let _ = fs::remove_dir_all(&real);
        let _ = fs::remove_file(&link);
    }

    #[test]
    fn the_default_path_hangs_off_the_keys_directory() {
        let path = api_key_path("openai").expect("path");
        assert!(path.ends_with("keys/api-keys/openai"));
    }
}

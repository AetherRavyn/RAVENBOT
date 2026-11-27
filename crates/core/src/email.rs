//! Validation shared by the agent email draft tool and the desktop commands.
//!
//! The rules are deliberately conservative and identical on both sides of the
//! IPC boundary: a draft that the agent may create is a draft a person may
//! later send, and an allowlist saved in Settings must mean the same thing to
//! both. Nothing in this module touches the network.

/// Maximum recipients in one To or Cc field.
pub const MAX_RECIPIENTS_PER_FIELD: usize = 20;
/// Maximum length of a single address, from RFC 5321's path limit.
pub const MAX_ADDRESS_LENGTH: usize = 320;
/// Maximum subject length, in characters.
pub const MAX_SUBJECT_LENGTH: usize = 200;
/// Maximum body length, in characters.
pub const MAX_BODY_LENGTH: usize = 50_000;
/// Maximum entries in the saved allowlist.
pub const MAX_ALLOWLIST_ENTRIES: usize = 200;

/// The `app_settings` key holding the comma-separated allowlist.
pub const ALLOWLIST_SETTING_KEY: &str = "agent_email_allowlist";

/// Validate one address and return it trimmed, exactly as the user wrote it.
///
/// "Plausible" is the standard here, not RFC-complete: one `@`, a non-empty
/// local part and domain, no whitespace or control characters, and at most 320
/// characters. `lettre` does the stricter parse when a message is built, but a
/// draft must be rejected before it reaches a transport.
pub fn validate_email_address(raw: &str, label: &str) -> Result<String, String> {
    let address = raw.trim();
    if address.is_empty() {
        return Err(format!("The {label} address is empty."));
    }
    if address.chars().count() > MAX_ADDRESS_LENGTH {
        return Err(format!(
            "The {label} address is longer than {MAX_ADDRESS_LENGTH} characters."
        ));
    }
    if address.chars().any(char::is_whitespace) || address.chars().any(char::is_control) {
        return Err(format!(
            "The {label} address contains whitespace or control characters."
        ));
    }
    let mut parts = address.split('@');
    let local = parts.next().unwrap_or_default();
    let domain = parts.next().unwrap_or_default();
    if local.is_empty() || domain.is_empty() || parts.next().is_some() {
        return Err(format!(
            "The {label} address is not valid: use one @ with text on both sides."
        ));
    }
    Ok(address.to_string())
}

/// Parse a comma/semicolon separated recipient field.
///
/// `required` distinguishes To (1–20) from Cc (0–20). Empty segments are
/// ignored, so a trailing comma is not an error.
pub fn parse_recipient_list(raw: &str, label: &str, required: bool) -> Result<Vec<String>, String> {
    let mut addresses = Vec::new();
    for part in raw.split([',', ';']) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        addresses.push(validate_email_address(part, label)?);
        if addresses.len() > MAX_RECIPIENTS_PER_FIELD {
            return Err(format!(
                "A {label} field may contain at most {MAX_RECIPIENTS_PER_FIELD} recipients."
            ));
        }
    }
    if required && addresses.is_empty() {
        return Err(format!("Add at least one {label} recipient."));
    }
    Ok(addresses)
}

/// Validate an email subject: 1–200 characters, no control characters.
pub fn validate_subject(raw: &str) -> Result<String, String> {
    let subject = raw.trim();
    let length = subject.chars().count();
    if length == 0 || length > MAX_SUBJECT_LENGTH {
        return Err(format!(
            "The subject must be between 1 and {MAX_SUBJECT_LENGTH} characters."
        ));
    }
    if subject.chars().any(char::is_control) {
        return Err("The subject contains a control character.".to_string());
    }
    Ok(subject.to_string())
}

/// Validate a plain-text body: 1–50,000 characters, with tabs and newlines but
/// no other control characters (including NUL and carriage returns).
pub fn validate_body(raw: &str) -> Result<String, String> {
    let length = raw.chars().count();
    if length == 0 || length > MAX_BODY_LENGTH {
        return Err(format!(
            "The body must be between 1 and {MAX_BODY_LENGTH} characters."
        ));
    }
    if raw
        .chars()
        .any(|character| character.is_control() && character != '\n' && character != '\t')
    {
        return Err(
            "The body contains a control character other than a newline or tab.".to_string(),
        );
    }
    Ok(raw.to_string())
}

/// Validate a domain entry in the allowlist. No scheme, path, wildcard or
/// address-local punctuation is accepted here; an entry containing `@` is
/// handled as an exact address instead.
fn validate_allowlist_domain(raw: &str) -> Result<String, String> {
    let domain = raw.trim().to_ascii_lowercase();
    if domain.is_empty() || domain.chars().count() > 253 {
        return Err("An allowlist domain is empty or longer than 253 characters.".to_string());
    }
    if domain == "*" {
        return Err(
            "Wildcards are not supported in the email allowlist; list a domain or an address."
                .to_string(),
        );
    }
    if domain.starts_with('.') || domain.ends_with('.') || domain.contains("..") {
        return Err(format!("The allowlist domain '{raw}' is not valid."));
    }
    for label in domain.split('.') {
        if label.is_empty()
            || label.starts_with('-')
            || label.ends_with('-')
            || !label
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '-')
        {
            return Err(format!("The allowlist domain '{raw}' is not valid."));
        }
    }
    if !domain.contains('.') {
        // A bare single label is a hostname, not an internet domain. Rejecting
        // it avoids an allowlist entry that silently matches nothing.
        return Err(format!(
            "The allowlist entry '{raw}' needs a domain such as example.com or a full address."
        ));
    }
    Ok(domain)
}

/// Parse and normalise the saved allowlist.
///
/// An entry containing `@` is an exact address (case-insensitive). Any other
/// entry is a domain that also covers its subdomains. An empty result means
/// "any recipient". Duplicate entries are collapsed.
pub fn validate_allowlist(raw: &str) -> Result<Vec<String>, String> {
    let mut entries: Vec<String> = Vec::new();
    for part in raw.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let entry = if part.contains('@') {
            validate_email_address(part, "allowlist")?.to_ascii_lowercase()
        } else {
            validate_allowlist_domain(part)?
        };
        if !entries.contains(&entry) {
            entries.push(entry);
        }
        if entries.len() > MAX_ALLOWLIST_ENTRIES {
            return Err(format!(
                "The email allowlist may contain at most {MAX_ALLOWLIST_ENTRIES} entries."
            ));
        }
    }
    Ok(entries)
}

/// Whether one address is covered by the allowlist.
///
/// An empty allowlist covers everyone. A domain entry covers the domain and
/// every subdomain but not a lookalike such as `notexample.com`.
pub fn address_allowed(address: &str, allowlist: &[String]) -> bool {
    if allowlist.is_empty() {
        return true;
    }
    let address = address.trim().to_ascii_lowercase();
    let Some((_, domain)) = address.rsplit_once('@') else {
        return false;
    };
    allowlist.iter().any(|entry| {
        let entry = entry.trim().to_ascii_lowercase();
        if entry.is_empty() {
            false
        } else if entry.contains('@') {
            entry == address
        } else {
            domain == entry || domain.ends_with(&format!(".{entry}"))
        }
    })
}

/// Reject the first recipient not covered by the allowlist, naming it so the
/// agent or person can correct the draft.
pub fn ensure_allowlisted(addresses: &[String], allowlist: &[String]) -> Result<(), String> {
    if let Some(denied) = addresses
        .iter()
        .find(|address| !address_allowed(address, allowlist))
    {
        return Err(format!(
            "The email allowlist does not allow {denied}. Add that address or its domain in Settings → Mail, or remove it from the draft."
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_need_one_at_and_no_whitespace() {
        assert_eq!(
            validate_email_address("  person@example.com ", "To").unwrap(),
            "person@example.com"
        );
        for bad in [
            "",
            "plain",
            "@example.com",
            "person@",
            "a@b@c",
            "person @example.com",
            "person@exa\nmple.com",
            "person@example.com\rbcc",
        ] {
            assert!(
                validate_email_address(bad, "To").is_err(),
                "must reject {bad:?}"
            );
        }
        // Surrounding whitespace is trimmed by the field parser.
        assert_eq!(
            validate_email_address(" person@example.com\n", "To").unwrap(),
            "person@example.com"
        );
        let too_long = format!("{}@example.com", "a".repeat(320));
        assert!(validate_email_address(&too_long, "To").is_err());
    }

    #[test]
    fn recipient_lists_are_bounded_and_accept_commas_or_semicolons() {
        let list = parse_recipient_list("a@example.com; b@example.com, c@example.com,", "To", true)
            .unwrap();
        assert_eq!(list.len(), 3);
        assert!(parse_recipient_list("   ", "To", true).is_err());
        assert!(parse_recipient_list("", "Cc", false).unwrap().is_empty());

        let too_many = (0..21)
            .map(|index| format!("person{index}@example.com"))
            .collect::<Vec<_>>()
            .join(",");
        assert!(parse_recipient_list(&too_many, "To", true).is_err());
    }

    #[test]
    fn subject_and_body_limits_are_character_based() {
        assert_eq!(validate_subject("  Hello  ").unwrap(), "Hello");
        assert!(validate_subject("").is_err());
        assert!(validate_subject(&"s".repeat(201)).is_err());
        assert!(validate_subject("Hello\nBcc: x@example.com").is_err());

        assert_eq!(validate_body("one\ntwo\tthree").unwrap(), "one\ntwo\tthree");
        assert!(validate_body("").is_err());
        assert!(validate_body("body\0").is_err());
        assert!(validate_body("body\r\n").is_err());
        assert!(validate_body(&"b".repeat(50_001)).is_err());
    }

    #[test]
    fn allowlist_accepts_exact_addresses_and_domains() {
        let parsed =
            validate_allowlist("Person@Example.com, example.org, mail.example.net").unwrap();
        assert_eq!(
            parsed,
            vec![
                "person@example.com".to_string(),
                "example.org".to_string(),
                "mail.example.net".to_string()
            ]
        );
        assert!(validate_allowlist("*").is_err());
        assert!(validate_allowlist("bad domain").is_err());
        assert!(validate_allowlist("example").is_err());
        assert!(validate_allowlist("person@").is_err());
    }

    #[test]
    fn domains_cover_subdomains_but_not_lookalikes() {
        let allowlist = vec!["example.com".to_string(), "person@other.test".to_string()];
        assert!(address_allowed("anyone@example.com", &allowlist));
        assert!(address_allowed("anyone@team.example.com", &allowlist));
        assert!(address_allowed("Person@Other.Test", &allowlist));
        assert!(address_allowed("person@example.com", &allowlist));
        assert!(!address_allowed("anyone@notexample.com", &allowlist));
        assert!(!address_allowed("someone-else@other.test", &allowlist));
        assert!(address_allowed("anyone@anywhere.test", &[]));
        assert!(ensure_allowlisted(&["a@example.com".into()], &allowlist).is_ok());
        let err = ensure_allowlisted(&["a@blocked.test".into()], &allowlist).unwrap_err();
        assert!(err.contains("a@blocked.test"), "{err}");
        assert!(err.contains("Settings → Mail"), "{err}");
    }
}

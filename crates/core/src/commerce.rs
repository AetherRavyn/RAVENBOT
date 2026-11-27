//! Spend-request validation and policy shared by the agent tool and the
//! owner-facing settings.
//!
//! Commerce v1 never moves money. An agent may only create a pending request;
//! a person approves or denies it, opens the merchant checkout in a browser,
//! and marks the result. No card data, payment rail, order API, tax, shipping
//! or currency conversion exists in this module.

use url::Url;

/// Maximum length of a request title, in characters.
pub const MAX_TITLE_LENGTH: usize = 200;
/// Maximum length of the merchant display name, in characters.
pub const MAX_MERCHANT_LENGTH: usize = 200;
/// Maximum length of the reason shown to the approver, in characters.
pub const MAX_REASON_LENGTH: usize = 2_000;
/// Maximum entries in the saved merchant allowlist.
pub const MAX_ALLOWLIST_ENTRIES: usize = 100;
/// Maximum amount in minor units. Keeps sums well inside SQLite's i64 range.
pub const MAX_AMOUNT_MINOR: u64 = 1_000_000_000_000;

/// `app_settings` key for the master switch. Defaults to off.
pub const ENABLED_SETTING_KEY: &str = "commerce_enabled";
/// `app_settings` key for the three-letter currency code.
pub const CURRENCY_SETTING_KEY: &str = "commerce_currency";
/// `app_settings` key for the per-request limit, in minor units.
pub const PER_REQUEST_LIMIT_SETTING_KEY: &str = "commerce_per_request_limit_minor";
/// `app_settings` key for the daily limit, in minor units.
pub const DAILY_LIMIT_SETTING_KEY: &str = "commerce_daily_limit_minor";
/// `app_settings` key for the merchant host allowlist.
pub const ALLOWLIST_SETTING_KEY: &str = "commerce_merchant_allowlist";

/// A validated spend request, ready to store as `pending`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpendRequestInput {
    pub title: String,
    pub merchant: String,
    pub url: String,
    pub amount_minor: u64,
    pub currency: String,
    pub reason: String,
}

/// The owner's saved policy. Construction validates every field, so an
/// instance of this type can be trusted by the tool and the approval command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpendPolicy {
    pub enabled: bool,
    pub currency: String,
    pub per_request_limit_minor: u64,
    pub daily_limit_minor: u64,
    pub allowed_merchants: Vec<String>,
}

fn validate_single_line(raw: &str, label: &str, max: usize) -> Result<String, String> {
    let value = raw.trim();
    if value.is_empty() {
        return Err(format!("The {label} is empty."));
    }
    if value.chars().count() > max {
        return Err(format!("The {label} is longer than {max} characters."));
    }
    if value.chars().any(char::is_control) {
        return Err(format!(
            "The {label} contains a control character or line break."
        ));
    }
    Ok(value.to_string())
}

/// Validate a request title: one line, 1–200 characters, no control characters.
pub fn validate_title(raw: &str) -> Result<String, String> {
    validate_single_line(raw, "spend title", MAX_TITLE_LENGTH)
}

/// Validate a merchant display name: one line, 1–200 characters.
pub fn validate_merchant(raw: &str) -> Result<String, String> {
    validate_single_line(raw, "merchant name", MAX_MERCHANT_LENGTH)
}

/// Validate the reason the agent gives the approver: 1–2,000 characters.
pub fn validate_reason(raw: &str) -> Result<String, String> {
    let value = raw.trim();
    if value.is_empty() {
        return Err("The reason is empty. Explain what the purchase is for.".to_string());
    }
    if value.chars().count() > MAX_REASON_LENGTH {
        return Err(format!(
            "The reason is longer than {MAX_REASON_LENGTH} characters."
        ));
    }
    if value
        .chars()
        .any(|character| character.is_control() && character != '\n' && character != '\t')
    {
        return Err("The reason contains an unsupported control character.".to_string());
    }
    Ok(value.to_string())
}

/// Normalize a three-letter ASCII currency code. No conversion is performed.
pub fn validate_currency(raw: &str) -> Result<String, String> {
    let value = raw.trim().to_ascii_uppercase();
    if value.len() != 3
        || !value
            .chars()
            .all(|character| character.is_ascii_alphabetic())
    {
        return Err("The currency must be a three-letter code such as USD or EUR.".to_string());
    }
    Ok(value)
}

/// Validate a positive amount in minor units (cents for USD/EUR).
pub fn validate_amount_minor(amount_minor: u64) -> Result<u64, String> {
    if amount_minor == 0 {
        return Err("The amount must be greater than zero.".to_string());
    }
    if amount_minor > MAX_AMOUNT_MINOR {
        return Err(format!(
            "The amount is above the supported maximum of {MAX_AMOUNT_MINOR} minor units."
        ));
    }
    Ok(amount_minor)
}

fn validate_hostname(raw: &str) -> Result<String, String> {
    let host = raw.trim().trim_start_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return Err("A merchant allowlist entry is empty.".to_string());
    }
    if host.len() > 253 {
        return Err(format!("The merchant domain `{host}` is too long."));
    }
    if host.contains("://")
        || host.contains('/')
        || host.contains(':')
        || host.contains('@')
        || host.contains(' ')
    {
        return Err(format!(
            "The merchant allowlist entry `{host}` must be a bare domain such as shop.example.com."
        ));
    }
    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() < 2 {
        return Err(format!(
            "The merchant allowlist entry `{host}` must include a dot, such as example.com."
        ));
    }
    for label in labels {
        if label.is_empty() || label.len() > 63 {
            return Err(format!(
                "The merchant domain `{host}` has an invalid label."
            ));
        }
        if label.starts_with('-') || label.ends_with('-') {
            return Err(format!(
                "The merchant domain `{host}` has an invalid label."
            ));
        }
        if !label
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
        {
            return Err(format!(
                "The merchant domain `{host}` may contain only letters, digits, dots and hyphens."
            ));
        }
    }
    Ok(host)
}

/// Parse a comma/semicolon/whitespace separated allowlist of bare hostnames.
///
/// Entries are lower-cased and de-duplicated. A URL, a port or a path is
/// rejected instead of being silently trimmed into a different host.
pub fn parse_merchant_allowlist(raw: &str) -> Result<Vec<String>, String> {
    let mut entries: Vec<String> = Vec::new();
    for part in raw.split([',', ';', '\n', '\r', '\t', ' ']) {
        if part.trim().is_empty() {
            continue;
        }
        let host = validate_hostname(part)?;
        if !entries.contains(&host) {
            entries.push(host);
        }
        if entries.len() > MAX_ALLOWLIST_ENTRIES {
            return Err(format!(
                "The merchant allowlist may contain at most {MAX_ALLOWLIST_ENTRIES} entries."
            ));
        }
    }
    Ok(entries)
}

fn host_is_allowed(host: &str, allowed: &[String]) -> bool {
    let host = host.to_ascii_lowercase();
    allowed
        .iter()
        .any(|entry| host == *entry || host.ends_with(&format!(".{entry}")))
}

/// Validate an HTTPS checkout URL and return it normalized.
///
/// Credentials, non-HTTPS schemes and URLs without a host are rejected. The
/// host is matched against the allowlist separately by [`SpendPolicy::check`].
pub fn validate_checkout_url(raw: &str) -> Result<String, String> {
    let value = raw.trim();
    if value.is_empty() {
        return Err("The checkout URL is empty.".to_string());
    }
    let parsed = Url::parse(value).map_err(|_| {
        "The checkout URL is not a valid absolute URL. Use an https:// link.".to_string()
    })?;
    if parsed.scheme() != "https" {
        return Err("The checkout URL must start with https://.".to_string());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("The checkout URL must not contain a username or password.".to_string());
    }
    if parsed.host_str().is_none() {
        return Err("The checkout URL has no host.".to_string());
    }
    let host = parsed.host_str().unwrap_or_default();
    if !host.contains('.') {
        return Err("The checkout URL host must be a domain such as shop.example.com.".to_string());
    }
    Ok(parsed.to_string())
}

impl SpendRequestInput {
    /// Validate and normalize every field. The order is deliberate: cheap text
    /// checks first, URL parsing last.
    #[allow(clippy::too_many_arguments)]
    pub fn validate(
        title: &str,
        merchant: &str,
        url: &str,
        amount_minor: u64,
        currency: &str,
        reason: &str,
    ) -> Result<Self, String> {
        Ok(Self {
            title: validate_title(title)?,
            merchant: validate_merchant(merchant)?,
            url: validate_checkout_url(url)?,
            amount_minor: validate_amount_minor(amount_minor)?,
            currency: validate_currency(currency)?,
            reason: validate_reason(reason)?,
        })
    }

    /// The hostname used for allowlist matching.
    pub fn host(&self) -> String {
        Url::parse(&self.url)
            .ok()
            .and_then(|url| url.host_str().map(|host| host.to_ascii_lowercase()))
            .unwrap_or_default()
    }
}

impl SpendPolicy {
    /// Build a policy from raw `app_settings` values.
    ///
    /// An invalid saved policy fails closed. Amounts use minor units, so the
    /// operator's "cents" and the stored integer can never drift apart.
    pub fn from_stored(
        enabled: bool,
        currency: &str,
        per_request_limit_minor: &str,
        daily_limit_minor: &str,
        allowlist: &str,
    ) -> Result<Self, String> {
        let currency = validate_currency(currency)?;
        let per_request_limit_minor =
            parse_limit(per_request_limit_minor, "per-request limit", enabled)?;
        let daily_limit_minor = parse_limit(daily_limit_minor, "daily limit", enabled)?;
        if enabled && (per_request_limit_minor == 0 || daily_limit_minor == 0) {
            return Err(
                "Commerce limits must be greater than zero when commerce is enabled.".to_string(),
            );
        }
        if enabled && per_request_limit_minor > daily_limit_minor {
            return Err(
                "The per-request limit cannot be greater than the daily limit.".to_string(),
            );
        }
        let allowed_merchants = parse_merchant_allowlist(allowlist)?;
        if enabled && allowed_merchants.is_empty() {
            return Err(
                "Commerce is enabled but the merchant allowlist is empty. Add at least one domain."
                    .to_string(),
            );
        }
        Ok(Self {
            enabled,
            currency,
            per_request_limit_minor,
            daily_limit_minor,
            allowed_merchants,
        })
    }

    /// Re-check a request against the current policy. Called when the agent
    /// creates the request and again when the owner approves it, so a policy
    /// edit between those moments cannot be bypassed by an old row.
    pub fn check(&self, request: &SpendRequestInput, spent_today_minor: u64) -> Result<(), String> {
        if !self.enabled {
            return Err(
                "Commerce is disabled. An owner must enable it in Settings → Commerce.".to_string(),
            );
        }
        if request.currency != self.currency {
            return Err(format!(
                "This request is in {}, but the workspace policy uses {}.",
                request.currency, self.currency
            ));
        }
        if request.amount_minor > self.per_request_limit_minor {
            return Err(format!(
                "The amount exceeds the per-request limit of {} {} minor units.",
                self.per_request_limit_minor, self.currency
            ));
        }
        let total = spent_today_minor
            .checked_add(request.amount_minor)
            .ok_or_else(|| "The daily spend total overflowed.".to_string())?;
        if total > self.daily_limit_minor {
            return Err(format!(
                "Approving this would exceed the daily limit of {} {} minor units (already committed today: {}).",
                self.daily_limit_minor, self.currency, spent_today_minor
            ));
        }
        let host = request.host();
        if host.is_empty() || !host_is_allowed(&host, &self.allowed_merchants) {
            return Err(format!(
                "The checkout host `{host}` is not in the merchant allowlist."
            ));
        }
        Ok(())
    }

    /// Check only the fields that can change between request and approval.
    /// Daily spend is added by the caller because it lives in the database.
    pub fn check_static(&self, request: &SpendRequestInput) -> Result<(), String> {
        if !self.enabled {
            return Err(
                "Commerce is disabled. An owner must enable it in Settings → Commerce.".to_string(),
            );
        }
        if request.currency != self.currency {
            return Err(format!(
                "This request is in {}, but the workspace policy uses {}.",
                request.currency, self.currency
            ));
        }
        if request.amount_minor > self.per_request_limit_minor {
            return Err(format!(
                "The amount exceeds the per-request limit of {} {} minor units.",
                self.per_request_limit_minor, self.currency
            ));
        }
        let host = request.host();
        if host.is_empty() || !host_is_allowed(&host, &self.allowed_merchants) {
            return Err(format!(
                "The checkout host `{host}` is not in the merchant allowlist."
            ));
        }
        Ok(())
    }
}

fn parse_limit(raw: &str, label: &str, required: bool) -> Result<u64, String> {
    let value = raw.trim();
    if value.is_empty() {
        if required {
            return Err(format!(
                "The {label} is empty. Enter an amount in minor units."
            ));
        }
        return Ok(0);
    }
    if !value.chars().all(|character| character.is_ascii_digit()) {
        return Err(format!(
            "The {label} must be a whole number of minor units (cents for USD/EUR)."
        ));
    }
    let parsed = value
        .parse::<u64>()
        .map_err(|_| format!("The {label} is too large."))?;
    if parsed == 0 && !required {
        return Ok(0);
    }
    validate_amount_minor(parsed).map_err(|error| format!("The {label} is invalid: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> SpendRequestInput {
        SpendRequestInput::validate(
            "Replacement SSD",
            "Example Store",
            "https://shop.example.com/checkout/ssd",
            12_500,
            "usd",
            "The test machine's drive failed.",
        )
        .expect("valid request")
    }

    fn policy() -> SpendPolicy {
        SpendPolicy::from_stored(
            true,
            "USD",
            "20000",
            "50000",
            "example.com, shop.example.org",
        )
        .expect("valid policy")
    }

    #[test]
    fn validates_and_normalizes_a_request() {
        let request = request();
        assert_eq!(request.currency, "USD");
        assert_eq!(request.url, "https://shop.example.com/checkout/ssd");
        assert_eq!(request.host(), "shop.example.com");
        assert_eq!(request.amount_minor, 12_500);
    }

    #[test]
    fn rejects_empty_or_overlong_text_fields() {
        assert!(validate_title("  ").is_err());
        assert!(validate_title(&"x".repeat(MAX_TITLE_LENGTH + 1)).is_err());
        assert!(validate_merchant("two\nlines").is_err());
        assert!(validate_reason("").is_err());
        assert!(validate_reason(&"x".repeat(MAX_REASON_LENGTH + 1)).is_err());
        assert!(validate_reason("line one\nline two\ttab").is_ok());
    }

    #[test]
    fn rejects_zero_or_negative_amounts_and_bad_currency() {
        assert!(validate_amount_minor(0).is_err());
        assert!(SpendRequestInput::validate(
            "Drive",
            "Store",
            "https://shop.example.com/checkout",
            0,
            "USD",
            "reason"
        )
        .is_err());
        assert!(validate_currency("US").is_err());
        assert!(validate_currency("US1").is_err());
        assert_eq!(validate_currency(" eur ").unwrap(), "EUR");
    }

    #[test]
    fn only_accepts_https_urls_without_credentials() {
        assert!(validate_checkout_url("http://shop.example.com/checkout").is_err());
        assert!(validate_checkout_url("javascript:alert(1)").is_err());
        assert!(validate_checkout_url("https://user:pass@shop.example.com/checkout").is_err());
        assert!(validate_checkout_url("https:///checkout").is_err());
        assert_eq!(
            validate_checkout_url("https://shop.example.com/checkout?a=1").unwrap(),
            "https://shop.example.com/checkout?a=1"
        );
    }

    #[test]
    fn allowlist_entries_are_bare_hosts_and_deduplicated() {
        let entries =
            parse_merchant_allowlist("Example.com, shop.example.com\nEXAMPLE.com").unwrap();
        assert_eq!(entries, vec!["example.com", "shop.example.com"]);
        assert!(parse_merchant_allowlist("https://example.com").is_err());
        assert!(parse_merchant_allowlist("example.com:443").is_err());
        assert!(parse_merchant_allowlist("localhost").is_err());
        assert!(parse_merchant_allowlist("-bad.example.com").is_err());
    }

    #[test]
    fn host_matching_requires_the_boundary_and_allows_subdomains() {
        let allowed = parse_merchant_allowlist("example.com").unwrap();
        assert!(host_is_allowed("example.com", &allowed));
        assert!(host_is_allowed("shop.example.com", &allowed));
        assert!(!host_is_allowed("evil-example.com", &allowed));
        assert!(!host_is_allowed("example.com.evil.test", &allowed));
    }

    #[test]
    fn accepts_a_request_inside_the_policy() {
        policy().check(&request(), 10_000).expect("allowed");
    }

    #[test]
    fn fails_closed_when_disabled_or_over_a_limit() {
        let mut disabled = policy();
        disabled.enabled = false;
        assert!(disabled.check(&request(), 0).is_err());

        let err = policy().check(&request(), 40_000).unwrap_err();
        assert!(err.contains("daily limit"));

        let mut low = policy();
        low.per_request_limit_minor = 10_000;
        let err = low.check(&request(), 0).unwrap_err();
        assert!(err.contains("per-request limit"));
    }

    #[test]
    fn rejects_a_currency_mismatch_and_a_host_outside_the_allowlist() {
        let mut eur = policy();
        eur.currency = "EUR".to_string();
        assert!(eur.check(&request(), 0).unwrap_err().contains("uses EUR"));

        let mut other = policy();
        other.allowed_merchants = vec!["not-example.com".to_string()];
        assert!(other
            .check(&request(), 0)
            .unwrap_err()
            .contains("not in the merchant allowlist"));
    }

    #[test]
    fn refuses_to_build_an_enabled_policy_without_merchants() {
        assert!(SpendPolicy::from_stored(true, "USD", "100", "100", "").is_err());
        assert!(SpendPolicy::from_stored(false, "USD", "100", "100", "").is_ok());
        assert!(SpendPolicy::from_stored(false, "USD", "", "", "").is_ok());
        assert!(SpendPolicy::from_stored(true, "USD", "", "", "example.com").is_err());
        assert!(SpendPolicy::from_stored(true, "USD", "200", "100", "example.com").is_err());
        assert!(SpendPolicy::from_stored(true, "USD", "10.5", "100", "example.com").is_err());
        assert!(SpendPolicy::from_stored(true, "USD", "0", "100", "example.com").is_err());
    }
}

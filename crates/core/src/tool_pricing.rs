//! Built-in price adapters for known paid tools.
//!
//! Provider billing is not uniform, so RAVENBOT cannot read a live per-call
//! price from every tool. The entries here are conservative **estimates** for
//! the paid paths the built-in skills can take. They are used only when the
//! user has not configured an exact per-call price in Settings; the user price
//! always wins.
//!
//! A tool whose paid path depends on a key is inactive while that key is
//! absent, because the built-in fallback is free. Reporting it as paid would
//! overstate spend and train users to ignore the number.

use serde::Serialize;

/// A known tool that can make a paid provider call.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PaidTool {
    /// Skill id as it appears in tool traces.
    pub id: &'static str,
    /// Human-readable label for the pricing table.
    pub label: &'static str,
    /// Conservative per-call estimate in USD. `0.0` means the provider does
    /// not publish a per-call price, so the tool is reported as unpriced.
    pub estimate_usd: f64,
    /// What the estimate represents.
    pub estimate_note: &'static str,
    /// Environment variable that enables the paid path. `None` means the tool
    /// is always paid.
    pub paid_when: Option<&'static str>,
}

/// The built-in catalog. Keep this in step with the skills that can reach a
/// metered provider; a missing entry silently reports real spend as free.
pub const PAID_TOOLS: &[PaidTool] = &[
    PaidTool {
        id: "tavily_search",
        label: "Tavily Search",
        // Basic search is one credit; pay-as-you-go is $0.008 per credit.
        estimate_usd: 0.008,
        estimate_note: "one Tavily basic-search credit at $0.008",
        paid_when: Some("TAVILY_API_KEY"),
    },
    PaidTool {
        id: "image_gen",
        label: "Image Generation",
        // DALL-E 3, the largest size the skill requests. Smaller sizes cost
        // less, so this is an upper bound rather than an exact charge.
        estimate_usd: 0.04,
        estimate_note: "one DALL-E 3 1024x1024 image at $0.04",
        paid_when: Some("OPENAI_API_KEY"),
    },
    PaidTool {
        id: "voice_output",
        label: "Voice Output",
        // OpenAI TTS bills per character, so a per-call number would be
        // invented. Report it as unpriced and let the user set a price.
        estimate_usd: 0.0,
        estimate_note: "OpenAI TTS bills per character; set an exact price",
        paid_when: Some("OPENAI_API_KEY"),
    },
    PaidTool {
        id: "voice_input",
        label: "Voice Input",
        // Whisper bills per minute.
        estimate_usd: 0.0,
        estimate_note: "Whisper bills per minute; set an exact price",
        paid_when: Some("OPENAI_API_KEY"),
    },
];

/// Where an effective price came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PriceSource {
    /// The user configured this exact price.
    User,
    /// A built-in provider estimate is in effect.
    Catalog,
    /// The tool is paid, but no per-call price is known.
    Unpriced,
    /// The paid path is not active in this environment.
    Inactive,
}

/// The price that will actually be charged for one call.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EffectiveToolPrice {
    pub id: &'static str,
    pub label: &'static str,
    /// The amount charged per call; `0.0` for inactive or unpriced tools.
    pub price_usd: f64,
    pub source: PriceSource,
    pub note: &'static str,
}

/// Look up a catalog entry by skill id.
pub fn paid_tool(id: &str) -> Option<&'static PaidTool> {
    PAID_TOOLS.iter().find(|tool| tool.id == id)
}

/// Resolve one price against a caller-supplied environment check.
///
/// The environment predicate is injected so the paid/inactive decision can be
/// tested without mutating process-global state.
pub fn resolve_price(
    id: &str,
    user_price: Option<f64>,
    env_present: impl Fn(&str) -> bool,
) -> Option<EffectiveToolPrice> {
    resolve_price_from(id, user_price, PAID_TOOLS, &env_present)
}

fn resolve_price_from(
    id: &str,
    user_price: Option<f64>,
    catalog: &'static [PaidTool],
    env_present: &dyn Fn(&str) -> bool,
) -> Option<EffectiveToolPrice> {
    let entry = catalog.iter().find(|tool| tool.id == id)?;
    // Activity is checked before the user price: charging a configured price
    // while the keyless fallback is running would invent spend that never
    // happened.
    let active = entry.paid_when.is_none_or(env_present);
    if !active {
        return Some(EffectiveToolPrice {
            id: entry.id,
            label: entry.label,
            price_usd: 0.0,
            source: PriceSource::Inactive,
            note: "the paid provider is not configured; the free fallback is used",
        });
    }
    if let Some(price) = user_price.filter(|price| *price > 0.0) {
        return Some(EffectiveToolPrice {
            id: entry.id,
            label: entry.label,
            price_usd: price,
            source: PriceSource::User,
            note: "your configured per-call price",
        });
    }
    if entry.estimate_usd <= 0.0 {
        return Some(EffectiveToolPrice {
            id: entry.id,
            label: entry.label,
            price_usd: 0.0,
            source: PriceSource::Unpriced,
            note: entry.estimate_note,
        });
    }
    Some(EffectiveToolPrice {
        id: entry.id,
        label: entry.label,
        price_usd: entry.estimate_usd,
        source: PriceSource::Catalog,
        note: entry.estimate_note,
    })
}

/// The amount to charge for one call.
///
/// User prices are not limited to the built-in catalog: an MCP tool or a
/// plugin can cost money too, and the user may know its price. For a known
/// paid tool the activity check still wins, so a configured price is not
/// charged while the free fallback is running.
pub fn resolve_charge(
    id: &str,
    user_price: Option<f64>,
    env_present: impl Fn(&str) -> bool,
) -> f64 {
    match resolve_price(id, user_price, env_present) {
        Some(price) if price.source != PriceSource::Inactive => price.price_usd,
        Some(_) => 0.0,
        None => user_price.filter(|price| *price > 0.0).unwrap_or(0.0),
    }
}

/// Every known paid tool and its effective price in this environment.
pub fn price_status(
    user_prices: &std::collections::BTreeMap<String, f64>,
    env_present: impl Fn(&str) -> bool,
) -> Vec<EffectiveToolPrice> {
    PAID_TOOLS
        .iter()
        .filter_map(|tool| resolve_price(tool.id, user_prices.get(tool.id).copied(), &env_present))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn nothing(_: &str) -> bool {
        false
    }

    fn everything(_: &str) -> bool {
        true
    }

    #[test]
    fn a_configured_user_price_wins_while_the_paid_path_is_active() {
        let resolved = resolve_price("tavily_search", Some(0.25), |name| name == "TAVILY_API_KEY")
            .expect("known paid tool");
        assert_eq!(resolved.price_usd, 0.25);
        assert_eq!(resolved.source, PriceSource::User);

        // A configured price must not be charged when the free fallback runs.
        let inactive =
            resolve_price("tavily_search", Some(0.25), nothing).expect("known paid tool");
        assert_eq!(inactive.source, PriceSource::Inactive);
        assert_eq!(inactive.price_usd, 0.0);
    }

    #[test]
    fn the_catalog_estimate_applies_only_when_the_paid_path_is_active() {
        let inactive = resolve_price("tavily_search", None, nothing).expect("known tool");
        assert_eq!(inactive.source, PriceSource::Inactive);
        assert_eq!(inactive.price_usd, 0.0);

        let active = resolve_price("tavily_search", None, |name| name == "TAVILY_API_KEY")
            .expect("known tool");
        assert_eq!(active.source, PriceSource::Catalog);
        assert!((active.price_usd - 0.008).abs() < 1e-12);
    }

    #[test]
    fn an_unknown_tool_has_no_catalog_price_but_can_still_be_priced() {
        assert!(resolve_price("memory_save", None, everything).is_none());
        assert_eq!(resolve_charge("memory_save", Some(0.25), everything), 0.25);
        assert_eq!(resolve_charge("memory_save", None, everything), 0.0);
    }

    #[test]
    fn a_known_tool_is_not_charged_for_its_free_fallback() {
        assert_eq!(resolve_charge("tavily_search", Some(0.25), nothing), 0.0);
    }

    #[test]
    fn a_known_paid_tool_without_a_published_price_is_reported_unpriced() {
        static CATALOG: &[PaidTool] = &[PaidTool {
            id: "custom_image",
            label: "Custom Image",
            estimate_usd: 0.0,
            estimate_note: "billed by the configured endpoint",
            paid_when: None,
        }];
        let resolved =
            resolve_price_from("custom_image", None, CATALOG, &everything).expect("catalog entry");
        assert_eq!(resolved.source, PriceSource::Unpriced);
        assert_eq!(resolved.price_usd, 0.0);
    }

    #[test]
    fn status_reports_every_catalog_tool() {
        let mut prices = BTreeMap::new();
        prices.insert("image_gen".to_string(), 0.5);
        let status = price_status(&prices, |name| {
            name == "TAVILY_API_KEY" || name == "OPENAI_API_KEY"
        });
        let tavily = status
            .iter()
            .find(|entry| entry.id == "tavily_search")
            .expect("tavily");
        assert_eq!(tavily.source, PriceSource::Catalog);
        let image = status
            .iter()
            .find(|entry| entry.id == "image_gen")
            .expect("image");
        assert_eq!(image.source, PriceSource::User);
        assert_eq!(image.price_usd, 0.5);
    }
}

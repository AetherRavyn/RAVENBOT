//! Vector slide layouts, rendered to SVG.
//!
//! This is the drawing half of `make_slides`: fixed 1920×1080 layouts, a
//! validated theme, code-defined icons and charts, and text that is wrapped and
//! shrunk until it fits. No browser, chart runtime, or web font is involved, so
//! a deck renders identically on a machine with no network.
//!
//! Every string that reaches the SVG is escaped and every colour is validated
//! before it is interpolated. A model-authored deck is data; it must not be
//! able to inject markup into the document that becomes the slide image.

use serde::Deserialize;
use serde_json::Value;

pub const WIDTH: u32 = 1920;
pub const HEIGHT: u32 = 1080;
const MARGIN: f64 = 120.0;

/// The palette and typeface for a deck.
#[derive(Debug, Clone)]
pub struct Theme {
    pub background: String,
    pub surface: String,
    pub text: String,
    pub muted: String,
    pub accent: String,
    pub accent2: String,
    pub font: String,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            background: "#111318".to_string(),
            surface: "#1b1f27".to_string(),
            text: "#f8fafc".to_string(),
            muted: "#a8b0bf".to_string(),
            accent: "#79b8ff".to_string(),
            accent2: "#9ae6b4".to_string(),
            font: "Inter, system-ui, sans-serif".to_string(),
        }
    }
}

impl Theme {
    /// Build a theme from a JSON object, falling back per field. Values that
    /// are not safe colours are replaced, never interpolated.
    pub fn from_value(value: Option<&Value>) -> Self {
        let mut theme = Self::default();
        let Some(map) = value.and_then(Value::as_object) else {
            return theme;
        };
        let color = |key: &str, fallback: &str| {
            map.get(key)
                .and_then(Value::as_str)
                .and_then(safe_color)
                .unwrap_or_else(|| fallback.to_string())
        };
        theme.background = color("background", &theme.background);
        theme.surface = color("surface", &theme.surface);
        theme.text = color("text", &theme.text);
        theme.muted = color("muted", &theme.muted);
        theme.accent = color("accent", &theme.accent);
        theme.accent2 = color("accent2", &theme.accent2);
        if let Some(font) = map.get("font").and_then(Value::as_str) {
            theme.font = safe_font(font);
        }
        theme
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CardSpec {
    #[serde(default)]
    pub title: String,
    #[serde(default, alias = "text", alias = "description")]
    pub body: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct StatSpec {
    #[serde(default)]
    pub value: String,
    #[serde(default, alias = "text")]
    pub label: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ItemSpec {
    #[serde(default, alias = "date", alias = "time")]
    pub when: String,
    #[serde(default, alias = "title", alias = "text")]
    pub what: String,
}

/// One slide as written by the model. Unknown fields are ignored so a model
/// cannot fail a whole deck by inventing an extra key.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SlideSpec {
    #[serde(default)]
    pub layout: String,
    #[serde(default)]
    pub kicker: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub subtitle: Option<String>,
    #[serde(default)]
    pub bullets: Vec<String>,
    #[serde(default)]
    pub cards: Vec<CardSpec>,
    #[serde(default)]
    pub stats: Vec<StatSpec>,
    #[serde(default)]
    pub items: Vec<ItemSpec>,
    #[serde(default)]
    pub quote: Option<String>,
    #[serde(default)]
    pub attribution: Option<String>,
    #[serde(default)]
    pub chart: Option<Value>,
    #[serde(default)]
    pub footer: Option<String>,
    #[serde(default)]
    pub number: Option<String>,
}

impl SlideSpec {
    pub fn layout(&self) -> &str {
        if self.layout.trim().is_empty() {
            if self.chart.is_some() {
                "chart"
            } else if !self.stats.is_empty() {
                "stats"
            } else if !self.cards.is_empty() {
                "cards"
            } else if !self.items.is_empty() {
                "timeline"
            } else if self.quote.is_some() {
                "quote"
            } else {
                "bullets"
            }
        } else {
            self.layout.trim()
        }
    }

    fn title_text(&self) -> &str {
        self.title.as_deref().unwrap_or("")
    }
}

/// Render one slide to an SVG document.
pub fn render_slide(slide: &SlideSpec, index: usize, theme: &Theme) -> String {
    match slide.layout() {
        "title" => title_layout(slide, index, theme),
        "section" => section_layout(slide, index, theme),
        "cards" => cards_layout(slide, index, theme),
        "stats" => stats_layout(slide, index, theme),
        "timeline" => timeline_layout(slide, index, theme),
        "quote" => quote_layout(slide, index, theme),
        "chart" => chart_layout(slide, index, theme),
        "closing" => closing_layout(slide, index, theme),
        _ => bullets_layout(slide, index, theme),
    }
}

// ── layouts ───────────────────────────────────────────────────────────────

fn title_layout(slide: &SlideSpec, index: usize, theme: &Theme) -> String {
    let mut svg = open(theme);
    if let Some(kicker) = slide.kicker.as_deref().or(slide.footer.as_deref()) {
        svg.push_str(&text(
            MARGIN,
            250.0,
            26.0,
            600,
            &theme.accent,
            kicker,
            "start",
            theme,
        ));
    }
    let title = slide.title_text();
    let (size, lines) = fit_block(title, 1500.0, 96.0, 46.0, 3);
    let start = 300.0;
    svg.push_str(&text_block(
        MARGIN,
        start,
        size,
        size * 0.98,
        700,
        &theme.text,
        &lines,
        "start",
        theme,
    ));
    svg.push_str(&rect(MARGIN, 760.0, 220.0, 8.0, &theme.accent));
    if let Some(subtitle) = slide.subtitle.as_deref() {
        let lines = wrap(subtitle, 64);
        svg.push_str(&text_block(
            MARGIN,
            830.0,
            34.0,
            46.0,
            400,
            &theme.muted,
            &lines.into_iter().take(3).collect::<Vec<_>>(),
            "start",
            theme,
        ));
    }
    svg.push_str(&page_number(index, slide, theme));
    close(svg)
}

fn section_layout(slide: &SlideSpec, index: usize, theme: &Theme) -> String {
    let mut svg = open(theme);
    let number = slide
        .number
        .clone()
        .unwrap_or_else(|| format!("{:02}", index + 1));
    svg.push_str(&text(
        MARGIN,
        430.0,
        220.0,
        800,
        &theme.accent,
        &number,
        "start",
        theme,
    ));
    let (size, lines) = fit_block(slide.title_text(), 1450.0, 82.0, 40.0, 2);
    svg.push_str(&text_block(
        MARGIN,
        540.0,
        size,
        size * 0.98,
        700,
        &theme.text,
        &lines,
        "start",
        theme,
    ));
    if let Some(subtitle) = slide.subtitle.as_deref() {
        let lines = wrap(subtitle, 64);
        svg.push_str(&text_block(
            MARGIN,
            780.0,
            32.0,
            44.0,
            400,
            &theme.muted,
            &lines.into_iter().take(3).collect::<Vec<_>>(),
            "start",
            theme,
        ));
    }
    svg.push_str(&page_number(index, slide, theme));
    close(svg)
}

fn bullets_layout(slide: &SlideSpec, index: usize, theme: &Theme) -> String {
    let mut svg = open(theme);
    svg.push_str(&heading(slide, theme));
    let bullets: Vec<&String> = slide.bullets.iter().take(8).collect();
    if bullets.is_empty() {
        if let Some(subtitle) = slide.subtitle.as_deref() {
            let lines = wrap(subtitle, 62);
            svg.push_str(&text_block(
                MARGIN,
                330.0,
                44.0,
                62.0,
                400,
                &theme.text,
                &lines,
                "start",
                theme,
            ));
        }
    } else {
        let gap = if bullets.len() > 5 { 112.0 } else { 138.0 };
        let size = if bullets.len() > 6 { 34.0 } else { 40.0 };
        let mut y = 320.0;
        for bullet in bullets {
            svg.push_str(&circle(MARGIN + 10.0, y - 12.0, 7.0, &theme.accent));
            let lines = wrap(bullet, 74);
            svg.push_str(&text_block(
                MARGIN + 48.0,
                y,
                size,
                size * 1.32,
                400,
                &theme.text,
                &lines.into_iter().take(2).collect::<Vec<_>>(),
                "start",
                theme,
            ));
            y += gap;
        }
    }
    svg.push_str(&page_number(index, slide, theme));
    close(svg)
}

fn cards_layout(slide: &SlideSpec, index: usize, theme: &Theme) -> String {
    let mut svg = open(theme);
    svg.push_str(&heading(slide, theme));
    let cards: Vec<&CardSpec> = slide.cards.iter().take(6).collect();
    let columns = if cards.len() <= 2 { 2 } else { 3 };
    let rows = ((cards.len().max(1) as f64) / columns as f64).ceil() as usize;
    let gap = 28.0;
    let width = (WIDTH as f64 - MARGIN * 2.0 - gap * (columns as f64 - 1.0)) / columns as f64;
    let height = (760.0 - gap * (rows.saturating_sub(1)) as f64) / rows.max(1) as f64;
    let top = 300.0;
    for (position, card) in cards.iter().enumerate() {
        let column = position % columns;
        let row = position / columns;
        let x = MARGIN + column as f64 * (width + gap);
        let y = top + row as f64 * (height + gap);
        svg.push_str(&round_rect(x, y, width, height, 24.0, &theme.surface));
        svg.push_str(&rect(x, y, 6.0, height, &theme.accent));
        let (title_size, title_lines) = fit_block(&card.title, width - 56.0, 34.0, 24.0, 2);
        svg.push_str(&text_block(
            x + 32.0,
            y + 54.0,
            title_size,
            title_size * 1.2,
            700,
            &theme.text,
            &title_lines,
            "start",
            theme,
        ));
        let body_lines = wrap(&card.body, ((width - 64.0) / 15.0).max(18.0) as usize);
        svg.push_str(&text_block(
            x + 32.0,
            y + 54.0 + title_size * 1.4 + 12.0,
            24.0,
            34.0,
            400,
            &theme.muted,
            &body_lines.into_iter().take(6).collect::<Vec<_>>(),
            "start",
            theme,
        ));
    }
    svg.push_str(&page_number(index, slide, theme));
    close(svg)
}

fn stats_layout(slide: &SlideSpec, index: usize, theme: &Theme) -> String {
    let mut svg = open(theme);
    svg.push_str(&heading(slide, theme));
    let stats: Vec<&StatSpec> = slide.stats.iter().take(4).collect();
    let count = stats.len().max(1);
    let width = (WIDTH as f64 - MARGIN * 2.0) / count as f64;
    for (position, stat) in stats.iter().enumerate() {
        let x = MARGIN + position as f64 * width;
        if position > 0 {
            svg.push_str(&rect(x - 1.0, 400.0, 2.0, 300.0, &theme.surface));
        }
        let (size, lines) = fit_block(&stat.value, width - 30.0, 112.0, 52.0, 1);
        svg.push_str(&text_block(
            x + 10.0,
            520.0,
            size,
            size,
            800,
            &theme.accent,
            &lines,
            "start",
            theme,
        ));
        let label = wrap(&stat.label, 26);
        svg.push_str(&text_block(
            x + 10.0,
            650.0,
            28.0,
            38.0,
            500,
            &theme.muted,
            &label.into_iter().take(3).collect::<Vec<_>>(),
            "start",
            theme,
        ));
    }
    if let Some(subtitle) = slide.subtitle.as_deref() {
        svg.push_str(&text(
            MARGIN,
            820.0,
            28.0,
            400,
            &theme.muted,
            subtitle,
            "start",
            theme,
        ));
    }
    svg.push_str(&page_number(index, slide, theme));
    close(svg)
}

fn timeline_layout(slide: &SlideSpec, index: usize, theme: &Theme) -> String {
    let mut svg = open(theme);
    svg.push_str(&heading(slide, theme));
    let items: Vec<&ItemSpec> = slide.items.iter().take(5).collect();
    let count = items.len().max(1);
    let usable = WIDTH as f64 - MARGIN * 2.0;
    let step = usable / count as f64;
    let line_y = 560.0;
    svg.push_str(&rect(MARGIN, line_y - 2.0, usable, 4.0, &theme.surface));
    for (position, item) in items.iter().enumerate() {
        let x = MARGIN + step * position as f64 + step * 0.5;
        svg.push_str(&circle(x, line_y, 14.0, &theme.accent));
        svg.push_str(&circle(x, line_y, 6.0, &theme.background));
        let when = wrap(&item.when, 18);
        svg.push_str(&text_block(
            x - step * 0.44,
            line_y - 60.0,
            30.0,
            38.0,
            700,
            &theme.accent2,
            &when.into_iter().take(2).collect::<Vec<_>>(),
            "start",
            theme,
        ));
        let what = wrap(&item.what, 24);
        svg.push_str(&text_block(
            x - step * 0.44,
            line_y + 78.0,
            28.0,
            38.0,
            400,
            &theme.text,
            &what.into_iter().take(4).collect::<Vec<_>>(),
            "start",
            theme,
        ));
    }
    svg.push_str(&page_number(index, slide, theme));
    close(svg)
}

fn quote_layout(slide: &SlideSpec, index: usize, theme: &Theme) -> String {
    let mut svg = open(theme);
    svg.push_str(&text(
        MARGIN,
        290.0,
        220.0,
        800,
        &theme.accent,
        "“",
        "start",
        theme,
    ));
    let quote = slide
        .quote
        .as_deref()
        .or(slide.title.as_deref())
        .unwrap_or("");
    let lines = wrap(quote, 46);
    svg.push_str(&text_block(
        MARGIN,
        400.0,
        58.0,
        80.0,
        500,
        &theme.text,
        &lines.into_iter().take(6).collect::<Vec<_>>(),
        "start",
        theme,
    ));
    if let Some(attribution) = slide.attribution.as_deref().or(slide.subtitle.as_deref()) {
        svg.push_str(&rect(MARGIN, 900.0, 90.0, 6.0, &theme.accent));
        svg.push_str(&text(
            MARGIN + 120.0,
            920.0,
            30.0,
            500,
            &theme.muted,
            attribution,
            "start",
            theme,
        ));
    }
    svg.push_str(&page_number(index, slide, theme));
    close(svg)
}

fn chart_layout(slide: &SlideSpec, index: usize, theme: &Theme) -> String {
    let mut svg = open(theme);
    svg.push_str(&heading(slide, theme));
    if let Some(chart) = &slide.chart {
        svg.push_str(&chart_svg(
            chart,
            MARGIN,
            290.0,
            WIDTH as f64 - MARGIN * 2.0,
            660.0,
            theme,
        ));
    } else {
        svg.push_str(&text(
            MARGIN,
            340.0,
            30.0,
            400,
            &theme.muted,
            "No chart data was supplied.",
            "start",
            theme,
        ));
    }
    svg.push_str(&page_number(index, slide, theme));
    close(svg)
}

fn closing_layout(slide: &SlideSpec, index: usize, theme: &Theme) -> String {
    let mut svg = open(theme);
    let title = if slide.title_text().is_empty() {
        "Thank you"
    } else {
        slide.title_text()
    };
    let (size, lines) = fit_block(title, 1500.0, 96.0, 46.0, 3);
    svg.push_str(&text_block(
        MARGIN,
        420.0,
        size,
        size * 0.98,
        700,
        &theme.text,
        &lines,
        "start",
        theme,
    ));
    svg.push_str(&rect(MARGIN, 720.0, 220.0, 8.0, &theme.accent));
    if let Some(subtitle) = slide.subtitle.as_deref() {
        let lines = wrap(subtitle, 64);
        svg.push_str(&text_block(
            MARGIN,
            800.0,
            32.0,
            44.0,
            400,
            &theme.muted,
            &lines.into_iter().take(3).collect::<Vec<_>>(),
            "start",
            theme,
        ));
    }
    svg.push_str(&page_number(index, slide, theme));
    close(svg)
}

// ── SVG primitives ────────────────────────────────────────────────────────

fn open(theme: &Theme) -> String {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{WIDTH}\" height=\"{HEIGHT}\" viewBox=\"0 0 {WIDTH} {HEIGHT}\">\
         <rect width=\"{WIDTH}\" height=\"{HEIGHT}\" fill=\"{}\"/>",
        escape_attr(&theme.background)
    )
}

fn close(mut svg: String) -> String {
    svg.push_str("</svg>");
    svg
}

fn heading(slide: &SlideSpec, theme: &Theme) -> String {
    let mut out = String::new();
    if let Some(kicker) = slide.kicker.as_deref() {
        out.push_str(&text(
            MARGIN,
            120.0,
            24.0,
            600,
            &theme.accent,
            kicker,
            "start",
            theme,
        ));
    }
    let (size, lines) = fit_block(slide.title_text(), 1650.0, 64.0, 40.0, 2);
    out.push_str(&text_block(
        MARGIN,
        185.0,
        size,
        size * 1.16,
        700,
        &theme.text,
        &lines,
        "start",
        theme,
    ));
    if let Some(subtitle) = slide.subtitle.as_deref() {
        out.push_str(&text(
            MARGIN,
            270.0,
            26.0,
            400,
            &theme.muted,
            subtitle,
            "start",
            theme,
        ));
    }
    out
}

fn page_number(index: usize, slide: &SlideSpec, theme: &Theme) -> String {
    let label = slide
        .footer
        .clone()
        .unwrap_or_else(|| format!("{:02}", index + 1));
    text(
        WIDTH as f64 - MARGIN,
        HEIGHT as f64 - 58.0,
        22.0,
        500,
        &theme.muted,
        &label,
        "end",
        theme,
    )
}

#[allow(clippy::too_many_arguments)] // mirrors SVG text attributes
pub fn text(
    x: f64,
    y: f64,
    size: f64,
    weight: u32,
    fill: &str,
    content: &str,
    anchor: &str,
    theme: &Theme,
) -> String {
    format!(
        "<text x=\"{x:.1}\" y=\"{y:.1}\" font-family=\"{}\" font-size=\"{size:.1}\" font-weight=\"{weight}\" fill=\"{}\" text-anchor=\"{anchor}\">{}</text>",
        escape_attr(&theme.font),
        escape_attr(fill),
        escape_xml(content)
    )
}

#[allow(clippy::too_many_arguments)] // mirrors a text block plus its style
pub fn text_block(
    x: f64,
    y: f64,
    size: f64,
    line_height: f64,
    weight: u32,
    fill: &str,
    lines: &[String],
    anchor: &str,
    theme: &Theme,
) -> String {
    let mut out = String::new();
    for (index, line) in lines.iter().enumerate() {
        out.push_str(&text(
            x,
            y + index as f64 * line_height,
            size,
            weight,
            fill,
            line,
            anchor,
            theme,
        ));
    }
    out
}

pub fn rect(x: f64, y: f64, width: f64, height: f64, fill: &str) -> String {
    format!(
        "<rect x=\"{x:.1}\" y=\"{y:.1}\" width=\"{width:.1}\" height=\"{height:.1}\" fill=\"{}\"/>",
        escape_attr(fill)
    )
}

pub fn round_rect(x: f64, y: f64, width: f64, height: f64, radius: f64, fill: &str) -> String {
    format!(
        "<rect x=\"{x:.1}\" y=\"{y:.1}\" width=\"{width:.1}\" height=\"{height:.1}\" rx=\"{radius:.1}\" fill=\"{}\"/>",
        escape_attr(fill)
    )
}

pub fn circle(x: f64, y: f64, radius: f64, fill: &str) -> String {
    format!(
        "<circle cx=\"{x:.1}\" cy=\"{y:.1}\" r=\"{radius:.1}\" fill=\"{}\"/>",
        escape_attr(fill)
    )
}

pub fn escape_xml(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if c.is_control() && c != '\n' && c != '\t' => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

fn escape_attr(value: &str) -> String {
    escape_xml(value)
}

/// Only a full six-digit hex colour is interpolated. `#abc` is expanded;
/// anything else (named colours, `url(...)`, quotes, semicolons) falls back.
fn safe_color(raw: &str) -> Option<String> {
    let value = raw.trim();
    if let Some(hex) = value.strip_prefix('#') {
        if hex.len() == 3 && hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
            let expanded: String = hex.chars().flat_map(|ch| [ch, ch]).collect();
            return Some(format!("#{expanded}"));
        }
        if hex.len() == 6 && hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
            return Some(format!("#{hex}"));
        }
    }
    None
}

fn safe_font(raw: &str) -> String {
    // A font-family is an attribute value. Reject anything that could close it
    // or start a second attribute; a friendly name never needs those.
    if raw.chars().any(|ch| {
        matches!(
            ch,
            '"' | '\'' | '<' | '>' | '(' | ')' | '=' | ';' | '{' | '}' | '\\'
        )
    }) {
        return Theme::default().font;
    }
    let cleaned: String = raw
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, ' ' | ',' | '-' | '_'))
        .take(120)
        .collect();
    if cleaned.trim().is_empty() {
        Theme::default().font
    } else {
        cleaned
    }
}

// ── text fitting ──────────────────────────────────────────────────────────

/// Wrap on word boundaries at approximately `max_chars` characters.
pub fn wrap(text: &str, max_chars: usize) -> Vec<String> {
    let max_chars = max_chars.max(8);
    let mut lines = Vec::new();
    for paragraph in text.replace("\r\n", "\n").split('\n') {
        let mut current = String::new();
        for word in paragraph.split_whitespace() {
            if current.is_empty() {
                current.push_str(word);
            } else if current.chars().count() + 1 + word.chars().count() <= max_chars {
                current.push(' ');
                current.push_str(word);
            } else {
                lines.push(std::mem::take(&mut current));
                current.push_str(word);
            }
        }
        if !current.is_empty() {
            lines.push(current);
        }
        if paragraph.trim().is_empty() {
            lines.push(String::new());
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Shrink a heading until it fits the width and line budget.
fn fit_block(
    text: &str,
    width: f64,
    size: f64,
    min_size: f64,
    max_lines: usize,
) -> (f64, Vec<String>) {
    let mut candidate = size;
    loop {
        let chars = ((width / (candidate * 0.54)).floor() as usize).max(8);
        let lines = wrap(text, chars);
        if lines.len() <= max_lines || candidate <= min_size {
            return (candidate, lines.into_iter().take(max_lines).collect());
        }
        candidate = (candidate - 4.0).max(min_size);
    }
}

// ── charts ────────────────────────────────────────────────────────────────

fn number(value: &Value) -> f64 {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|text| text.parse::<f64>().ok()))
        .filter(|value| value.is_finite())
        .unwrap_or(0.0)
}

fn series(chart: &Value) -> Vec<(String, Vec<f64>)> {
    let mut out = Vec::new();
    if let Some(list) = chart.get("series").and_then(Value::as_array) {
        for entry in list {
            let name = entry
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let values: Vec<f64> = entry
                .get("values")
                .or_else(|| entry.get("data"))
                .and_then(Value::as_array)
                .map(|values| values.iter().map(number).collect())
                .unwrap_or_default();
            if !values.is_empty() {
                out.push((name, values));
            }
        }
    }
    if out.is_empty() {
        let values: Vec<f64> = chart
            .get("values")
            .or_else(|| chart.get("data"))
            .and_then(Value::as_array)
            .map(|values| values.iter().map(number).collect())
            .unwrap_or_default();
        if !values.is_empty() {
            out.push((String::new(), values));
        }
    }
    out
}

fn labels(chart: &Value, count: usize) -> Vec<String> {
    chart
        .get("labels")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| value.to_string())
                })
                .collect()
        })
        .unwrap_or_else(|| (1..=count).map(|index| index.to_string()).collect())
}

pub fn chart_svg(chart: &Value, x: f64, y: f64, width: f64, height: f64, theme: &Theme) -> String {
    let kind = chart
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("bar")
        .to_ascii_lowercase();
    let series = series(chart);
    if series.is_empty() {
        return text(
            x,
            y + 40.0,
            28.0,
            400,
            &theme.muted,
            "The chart has no values.",
            "start",
            theme,
        );
    }
    let count = series
        .iter()
        .map(|(_, values)| values.len())
        .max()
        .unwrap_or(0);
    let labels = labels(chart, count);
    let palette = [&theme.accent, &theme.accent2, &theme.muted, &theme.text];

    let mut svg = String::new();
    if matches!(kind.as_str(), "pie" | "donut") {
        let values = &series[0].1;
        let total: f64 = values.iter().map(|value| value.abs()).sum();
        let cx = x + width * 0.42;
        let cy = y + height * 0.5;
        let radius = height.min(width) * 0.36;
        if total <= 0.0 {
            return text(
                x,
                y + 40.0,
                28.0,
                400,
                &theme.muted,
                "The chart values sum to zero.",
                "start",
                theme,
            );
        }
        let mut angle = -std::f64::consts::FRAC_PI_2;
        for (index, value) in values.iter().enumerate() {
            let sweep = (value.abs() / total) * std::f64::consts::TAU;
            let end = angle + sweep;
            let large = if sweep > std::f64::consts::PI { 1 } else { 0 };
            let x1 = cx + radius * angle.cos();
            let y1 = cy + radius * angle.sin();
            let x2 = cx + radius * end.cos();
            let y2 = cy + radius * end.sin();
            let color = palette[index % palette.len()];
            if sweep >= std::f64::consts::TAU - 0.0001 {
                svg.push_str(&circle(cx, cy, radius, color));
            } else {
                svg.push_str(&format!(
                    "<path d=\"M {cx:.1} {cy:.1} L {x1:.1} {y1:.1} A {radius:.1} {radius:.1} 0 {large} 1 {x2:.1} {y2:.1} Z\" fill=\"{}\"/>",
                    escape_attr(color)
                ));
            }
            angle = end;
        }
        if kind == "donut" {
            svg.push_str(&circle(cx, cy, radius * 0.56, &theme.background));
        }
        let mut legend_y = y + 80.0;
        for (index, label) in labels.iter().enumerate().take(values.len()) {
            svg.push_str(&circle(
                cx + radius + 80.0,
                legend_y - 8.0,
                8.0,
                palette[index % palette.len()],
            ));
            svg.push_str(&text(
                cx + radius + 100.0,
                legend_y,
                24.0,
                400,
                &theme.muted,
                label,
                "start",
                theme,
            ));
            legend_y += 42.0;
        }
        return svg;
    }

    let plot_x = x + 70.0;
    let plot_y = y + 40.0;
    let plot_w = width - 100.0;
    let plot_h = height - 120.0;
    let max_value = series
        .iter()
        .flat_map(|(_, values)| values.iter().copied())
        .fold(0.0f64, f64::max)
        .max(1.0);
    let min_value = series
        .iter()
        .flat_map(|(_, values)| values.iter().copied())
        .fold(0.0f64, f64::min)
        .min(0.0);
    let span = (max_value - min_value).max(1.0);
    let baseline = plot_y + plot_h * (max_value / span);

    svg.push_str(&rect(plot_x, baseline, plot_w, 1.5, &theme.surface));
    svg.push_str(&rect(plot_x, plot_y, 1.5, plot_h, &theme.surface));

    match kind.as_str() {
        "line" | "area" | "scatter" => {
            for (series_index, (name, values)) in series.iter().enumerate() {
                let color = palette[series_index % palette.len()];
                let points: Vec<(f64, f64)> = values
                    .iter()
                    .enumerate()
                    .map(|(index, value)| {
                        let px = plot_x
                            + if count <= 1 {
                                plot_w / 2.0
                            } else {
                                plot_w * index as f64 / (count - 1) as f64
                            };
                        let py = plot_y + plot_h * ((max_value - value) / span);
                        (px, py)
                    })
                    .collect();
                if kind == "area" && !points.is_empty() {
                    let mut path = format!("M {:.1} {:.1}", points[0].0, baseline);
                    for (px, py) in &points {
                        path.push_str(&format!(" L {px:.1} {py:.1}"));
                    }
                    path.push_str(&format!(
                        " L {:.1} {:.1} Z",
                        points.last().unwrap().0,
                        baseline
                    ));
                    svg.push_str(&format!(
                        "<path d=\"{path}\" fill=\"{}\" opacity=\"0.20\"/>",
                        escape_attr(color)
                    ));
                }
                if kind == "scatter" {
                    for (px, py) in &points {
                        svg.push_str(&circle(*px, *py, 7.0, color));
                    }
                } else {
                    let path = points
                        .iter()
                        .enumerate()
                        .map(|(index, (px, py))| {
                            if index == 0 {
                                format!("M {px:.1} {py:.1}")
                            } else {
                                format!(" L {px:.1} {py:.1}")
                            }
                        })
                        .collect::<String>();
                    svg.push_str(&format!(
                        "<path d=\"{path}\" fill=\"none\" stroke=\"{}\" stroke-width=\"5\" stroke-linejoin=\"round\" stroke-linecap=\"round\"/>",
                        escape_attr(color)
                    ));
                }
                if !name.is_empty() {
                    svg.push_str(&text(
                        plot_x + plot_w - 10.0,
                        plot_y + 22.0 + series_index as f64 * 34.0,
                        22.0,
                        600,
                        color,
                        name,
                        "end",
                        theme,
                    ));
                }
            }
        }
        _ => {
            // Grouped bars.
            let groups = count.max(1);
            let group_w = plot_w / groups as f64;
            let bar_w = (group_w * 0.72 / series.len().max(1) as f64).max(4.0);
            for (series_index, (_, values)) in series.iter().enumerate() {
                let color = palette[series_index % palette.len()];
                for (index, value) in values.iter().enumerate() {
                    let zero = plot_y + plot_h * (max_value / span);
                    let value_y = plot_y + plot_h * ((max_value - value) / span);
                    let top = value_y.min(zero);
                    let bar_height = (value_y - zero).abs().max(1.0);
                    let gx = plot_x + group_w * index as f64 + group_w * 0.14;
                    let bx = gx + series_index as f64 * bar_w;
                    svg.push_str(&round_rect(bx, top, bar_w * 0.9, bar_height, 4.0, color));
                }
            }
        }
    }

    // x labels — every nth label so they never overlap.
    let step = (count as f64 / 12.0).ceil().max(1.0) as usize;
    for (index, label) in labels.iter().enumerate().take(count) {
        if index % step != 0 {
            continue;
        }
        let px = plot_x
            + if count <= 1 {
                plot_w / 2.0
            } else {
                plot_w * index as f64 / (count - 1) as f64
            };
        svg.push_str(&text(
            px,
            plot_y + plot_h + 44.0,
            22.0,
            400,
            &theme.muted,
            label,
            "middle",
            theme,
        ));
    }
    svg
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(layout: &str) -> SlideSpec {
        SlideSpec {
            layout: layout.to_string(),
            kicker: Some("Kicker".to_string()),
            title: Some("A title that is long enough to wrap across two lines".to_string()),
            subtitle: Some("A subtitle".to_string()),
            bullets: vec!["First point".to_string(), "Second point".to_string()],
            cards: vec![
                CardSpec {
                    title: "One".into(),
                    body: "Body one".into(),
                },
                CardSpec {
                    title: "Two".into(),
                    body: "Body two".into(),
                },
            ],
            stats: vec![
                StatSpec {
                    value: "42%".into(),
                    label: "Conversion".into(),
                },
                StatSpec {
                    value: "1.2k".into(),
                    label: "Users".into(),
                },
            ],
            items: vec![ItemSpec {
                when: "Q1".into(),
                what: "Started".into(),
            }],
            quote: Some("A quotation".to_string()),
            attribution: Some("Someone".to_string()),
            chart: Some(serde_json::json!({
                "type": "bar",
                "labels": ["A", "B", "C"],
                "series": [{"name": "One", "values": [4, 8, 6]}]
            })),
            footer: None,
            number: Some("01".to_string()),
        }
    }

    #[test]
    fn every_layout_renders_a_full_svg() {
        for layout in [
            "title", "section", "bullets", "cards", "stats", "timeline", "quote", "chart",
            "closing",
        ] {
            let svg = render_slide(&sample(layout), 0, &Theme::default());
            assert!(svg.starts_with("<svg"), "{layout}");
            assert!(svg.ends_with("</svg>"), "{layout}");
            assert!(svg.contains("width=\"1920\""), "{layout}");
            assert!(svg.contains("height=\"1080\""), "{layout}");
        }
    }

    #[test]
    fn text_and_attributes_are_escaped() {
        let mut slide = sample("title");
        slide.title = Some("<script>alert('x')</script> & more".to_string());
        let svg = render_slide(&slide, 0, &Theme::default());
        assert!(!svg.contains("<script>"), "{svg}");
        assert!(svg.contains("&lt;script&gt;"), "{svg}");
        assert!(svg.contains("&amp; more"), "{svg}");
    }

    #[test]
    fn a_hostile_theme_colour_cannot_inject_markup() {
        let theme = Theme::from_value(Some(&serde_json::json!({
            "background": "\" onload=\"alert(1)",
            "accent": "#79b8ff",
            "font": "Inter\" onload=\"x"
        })));
        assert_eq!(theme.background, Theme::default().background);
        assert_eq!(theme.accent, "#79b8ff");
        assert!(!theme.font.contains('"'));
        let svg = render_slide(&sample("title"), 0, &theme);
        assert!(!svg.contains("onload"), "{svg}");
    }

    #[test]
    fn a_malformed_chart_does_not_take_down_the_slide() {
        for chart in [
            serde_json::json!({"type": "bar"}),
            serde_json::json!({"type": "pie", "values": []}),
            serde_json::json!({"type": "line", "values": ["nope", null]}),
        ] {
            let mut slide = sample("chart");
            slide.chart = Some(chart);
            let svg = render_slide(&slide, 0, &Theme::default());
            assert!(svg.starts_with("<svg"));
        }
    }

    #[test]
    fn wrapping_never_loses_words() {
        let text = "one two three four five six seven eight nine ten";
        let lines = wrap(text, 18);
        assert_eq!(lines.join(" "), text);
    }
}

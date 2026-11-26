# Localization

The desktop UI ships in twelve languages. The dictionary is the source of
truth; components call `t("section.key")` and never carry literal user-facing
English.

| Code | Language | Native name | Direction |
|---|---|---|---|
| `en` | English | English | LTR |
| `es` | Spanish | Español | LTR |
| `fr` | French | Français | LTR |
| `de` | German | Deutsch | LTR |
| `pt` | Portuguese (Brazil) | Português | LTR |
| `it` | Italian | Italiano | LTR |
| `ru` | Russian | Русский | LTR |
| `hi` | Hindi | हिन्दी | LTR |
| `tr` | Turkish | Türkçe | LTR |
| `ja` | Japanese | 日本語 | LTR |
| `ko` | Korean | 한국어 | LTR |
| `zh` | Chinese (Simplified) | 中文 | LTR |

Arabic and Hebrew are deliberately **not** listed. They need right-to-left
layout, mirrored icons and a bidi audit; adding the strings alone would be a
false claim.

## Provenance

The six original locales (`en`, `es`, `fr`, `de`, `ja`, `zh`) were already
complete and parity-tested before this release and were not changed by this
pass. The six added (`pt`, `it`, `ru`, `hi`, `tr`, `ko`) started from a
machine-assisted draft and then received a language-model review pass in the
target language. They are complete and parity-tested, but
they are **not professional human translations**. Corrections from native
speakers are welcome; a correction only needs to change the value in the locale
file and pass the test suite.

## Contract

`src/lib/i18n.test.ts` walks every English key and fails when:

1. a key is missing from any locale or is empty;
2. a locale has a key English does not have;
3. a value contains a single-brace placeholder `t()` never fills;
4. a `{{placeholder}}` name differs from English between locales;
5. a call site does not pass a placeholder its English string asks for.

That means a new string must be added to **all twelve** locale files, even when
the value is initially the English text. `t()` falls back to English at runtime,
but the test does not accept fallback as translation.

## Rules

- Keep `{{name}}` placeholders exactly; they are filled by `t(key, params)`.
- Keep brand and protocol names unchanged: RAVENBOT, OpenAI, Anthropic,
  OpenRouter, Ollama, Slack, Discord, Telegram, WhatsApp, Microsoft Teams,
  GitHub, Docker, Linux, macOS, Windows, MCP, API, JSON, URL, HTTP, SQLite,
  Markdown, CSV, PDF, npm, cargo.
- Keep inline code, URLs, file paths, shell commands and keyboard shortcuts
  unchanged.
- Buttons stay short and use sentence case. Do not add exclamation marks that
  the English string does not have.
- Use the same register throughout a locale: Brazilian Portuguese uses
  `você`, Italian `tu`, Russian `вы`, Hindi `आप`, Turkish `sen`, Korean
  `해요체`.
- Translate meaning, not words. A "run" is an execution, not a race; a "job" is
  an automation recipe, not employment; a "claim" is task ownership, not a legal
  claim; "clear" resets a field, not a light colour.

## Adding or updating a locale

1. Copy `src/lib/i18n/locales/en.json` to `<code>.json` and translate every
   value. `scripts/translate-locale.py <code>` can produce a first draft with
   placeholders, code spans, URLs, environment variables and product names
   protected. Review the draft; do not ship it unreviewed.
2. Add the code to the `Locale` union in `state.svelte.ts`.
3. Import the dictionary and add it to `locales` and `getAvailableLocales()` in
   `index.ts`.
4. Import it in `i18n.test.ts`, add it to `dictionaries`, and update the
   expected locale count.
5. Run:
   ```sh
   npx vitest run src/lib/i18n.test.ts
   npm run check
   ```
6. For an RTL language, do not stop at the dictionary: audit the layout,
   logical properties, icons and focus order first.

The Settings language picker is generated from `getAvailableLocales()`, so no
separate UI list needs updating. `initI18n()` also maps a browser tag such as
`pt-BR` to `pt`.

## Tooling

Three development scripts support a locale pass. They are not runtime
dependencies and are not part of the app build.

- `scripts/translate-locale.py <code>` produces a machine-assisted draft. It
  needs Python `requests` and network access, and protects placeholders, code
  spans, HTML, URLs, environment variables and product names with tokens. The
  output must be reviewed before shipping.
- `scripts/merge-locale.py <code> <fragment.json>` merges a translated
  fragment into an existing dictionary. It rejects unknown keys, empty values
  and structural changes, so a long pass cannot silently drop a key.
- `scripts/check-locale.py <code>` reports key/order gaps, placeholder drift,
  leftover tokens and values identical to English. The Vitest suite remains the
  blocking gate; this is the faster review loop.

## RTL and the remote page

The remote phone page is a small standalone client and remains English; it was
never part of the desktop dictionary. Localizing it needs a separate, reviewed
string table and the same CSP-safe rendering rules.

// Reactive locale atom. Lives in a .svelte.ts module so `t()` calls made
// inside templates re-render when the locale changes (P7 i18n coverage).
export type Locale =
  | "en"
  | "es"
  | "fr"
  | "de"
  | "pt"
  | "it"
  | "ru"
  | "hi"
  | "tr"
  | "ja"
  | "ko"
  | "zh";

class LocaleStore {
  current = $state<Locale>("en");
}

export const localeState = new LocaleStore();

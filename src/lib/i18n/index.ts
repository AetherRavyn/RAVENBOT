/**
 * RAVENBOT Internationalization System
 * 
 * Provides type-safe translations with fallback support.
 * Strings are externalized from day one for easy localization.
 */

// Translation dictionaries (all 6 locales fully translated)
import en from './locales/en.json';
import es from './locales/es.json';
import fr from './locales/fr.json';
import de from './locales/de.json';
import ja from './locales/ja.json';
import zh from './locales/zh.json';

// Type definitions for translation keys
type NestedKeyOf<T> = T extends object
  ? { [K in keyof T & string]: K | `${K}.${NestedKeyOf<T[K]>}` }[keyof T & string]
  : never;

export type TranslationKey = NestedKeyOf<typeof en>;

// Supported locales
export type Locale = 'en' | 'es' | 'fr' | 'de' | 'ja' | 'zh';

// Translation dictionaries
const locales: Record<Locale, Record<string, any>> = {
  en,
  es,
  fr,
  de,
  ja,
  zh,
};

// Current locale state
let currentLocale: Locale = 'en';

/**
 * Get a nested value from an object using dot notation
 */
function getNestedValue(obj: any, path: string): string | undefined {
  return path.split('.').reduce((current, key) => current?.[key], obj);
}

/**
 * Set the current locale
 */
export function setLocale(locale: Locale): void {
  currentLocale = locale;

  // Safe in non-DOM contexts (tests, workers)
  if (typeof document !== "undefined") {
    document.documentElement.lang = locale;
  }
  
  // Store preference
  if (typeof localStorage !== 'undefined') {
    localStorage.setItem('ravenbot-locale', locale);
  }
}

/**
 * Get the current locale
 */
export function getLocale(): Locale {
  return currentLocale;
}

/**
 * Get a translation by key
 */
export function t(key: TranslationKey, params?: Record<string, string | number>): string {
  // Try current locale first, fallback to English
  let translation = getNestedValue(locales[currentLocale], key)
    ?? getNestedValue(locales.en, key)
    ?? key;

  // Replace parameters
  if (params) {
    Object.entries(params).forEach(([paramKey, value]) => {
      translation = translation.replace(`{{${paramKey}}}`, String(value));
    });
  }

  return translation;
}

/**
 * Initialize i18n from stored preferences or browser language
 */
export function initI18n(): void {
  // Check localStorage first
  if (typeof localStorage !== 'undefined') {
    const stored = localStorage.getItem('ravenbot-locale') as Locale | null;
    if (stored && locales[stored]) {
      setLocale(stored);
      return;
    }
  }

  // Detect browser language
  if (typeof navigator !== 'undefined') {
    const browserLang = navigator.language.split('-')[0] as Locale;
    if (locales[browserLang]) {
      setLocale(browserLang);
      return;
    }
  }

  // Default to English
  setLocale('en');
}

/**
 * Get available locales with their display names
 */
export function getAvailableLocales(): { code: Locale; name: string; nativeName: string }[] {
  return [
    { code: 'en', name: 'English', nativeName: 'English' },
    { code: 'es', name: 'Spanish', nativeName: 'Español' },
    { code: 'fr', name: 'French', nativeName: 'Français' },
    { code: 'de', name: 'German', nativeName: 'Deutsch' },
    { code: 'ja', name: 'Japanese', nativeName: '日本語' },
    { code: 'zh', name: 'Chinese', nativeName: '中文' },
  ];
}

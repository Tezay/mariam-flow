import { en } from './en';
import { fr } from './fr';

/** Languages the dashboard ships with. English is the reference. */
export const LOCALES = ['en', 'fr'] as const;

export type Locale = (typeof LOCALES)[number];

/** Every message key, derived from the reference dictionary. */
export type MessageKey = keyof typeof en;

/** The shape every translation must have, key for key. */
export type Messages = Record<MessageKey, string>;

const DICTIONARIES: Record<Locale, Messages> = { en, fr };

/** Values substituted into `{placeholders}` of a message. */
export type Params = Record<string, string | number>;

/**
 * Resolves a message in `locale`, substituting any `{placeholder}`.
 *
 * A key missing from a translation falls back to English rather than to the
 * raw key: an untranslated sentence is inconvenient, an identifier on screen
 * is a bug the installer has to interpret.
 */
export function translate(locale: Locale, key: MessageKey, params?: Params): string {
  const template = DICTIONARIES[locale][key] ?? en[key];
  if (!params) {
    return template;
  }
  return template.replace(/\{(\w+)\}/g, (whole, name: string) =>
    name in params ? String(params[name]) : whole,
  );
}

/**
 * Picks the best available locale from the browser's ordered preferences.
 *
 * Only the primary subtag is compared, so `fr-CA` and `fr-BE` both land on
 * French. Anything unrecognised leaves the reference locale in place.
 */
export function detectLocale(preferences: readonly string[]): Locale {
  for (const preference of preferences) {
    const primary = preference.toLowerCase().split('-')[0];
    const match = LOCALES.find((locale) => locale === primary);
    if (match) {
      return match;
    }
  }
  return 'en';
}

/** The dictionary of a locale, for tests and tooling. */
export function dictionary(locale: Locale): Messages {
  return DICTIONARIES[locale];
}

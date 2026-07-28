import { describe, expect, it } from 'vitest';

import { en } from './en';
import { LOCALES, detectLocale, dictionary, translate, type Locale } from './messages';

describe('dictionaries', () => {
  const reference = Object.keys(en).sort();

  // The compiler already refuses a translation that is missing a key. This
  // catches the other direction: a key removed from the reference and left
  // behind in a translation, which no type can see.
  it.each(LOCALES)('%s has exactly the reference keys', (locale: Locale) => {
    expect(Object.keys(dictionary(locale)).sort()).toEqual(reference);
  });

  it.each(LOCALES)('%s leaves no message empty', (locale: Locale) => {
    for (const [key, value] of Object.entries(dictionary(locale))) {
      expect(value.trim(), `${locale}.${key}`).not.toBe('');
    }
  });

  it.each(LOCALES)('%s keeps the placeholders of the reference', (locale: Locale) => {
    const placeholders = (text: string) => (text.match(/\{(\w+)\}/g) ?? []).sort();
    for (const key of Object.keys(en) as (keyof typeof en)[]) {
      expect(placeholders(dictionary(locale)[key]), `${locale}.${key}`).toEqual(
        placeholders(en[key]),
      );
    }
  });
});

describe('translate', () => {
  it('returns the message of the requested locale', () => {
    expect(translate('en', 'login.submit')).toBe('Sign in');
    expect(translate('fr', 'login.submit')).toBe('Se connecter');
  });

  it('substitutes placeholders', () => {
    expect(translate('en', 'login.throttled', { seconds: 4 })).toBe(
      'Too many attempts. Try again in 4 s.',
    );
    expect(translate('en', 'wizard.progress', { current: 2, total: 4 })).toBe('Step 2 of 4');
  });

  it('leaves an unknown placeholder alone rather than printing undefined', () => {
    expect(translate('en', 'login.throttled', { other: 1 })).toContain('{seconds}');
  });
});

describe('detectLocale', () => {
  it('matches on the primary subtag', () => {
    expect(detectLocale(['fr-CA', 'en'])).toBe('fr');
    expect(detectLocale(['en-GB'])).toBe('en');
  });

  it('honours the order of preference', () => {
    expect(detectLocale(['de', 'fr', 'en'])).toBe('fr');
  });

  it('falls back to the reference locale', () => {
    expect(detectLocale(['de', 'es'])).toBe('en');
    expect(detectLocale([])).toBe('en');
  });
});

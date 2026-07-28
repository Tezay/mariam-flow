import { detectLocale, translate, type Locale, type MessageKey, type Params } from './messages';

const STORAGE_KEY = 'mariam-flow.locale';

let current = $state<Locale>('en');

/** The locale currently in force. */
export function locale(): Locale {
  return current;
}

/** Switches locale and remembers the choice on this device. */
export function setLocale(next: Locale): void {
  current = next;
  try {
    localStorage.setItem(STORAGE_KEY, next);
  } catch {
    // Private browsing, or storage disabled: the choice simply does not
    // outlive the session, which is not worth failing a render over.
  }
}

/** Flips between the two shipped locales. */
export function toggleLocale(): void {
  setLocale(current === 'en' ? 'fr' : 'en');
}

/**
 * Chooses the starting locale: an explicit earlier choice wins, otherwise
 * the browser's preferences decide.
 */
export function initLocale(): void {
  let stored: string | null = null;
  try {
    stored = localStorage.getItem(STORAGE_KEY);
  } catch {
    stored = null;
  }
  current = stored === 'en' || stored === 'fr' ? stored : detectLocale(navigator.languages ?? []);
  document.documentElement.lang = current;
}

/** Resolves a message in the current locale. */
export function t(key: MessageKey, params?: Params): string {
  return translate(current, key, params);
}

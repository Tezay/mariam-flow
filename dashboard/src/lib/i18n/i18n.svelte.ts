import { detectLocale, translate, type Locale, type MessageKey, type Params } from './messages';

const STORAGE_KEY = 'mariam-flow.locale';
const CLOCK_KEY = 'mariam-flow.hour12';

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

  // The clock cycle is a separate preference from the language: a French
  // operator may still want a 12-hour clock, and an English one a 24-hour
  // clock. Default to 24 hours, which is what an operations screen wants.
  let clock: string | null = null;
  try {
    clock = localStorage.getItem(CLOCK_KEY);
  } catch {
    clock = null;
  }
  twelveHour = clock === '12';
}

let twelveHour = $state(false);

/** Whether times are shown on a 12-hour clock. */
export function hour12(): boolean {
  return twelveHour;
}

/** Flips between a 24-hour and a 12-hour clock, and remembers the choice. */
export function toggleHourCycle(): void {
  twelveHour = !twelveHour;
  try {
    localStorage.setItem(CLOCK_KEY, twelveHour ? '12' : '24');
  } catch {
    // Storage disabled: the choice simply does not outlive the session.
  }
}

/**
 * The BCP-47 tag to format dates and numbers with.
 *
 * Derived from the interface language rather than the browser's, so a
 * French dashboard never prints `11:41 PM`. Both tags render a 24-hour
 * clock, which is what an operations screen wants.
 */
export function formattingLocale(): string {
  return current === 'fr' ? 'fr-FR' : 'en-GB';
}

/** Resolves a message in the current locale. */
export function t(key: MessageKey, params?: Params): string {
  return translate(current, key, params);
}

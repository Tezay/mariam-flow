/**
 * The calibration screen's pure logic.
 *
 * A label marks a *change of state*, not an occurrence: training reads the
 * latest label at or before each analysis window, so one press stands until
 * the next. Everything here follows from that.
 */

import { DENSITY_CLASSES, type ClassMapping, type DensityClass } from './api';

/** The class meanings a site starts from, before anyone describes them. */
export function blankClasses(): ClassMapping {
  return { empty: '', low: '', medium: '', saturated: '' };
}

/** What to write on a class button: the site's own words, or the class name. */
export function buttonLabel(
  density: DensityClass,
  classes: ClassMapping | null,
  fallback: string,
): string {
  const described = classes?.[density]?.trim();
  return described && described.length > 0 ? described : fallback;
}

/** A duration in the units a reader of a running capture thinks in. */
export function formatElapsed(seconds: number): string {
  const whole = Math.max(0, Math.floor(seconds));
  if (whole < 60) {
    return `${whole} s`;
  }
  const minutes = Math.floor(whole / 60);
  if (minutes < 60) {
    return `${minutes} min ${whole % 60} s`;
  }
  return `${Math.floor(minutes / 60)} h ${minutes % 60} min`;
}

/** Seconds between two appliance timestamps, never negative. */
export function secondsBetween(fromUs: number, toUs: number): number {
  return Math.max(0, (toUs - fromUs) / 1_000_000);
}

/**
 * Whether the capture is recording frames that training will discard.
 *
 * Windows before the first label carry no class and are dropped, so a capture
 * left unlabelled is recording nothing usable — silently, which is the whole
 * reason to say it out loud.
 */
export function isDiscardingFrames(labelled: boolean, elapsedSeconds: number): boolean {
  return !labelled && elapsedSeconds >= GRACE_SECONDS;
}

/** How long an unlabelled capture is given before it is worth a warning. */
export const GRACE_SECONDS = 10;

/** The order the class buttons are shown in: densest first, thumb nearest. */
export function buttonOrder(): DensityClass[] {
  return [...DENSITY_CLASSES].reverse();
}

/**
 * A description to start from, so nobody has to invent one while a queue is
 * forming. Edited freely — it is only a first line.
 */
export function defaultEnvironment(siteName: string | null, at: Date, locale: string): string {
  const when = at.toLocaleString(locale, {
    day: 'numeric',
    month: 'long',
    hour: '2-digit',
    minute: '2-digit',
  });
  return siteName ? `${siteName} — ${when}` : when;
}

/** How many recordings a page of the history shows. */
export const PAGE_SIZE = 8;

/** The slice of the history a given page shows, clamped to what exists. */
export function page<T>(items: T[], index: number): T[] {
  const pages = Math.max(1, Math.ceil(items.length / PAGE_SIZE));
  const clamped = Math.min(Math.max(0, index), pages - 1);
  return items.slice(clamped * PAGE_SIZE, clamped * PAGE_SIZE + PAGE_SIZE);
}

/** How many pages the history needs. */
export function pageCount(total: number): number {
  return Math.max(1, Math.ceil(total / PAGE_SIZE));
}

/** Presentation of what the machine reports about itself. */

/**
 * Uptime as a duration a reader can act on.
 *
 * Days carry their remaining hours, because "1 d" for an appliance up for
 * 47 hours hides that it is about to reach two — which is exactly the kind of
 * thing someone reads this figure to find out.
 */
export function formatUptime(seconds: number): string {
  if (seconds < 60) {
    return `${Math.floor(seconds)} s`;
  }
  if (seconds < 3600) {
    return `${Math.floor(seconds / 60)} min`;
  }
  if (seconds < 86_400) {
    return `${Math.floor(seconds / 3600)} h`;
  }
  const days = Math.floor(seconds / 86_400);
  const hours = Math.floor((seconds % 86_400) / 3600);
  return hours === 0 ? `${days} d` : `${days} d ${hours} h`;
}

/** Kibibytes as whole mebibytes, which is the scale a reader thinks in. */
export function asMegabytes(kilobytes: number): number {
  return Math.round(kilobytes / 1024);
}

/** Kibibytes as gibibytes to one decimal, the scale a card is sold in. */
export function asGigabytes(kilobytes: number): string {
  return (kilobytes / 1_048_576).toFixed(1);
}

export function usedPercent(total: number, available: number): number {
  return Math.round(((total - available) / total) * 100);
}

/** The load average as a share of what the cores can take. */
export function loadPercent(load: number, cpus: number): number {
  return Math.round((load / cpus) * 100);
}

/** How far the appliance clock may sit from this device's before it is said. */
export const CLOCK_TOLERANCE_MS = 120_000;

/**
 * The appliance has no battery-backed clock: left unset after a power cut, it
 * dates what it records from where it stopped.
 */
export function clocksDisagree(applianceUs: number, deviceMs: number): boolean {
  return Math.abs(applianceUs / 1000 - deviceMs) > CLOCK_TOLERANCE_MS;
}

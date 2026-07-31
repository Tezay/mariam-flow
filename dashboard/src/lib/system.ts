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

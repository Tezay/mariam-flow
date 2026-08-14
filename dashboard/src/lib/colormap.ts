/**
 * The amplitude scale the appliance and the Python session report share.
 *
 * Viridis is what `flow_ml.report` draws its heatmaps with, so the two read
 * alike; it is also perceptually uniform, which a single hue cannot be beyond
 * five or six levels.
 *
 * Thirty-two anchors rather than matplotlib's 256: interpolating between them
 * lands within a rounding step of the full table, at a twentieth of the size.
 */

type Rgb = [number, number, number];

const VIRIDIS: Rgb[] = [
  [68, 1, 84],
  [71, 13, 96],
  [72, 24, 106],
  [72, 35, 116],
  [71, 46, 124],
  [69, 56, 130],
  [66, 65, 134],
  [62, 74, 137],
  [58, 84, 140],
  [54, 93, 141],
  [50, 101, 142],
  [46, 109, 142],
  [43, 117, 142],
  [40, 125, 142],
  [37, 132, 142],
  [34, 140, 141],
  [31, 148, 140],
  [30, 156, 137],
  [32, 163, 134],
  [37, 171, 130],
  [46, 179, 124],
  [58, 186, 118],
  [72, 193, 110],
  [88, 199, 101],
  [108, 205, 90],
  [127, 211, 78],
  [147, 215, 65],
  [168, 219, 52],
  [192, 223, 37],
  [213, 226, 26],
  [234, 229, 26],
  [253, 231, 37],
];

/** A hole in a capture: viridis holds no warm grey, so nothing can be
 * mistaken for it. */
export const HOLE: Rgb = [0xf0, 0xe4, 0xd0];

/** The colour a normalised amplitude in `[0, 1]` sits at. */
export function viridis(t: number): Rgb {
  const clamped = Math.min(1, Math.max(0, t));
  const position = clamped * (VIRIDIS.length - 1);
  const low = Math.floor(position);
  const high = Math.min(VIRIDIS.length - 1, low + 1);
  const blend = position - low;
  return [
    VIRIDIS[low][0] + (VIRIDIS[high][0] - VIRIDIS[low][0]) * blend,
    VIRIDIS[low][1] + (VIRIDIS[high][1] - VIRIDIS[low][1]) * blend,
    VIRIDIS[low][2] + (VIRIDIS[high][2] - VIRIDIS[low][2]) * blend,
  ];
}

/** The scale as CSS, for a colour bar that must match the canvas exactly. */
export function viridisGradient(): string {
  const stops = VIRIDIS.map(([r, g, b], index) => {
    const percent = Math.round((index / (VIRIDIS.length - 1)) * 100);
    return `rgb(${r} ${g} ${b}) ${percent}%`;
  });
  return `linear-gradient(to right, ${stops.join(', ')})`;
}

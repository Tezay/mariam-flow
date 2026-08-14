<script lang="ts">
  import uPlot from 'uplot';
  import 'uplot/dist/uPlot.min.css';

  import { type Portrait } from '$lib/api/portrait';
  import { type Viewport, featureSeries, labelSegments, seconds } from '$lib/portrait';
  import { t } from '$lib/i18n/i18n.svelte';

  let {
    portrait,
    feature,
    viewport,
    onzoom,
    oncursor,
    ongutters,
  }: {
    portrait: Portrait;
    feature: string;
    viewport: Viewport;
    onzoom: (viewport: Viewport) => void;
    /** Seconds under the pointer, or nothing once it leaves. */
    oncursor: (at: number | null) => void;
    /** What uPlot reserves either side, so the other panels can match it. */
    ongutters: (left: number, right: number) => void;
  } = $props();

  const HEIGHT = 190;
  /** Below this spacing points merge into the line and stop informing. */
  const POINT_SPACING = 9;

  let host = $state<HTMLDivElement | null>(null);
  let chart: uPlot | null = null;
  /** Set once the capture's own span has replaced uPlot's data-fitted one. */
  let framed = false;

  /* Series colours and class tints are read from the theme rather than
     restated here, so a token changed in one place does not leave a chart
     behind. */
  const STROKES = ['--color-mariam-600', '--color-ink-500'];

  function tints(styles: CSSStyleDeclaration): Map<string, string> {
    const named = ['empty', 'low', 'medium', 'saturated'] as const;
    return new Map(
      named.map((name) => [name, styles.getPropertyValue(`--color-density-${name}`).trim()]),
    );
  }

  /** Paints each marked stretch behind the series. */
  function shadeClasses(u: uPlot, palette: Map<string, string>) {
    const { ctx } = u;
    ctx.save();
    ctx.globalAlpha = 0.13;
    for (const segment of labelSegments(portrait)) {
      const from = u.valToPos(seconds(portrait, segment.from_us), 'x', true);
      const to = u.valToPos(seconds(portrait, segment.to_us), 'x', true);
      ctx.fillStyle = palette.get(segment.density) ?? 'transparent';
      ctx.fillRect(from, u.bbox.top, to - from, u.bbox.height);
    }
    ctx.restore();
  }

  $effect(() => {
    const container = host;
    if (!container) {
      return;
    }
    const styles = getComputedStyle(container);
    const palette = tints(styles);
    const [at] = featureSeries(portrait, portrait.nodes[0], feature);
    const values = portrait.nodes.map((node) => featureSeries(portrait, node, feature)[1]);

    const built = new uPlot(
      {
        width: container.clientWidth || 640,
        height: HEIGHT,
        legend: { show: false },
        scales: { x: { time: false } },
        axes: [{ label: t('portrait.seconds') }, {}],
        cursor: { y: false, drag: { x: true, y: false } },
        series: [
          {},
          ...portrait.nodes.map((node, index) => ({
            label: node.node_id,
            stroke: styles.getPropertyValue(STROKES[index % STROKES.length]).trim(),
            width: 1.5,
            spanGaps: false,
            points: {
              // Shown as soon as they fit: a reader has to see where the
              // measurements actually are, not only the line through them.
              show: (u: uPlot, _s: number, first: number, last: number) =>
                (last - first + 1) * POINT_SPACING <= u.bbox.width / devicePixelRatio,
              size: 5,
            },
          })),
        ],
        hooks: {
          drawClear: [(u) => shadeClasses(u, palette)],
          setScale: [
            (u, key) => {
              // uPlot frames x on the data, which begins one analysis window
              // after the capture does. Publishing that would hand every other
              // panel a stretch that is not the recording.
              if (key !== 'x' || !framed) {
                return;
              }
              const { min, max } = u.scales.x;
              if (min !== undefined && max !== undefined) {
                onzoom({ from: min, to: max });
              }
            },
          ],
          setCursor: [
            (u) => {
              const value = u.cursor.left === undefined ? null : u.posToVal(u.cursor.left, 'x');
              oncursor(u.cursor.idx === null || value === null ? null : value);
            },
          ],
        },
      },
      [at, ...values],
      container,
    );
    chart = built;
    built.setScale('x', { min: viewport.from, max: viewport.to });
    framed = true;
    ongutters(
      built.bbox.left / devicePixelRatio,
      (container.clientWidth * devicePixelRatio - built.bbox.left - built.bbox.width) /
        devicePixelRatio,
    );

    const observer = new ResizeObserver(() => {
      built.setSize({ width: container.clientWidth, height: HEIGHT });
      ongutters(
        built.bbox.left / devicePixelRatio,
        (container.clientWidth * devicePixelRatio - built.bbox.left - built.bbox.width) /
          devicePixelRatio,
      );
    });
    observer.observe(container);
    return () => {
      observer.disconnect();
      built.destroy();
      chart = null;
      framed = false;
    };
  });

  /* uPlot owns the x scale; `viewport` mirrors it. Applying a value it already
     holds would loop through `setScale`, so the guard is what keeps the
     reset button and a drag from fighting each other. */
  $effect(() => {
    const { from, to } = viewport;
    const scale = chart?.scales.x;
    if (chart && (scale?.min !== from || scale?.max !== to)) {
      chart.setScale('x', { min: from, max: to });
    }
  });
</script>

<div bind:this={host} class="w-full"></div>

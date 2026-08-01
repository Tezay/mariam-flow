<script lang="ts">
  import LineChart from '@lucide/svelte/icons/chart-line';
  import Table from '@lucide/svelte/icons/table';

  import { DENSITY_CLASSES, type MinuteSummary } from '$lib/api';
  import { formattingLocale, hour12, t } from '$lib/i18n/i18n.svelte';
  import {
    DENSITY_FILL,
    DENSITY_SWATCH,
    areaPath,
    densityName,
    formatClock,
    linePath,
    nearestIndex,
    plotGeometry,
  } from '$lib/live';

  let { minutes }: { minutes: MinuteSummary[] } = $props();

  /* The figure carries two readings of the same hour, sharing one x-axis:
     the waiting time as a line — a continuous magnitude, which answers "is
     it growing?" — and the density class as a continuous band beneath it —
     an ordinal state, which answers "was it saturated at noon?". Averaging
     the class would be meaningless, so it is never plotted as a height. */

  /** Headroom above the plot, so the peak's hover marker is not clipped. */
  const TOP = 8;
  const PLOT_HEIGHT = 96;
  const BAND_HEIGHT = 12;
  const BAND_GAP = 8;
  const AXIS_HEIGHT = 18;

  const HEIGHT = TOP + PLOT_HEIGHT + BAND_GAP + BAND_HEIGHT + AXIS_HEIGHT;
  const BASELINE = TOP + PLOT_HEIGHT;

  /* The viewBox is sized to the measured element, so one SVG unit is always
     one CSS pixel. A fixed viewBox stretched to the container scales
     everything with the width instead — a 2px line becoming ten, 9px labels
     becoming forty on a wide screen, which is what a full-width desktop
     card did. The height stays fixed: a chart is not more informative for
     being taller. */
  let width = $state(320);

  /* Hover is an enhancement, never a gate: every value the crosshair shows
     is also in the table view, which is the keyboard and screen-reader path
     to the same numbers. */
  let showTable = $state(false);
  let hovered = $state<number | null>(null);

  const geometry = $derived(
    plotGeometry(
      minutes.map((minute) => minute.wait_minutes),
      { width, top: TOP, height: PLOT_HEIGHT },
    ),
  );

  const cellWidth = $derived(minutes.length === 0 ? 0 : width / minutes.length);
  const hoveredMinute = $derived(hovered === null ? null : (minutes[hovered] ?? null));

  /** Newest first: a reader scanning a log looks at the top for "now". */
  const rows = $derived([...minutes].reverse());

  function clockOf(minute_us: number): string {
    return formatClock(minute_us, formattingLocale(), hour12());
  }

  /** Snaps the pointer to the nearest minute — readers aim at a time. */
  function track(event: PointerEvent) {
    const box = (event.currentTarget as SVGElement).getBoundingClientRect();
    hovered = nearestIndex((event.clientX - box.left) / box.width, minutes.length);
  }
</script>

<section class="rounded-xl bg-white p-4 ring-1 ring-ink-100">
  <header class="flex items-baseline justify-between gap-3">
    <h3 class="text-sm font-medium text-ink-900">{t('live.history')}</h3>
    <button
      type="button"
      onclick={() => (showTable = !showTable)}
      class="inline-flex items-center gap-1.5 rounded-md px-2 py-1 text-xs text-ink-500
             transition-colors hover:bg-ink-100 hover:text-ink-900"
    >
      {#if showTable}
        <LineChart size={14} aria-hidden="true" />{t('live.showChart')}
      {:else}
        <Table size={14} aria-hidden="true" />{t('live.showTable')}
      {/if}
    </button>
  </header>

  {#if minutes.length < 2}
    <p class="mt-3 text-sm text-ink-500">{t('live.historyEmpty')}</p>
  {:else if showTable}
    <!-- The table view is the chart's twin: every value is reachable
         without hovering. The swatch beside each level speeds up scanning,
         but the level is always written out too, so the table still reads
         without colour. -->
    <div class="mt-3 max-h-64 overflow-y-auto">
      <table class="w-full text-left text-sm tabular-nums">
        <thead class="sticky top-0 bg-white text-xs uppercase tracking-wide text-ink-500">
          <tr>
            <th scope="col" class="py-1 font-medium">{t('live.time')}</th>
            <th scope="col" class="py-1 font-medium">{t('live.wait')}</th>
            <th scope="col" class="py-1 font-medium">{t('live.class')}</th>
          </tr>
        </thead>
        <tbody>
          {#each rows as minute (minute.minute_us)}
            <tr class="border-t border-ink-100">
              <td class="py-1 text-ink-500">{clockOf(minute.minute_us)}</td>
              <td class="py-1 text-ink-900">{minute.wait_minutes.toFixed(1)}</td>
              <td class="py-1 text-ink-900">
                <span class="flex items-center gap-2">
                  <span
                    class="size-2.5 shrink-0 rounded-sm {DENSITY_SWATCH[densityName(minute.class)]}"
                  ></span>
                  {t(`class.${densityName(minute.class)}` as const)}
                </span>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {:else}
    <figure class="mt-3">
      <div bind:clientWidth={width}>
        <svg
          viewBox="0 0 {width} {HEIGHT}"
          {width}
          height={HEIGHT}
          class="touch-none"
          role="img"
          aria-label={t('live.history')}
          onpointermove={track}
          onpointerleave={() => (hovered = null)}
        >
          <!-- Recessive hairlines, solid: a grid is not a threshold. -->
          {#each [0, 0.5, 1] as ratio (ratio)}
            <line
              x1="0"
              x2={width}
              y1={TOP + PLOT_HEIGHT * ratio}
              y2={TOP + PLOT_HEIGHT * ratio}
              stroke="var(--color-ink-100)"
              stroke-width="1"
            />
          {/each}

          <path
            d={areaPath(geometry.points, { width, baseline: BASELINE })}
            fill="var(--color-mariam-600)"
            fill-opacity="0.1"
          />
          <path
            d={linePath(geometry.points)}
            fill="none"
            stroke="var(--color-mariam-600)"
            stroke-width="2"
            stroke-linejoin="round"
            stroke-linecap="round"
          />

          <!-- One timeline coloured by state, not sixty discrete marks:
               neighbouring minutes of the same class are meant to merge. -->
          {#each minutes as minute, index (minute.minute_us)}
            <rect
              x={index * cellWidth}
              y={BASELINE + BAND_GAP}
              width={cellWidth + 0.5}
              height={BAND_HEIGHT}
              class={DENSITY_FILL[densityName(minute.class)]}
            />
          {/each}

          {#if hoveredMinute && hovered !== null}
            {@const point = geometry.points[hovered]}
            <line
              x1={point.x}
              x2={point.x}
              y1={TOP}
              y2={BASELINE + BAND_GAP + BAND_HEIGHT}
              stroke="var(--color-ink-200)"
              stroke-width="1"
            />
            <circle
              cx={point.x}
              cy={point.y}
              r="4"
              fill="var(--color-mariam-600)"
              stroke="white"
              stroke-width="2"
            />
          {/if}

          <text x="0" y={HEIGHT - 4} class="fill-ink-500 text-[10px]">
            {clockOf(minutes[0].minute_us)}
          </text>
          <text x={width} y={HEIGHT - 4} text-anchor="end" class="fill-ink-500 text-[10px]">
            {clockOf(minutes[minutes.length - 1].minute_us)}
          </text>
        </svg>
      </div>

      <!-- The legend lives inside the caption: it is the identity key for
           the band, and a figure caption must be the first or last child. -->
      <figcaption class="mt-2 space-y-2 text-xs text-ink-500">
        {#if hoveredMinute}
          <!-- Value leads, label follows: the reader has the time and wants
               the number. -->
          <p>
            <span class="font-medium tabular-nums text-ink-900">
              {hoveredMinute.wait_minutes.toFixed(1)}
              {t('live.minutesShort')}
            </span>
            · {clockOf(hoveredMinute.minute_us)} ·
            {t(`class.${densityName(hoveredMinute.class)}` as const)}
          </p>
        {:else}
          <p>{t('live.peak', { value: geometry.peak.toFixed(1) })}</p>
        {/if}

        <ul class="flex flex-wrap gap-x-3 gap-y-1">
          {#each DENSITY_CLASSES as name (name)}
            <li class="flex items-center gap-1.5">
              <span class="size-2.5 rounded-sm {DENSITY_SWATCH[name]}"></span>
              {t(`class.${name}` as const)}
            </li>
          {/each}
        </ul>
      </figcaption>
    </figure>
  {/if}
</section>

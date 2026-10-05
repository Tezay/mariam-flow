<script lang="ts">
  import LineChart from '@lucide/svelte/icons/chart-line';
  import Table from '@lucide/svelte/icons/table';

  import { DENSITY_CLASSES, type Estimate, type MinuteSummary } from '$lib/api/live';
  import {
    MINUTE_US,
    RANGES,
    type Range,
    linePath,
    pointedAt,
    rangeLabel,
    stretches,
    timeTicks,
    waitTicks,
  } from '$lib/history';
  import { formattingLocale, hour12, t } from '$lib/i18n/i18n.svelte';
  import { DENSITY_FILL, DENSITY_SWATCH, densityName, formatClock } from '$lib/live';

  let {
    minutes,
    range,
    nowUs,
    current,
    onrange,
  }: {
    minutes: MinuteSummary[];
    range: Range;
    /** The appliance clock, which the axis ends on. */
    nowUs: number | null;
    /** The estimate in progress, which continues the curve to the present. */
    current: Estimate | null;
    onrange: (range: Range) => void;
  } = $props();

  const uid = $props.id();

  /* The figure carries two readings of the same period, sharing one x-axis:
     the waiting time as a line — a continuous magnitude, which answers "is
     it growing?" — and the density class as a continuous band beneath it —
     an ordinal state, which answers "was it saturated at noon?". Averaging
     the class would be meaningless, so it is never plotted as a height. */

  /** Room for the unit, and for the marker of a point at the top of the scale. */
  const TOP = 16;
  const PLOT_HEIGHT = 96;
  const BAND_HEIGHT = 12;
  const BAND_GAP = 8;
  const AXIS_HEIGHT = 18;
  /** Room on the left for the graduations, and on the right for the present's marker. */
  const GUTTER = 28;
  const MARGIN = 8;

  const HEIGHT = TOP + PLOT_HEIGHT + BAND_GAP + BAND_HEIGHT + AXIS_HEIGHT;
  const BASELINE = TOP + PLOT_HEIGHT;
  const BAND = BASELINE + BAND_GAP;

  /* The viewBox is sized to the measured element, so one SVG unit is always
     one CSS pixel. A fixed viewBox stretched to the container scales
     everything with the width instead — a 2px line becoming ten, 9px labels
     becoming forty on a wide screen. The height stays fixed: a chart is not
     more informative for being taller. */
  let width = $state(320);

  /* Hover is an enhancement, never a gate: every value the crosshair shows
     is also in the table view, which is the keyboard and screen-reader path
     to the same numbers. */
  let showTable = $state(false);
  let pointer = $state<number | null>(null);

  const to = $derived(nowUs ?? (minutes.at(-1)?.minute_us ?? 0) + MINUTE_US);
  const from = $derived(to - range * MINUTE_US);
  const parts = $derived(stretches(minutes, from, to, current !== null));
  const shown = $derived(parts.flatMap((part) => (part.kind === 'run' ? part.minutes : [])));
  const empty = $derived(shown.length === 0 && current === null);

  const peak = $derived(
    Math.max(0, current?.wait_minutes ?? 0, ...shown.map((minute) => minute.wait_minutes)),
  );
  const graduations = $derived(waitTicks(peak));
  const ceiling = $derived(graduations[graduations.length - 1]);

  const x = (us: number) => GUTTER + ((us - from) / (to - from)) * (width - GUTTER - MARGIN);
  const y = (wait: number) => BASELINE - (wait / ceiling) * PLOT_HEIGHT;

  const curves = $derived(
    parts.flatMap((part) => {
      if (part.kind !== 'run') {
        return [];
      }
      const points = part.minutes.map((minute) => ({
        x: x(minute.minute_us + MINUTE_US / 2),
        y: y(minute.wait_minutes),
      }));
      /* Carried flat to both ends of the run: plotted at their centres alone,
         the minutes would stop half a minute short of the band beneath. */
      if (points.length > 0) {
        points.unshift({ x: x(part.from_us), y: points[0].y });
      }
      if (part.live && current) {
        points.push({ x: x(to), y: y(current.wait_minutes) });
      } else if (points.length > 0) {
        points.push({ x: x(part.to_us), y: points[points.length - 1].y });
      }
      return points.length > 1 ? [points] : [];
    }),
  );

  const pointed = $derived(pointer === null ? null : pointedAt(parts, pointer));
  const marker = $derived.by(() => {
    if (pointed?.kind === 'minute') {
      return {
        x: x(pointed.minute.minute_us + MINUTE_US / 2),
        y: y(pointed.minute.wait_minutes),
      };
    }
    if (pointed?.kind === 'now' && current) {
      return { x: x(to), y: y(current.wait_minutes) };
    }
    return pointed && pointer !== null ? { x: x(pointer), y: null } : null;
  });

  /** Newest first: a reader scanning a log looks at the top for "now". */
  const rows = $derived([...shown].reverse());

  function clockOf(us: number): string {
    return formatClock(us, formattingLocale(), hour12());
  }

  function track(event: PointerEvent) {
    const box = (event.currentTarget as SVGElement).getBoundingClientRect();
    const ratio = (event.clientX - box.left - GUTTER) / (box.width - GUTTER - MARGIN);
    pointer = from + Math.max(0, Math.min(1, ratio)) * (to - from);
  }
</script>

{#snippet swatch()}
  <svg width="10" height="10" aria-hidden="true" class="rounded-sm">
    <rect width="10" height="10" fill="url(#{uid}-gap)" />
  </svg>
{/snippet}

<section class="rounded-md bg-white p-4 ring-1 ring-ink-100">
  <header class="flex flex-wrap items-center justify-between gap-x-3 gap-y-2">
    <h3 class="text-sm font-medium text-ink-900">{t('live.history')}</h3>
    <div class="flex items-center gap-2">
      <div role="group" aria-label={t('live.range')} class="flex rounded-md bg-ink-50 p-0.5">
        {#each RANGES as option (option)}
          <button
            type="button"
            aria-pressed={option === range}
            onclick={() => onrange(option)}
            class="rounded px-2 py-1 text-xs transition-colors {option === range
              ? 'bg-white font-medium text-ink-900 ring-1 ring-ink-200'
              : 'text-ink-500 hover:text-ink-900'}"
          >
            {rangeLabel(option)}
          </button>
        {/each}
      </div>
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
    </div>
  </header>

  {#if empty}
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
          onpointerleave={() => (pointer = null)}
        >
          <defs>
            <pattern
              id="{uid}-gap"
              width="6"
              height="6"
              patternUnits="userSpaceOnUse"
              patternTransform="rotate(45)"
            >
              <rect width="6" height="6" fill="var(--color-ink-100)" />
              <line x1="0" y1="0" x2="0" y2="6" stroke="var(--color-ink-300)" stroke-width="2" />
            </pattern>
            <!-- A minute straddling the start of the window is plotted at its
                 centre, which lies outside it. -->
            <clipPath id="{uid}-plot">
              <rect x={GUTTER} y="0" width={width - GUTTER} height={HEIGHT} />
            </clipPath>
          </defs>

          <text x="0" y="9" class="fill-ink-500 text-[10px]">{t('live.minutesShort')}</text>
          {#each graduations as value (value)}
            <line
              x1={GUTTER}
              x2={width - MARGIN}
              y1={y(value)}
              y2={y(value)}
              stroke="var(--color-ink-100)"
              stroke-width="1"
            />
            <text
              x={GUTTER - 6}
              y={y(value) + 3}
              text-anchor="end"
              class="fill-ink-500 text-[10px] tabular-nums"
            >
              {Number.isInteger(value) ? value : value.toFixed(1)}
            </text>
          {/each}

          <g clip-path="url(#{uid}-plot)">
            {#each curves as points, index (index)}
              {@const first = points[0]}
              {@const last = points[points.length - 1]}
              <path
                d="{linePath(points)} L{last.x.toFixed(1)},{BASELINE} L{first.x.toFixed(
                  1,
                )},{BASELINE} Z"
                fill="var(--color-mariam-600)"
                fill-opacity="0.1"
              />
              <path
                d={linePath(points)}
                fill="none"
                stroke="var(--color-mariam-600)"
                stroke-width="2"
                stroke-linejoin="round"
                stroke-linecap="round"
              />
            {/each}

            <!-- One timeline coloured by state, not a mark per minute:
                 neighbouring minutes of the same class are meant to merge.
                 The minute in progress is left out: its class is settled
                 only when it ends, and a colour shown must not change. -->
            {#each parts as part (part.from_us)}
              {#if part.kind === 'gap'}
                <rect
                  data-gap
                  x={x(part.from_us)}
                  y={BAND}
                  width={x(part.to_us) - x(part.from_us)}
                  height={BAND_HEIGHT}
                  fill="url(#{uid}-gap)"
                />
              {:else}
                {#each part.minutes as minute (minute.minute_us)}
                  <rect
                    x={x(minute.minute_us)}
                    y={BAND}
                    width={x(minute.minute_us + MINUTE_US) - x(minute.minute_us) + 0.5}
                    height={BAND_HEIGHT}
                    class={DENSITY_FILL[densityName(minute.class)]}
                  />
                {/each}
              {/if}
            {/each}
          </g>

          {#if current}
            <circle
              cx={x(to)}
              cy={y(current.wait_minutes)}
              r="4"
              fill="var(--color-mariam-600)"
              stroke="white"
              stroke-width="2"
            />
          {/if}

          {#if marker}
            <line
              x1={marker.x}
              x2={marker.x}
              y1={TOP}
              y2={BAND + BAND_HEIGHT}
              stroke="var(--color-ink-300)"
              stroke-width="1"
            />
            {#if marker.y !== null}
              <circle
                cx={marker.x}
                cy={marker.y}
                r="4"
                fill="var(--color-mariam-600)"
                stroke="white"
                stroke-width="2"
              />
            {/if}
          {/if}

          {#each timeTicks(from, to, range / 6) as tick (tick)}
            <!-- Left off where a label would run past either edge. -->
            {#if x(tick) > GUTTER + 14 && x(tick) < width - 14}
              <text
                x={x(tick)}
                y={HEIGHT - 4}
                text-anchor="middle"
                class="fill-ink-500 text-[10px] tabular-nums"
              >
                {clockOf(tick)}
              </text>
            {/if}
          {/each}
        </svg>
      </div>

      <!-- The legend lives inside the caption: it is the identity key for
           the band, and a figure caption must be the first or last child. -->
      <figcaption class="mt-2 space-y-2 text-xs text-ink-500">
        <!-- Value leads, label follows: the reader has the time and wants
             the number. -->
        {#if pointed?.kind === 'minute'}
          <p>
            <span class="font-medium tabular-nums text-ink-900">
              {pointed.minute.wait_minutes.toFixed(1)}
              {t('live.minutesShort')}
            </span>
            · {clockOf(pointed.minute.minute_us)} ·
            {t(`class.${densityName(pointed.minute.class)}` as const)}
          </p>
        {:else if pointed?.kind === 'now' && current}
          <p>
            <span class="font-medium tabular-nums text-ink-900">
              {current.wait_minutes.toFixed(1)}
              {t('live.minutesShort')}
            </span>
            · {t('live.now')} ·
            {t(`class.${current.class}` as const)}
          </p>
        {:else if pointed?.kind === 'gap'}
          <p>
            {t('live.gap', { from: clockOf(pointed.from_us), to: clockOf(pointed.to_us) })}
          </p>
        {:else}
          <p>{t('live.peak', { value: peak.toFixed(1) })}</p>
        {/if}

        <ul class="flex flex-wrap gap-x-3 gap-y-1">
          {#each DENSITY_CLASSES as name (name)}
            <li class="flex items-center gap-1.5">
              <span class="size-2.5 rounded-sm {DENSITY_SWATCH[name]}"></span>
              {t(`class.${name}` as const)}
            </li>
          {/each}
          <li class="flex items-center gap-1.5">{@render swatch()}{t('live.noEstimate')}</li>
        </ul>
        <p>{t('live.bandMeaning')}</p>
      </figcaption>
    </figure>
  {/if}
</section>

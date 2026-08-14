<script lang="ts">
  import Maximize from '@lucide/svelte/icons/maximize-2';

  import { type Portrait } from '$lib/api/portrait';
  import {
    type Viewport,
    binSeconds,
    fractionIn,
    isZoomed,
    labelAt,
    seconds,
    wholeCapture,
  } from '$lib/portrait';
  import { t } from '$lib/i18n/i18n.svelte';
  import ColourBar from '$components/analysis/ColourBar.svelte';
  import CsiHeatmap from '$components/analysis/CsiHeatmap.svelte';
  import FeatureChart from '$components/analysis/FeatureChart.svelte';
  import LabelBand from '$components/analysis/LabelBand.svelte';
  import Button from '$components/ui/Button.svelte';

  let {
    portrait,
    pixels,
    feature,
  }: {
    portrait: Portrait;
    pixels: Uint8Array | null;
    feature: string;
  } = $props();

  /* One viewport and one cursor for every panel: without them these are three
     pictures that happen to be stacked, which is what makes a shared axis, a
     shared zoom and a crosshair impossible. */
  // svelte-ignore state_referenced_locally
  let viewport = $state<Viewport>(wholeCapture(portrait));
  let cursor = $state<number | null>(null);
  /* uPlot reserves space for its axis; the panels beside it have to give up
     exactly as much, or nothing lines up. It is measured rather than guessed
     because uPlot sizes it from the tick labels it ends up drawing. */
  let gutters = $state({ left: 48, right: 8 });

  const zoomed = $derived(isZoomed(portrait, viewport));
  const cursorAt = $derived(cursor === null ? null : fractionIn(viewport, cursor));
  const marked = $derived(
    cursor === null ? null : labelAt(portrait, portrait.started_at_us + cursor * 1_000_000),
  );

  /** The value each receiver measured nearest the cursor. */
  const readings = $derived.by(() => {
    const at = cursor;
    if (at === null || portrait.feature_ts_us.length === 0) {
      return [];
    }
    const nearest = portrait.feature_ts_us.reduce(
      (best, ts, index) =>
        Math.abs(seconds(portrait, ts) - at) <
        Math.abs(seconds(portrait, portrait.feature_ts_us[best]) - at)
          ? index
          : best,
      0,
    );
    return portrait.nodes.map((node) => ({
      node_id: node.node_id,
      value: node.features[feature]?.[nearest] ?? null,
    }));
  });

  /* The declaration above frames the first capture; this reframes when another
     one is opened, whose span has nothing to do with the window left over. */
  $effect(() => {
    void portrait.session_id;
    viewport = wholeCapture(portrait);
  });
</script>

<div style:--gutter-left="{gutters.left}px" style:--gutter-right="{gutters.right}px">
  <!-- Reserved rather than conditional: a readout appearing under the pointer
       would shift every panel below it on each move. -->
  <div class="flex min-h-8 flex-wrap items-center gap-x-4 gap-y-1 text-sm">
    {#if cursor === null}
      <span class="text-ink-500">{t('portrait.hoverHint')}</span>
    {:else}
      <span class="tabular-nums text-ink-900">{cursor.toFixed(1)} s</span>
      {#each readings as reading (reading.node_id)}
        <span class="tabular-nums text-ink-500">
          {reading.node_id}
          <span class="text-ink-900">
            {reading.value === null ? '—' : reading.value.toFixed(2)}
          </span>
        </span>
      {/each}
      {#if marked}
        <span class="flex items-center gap-1.5 text-ink-500">
          <span
            class="size-2.5 shrink-0 rounded-xs"
            style:background-color="var(--color-density-{marked})"
          ></span>
          {t(`class.${marked}` as const)}
        </span>
      {/if}
    {/if}
    {#if zoomed}
      <span class="ms-auto flex items-center gap-2">
        <span class="text-xs tabular-nums text-ink-500">
          {viewport.from.toFixed(1)}–{viewport.to.toFixed(1)} s
        </span>
        <Button variant="outline" size="sm" onclick={() => (viewport = wholeCapture(portrait))}>
          <Maximize size={13} aria-hidden="true" />
          {t('portrait.resetZoom')}
        </Button>
      </span>
    {/if}
  </div>

  <!-- Every panel is inset by the chart's own gutters, so one instant sits at
       one horizontal position throughout. -->
  <div class="relative mt-2">
    <div style:padding-left="var(--gutter-left)" style:padding-right="var(--gutter-right)">
      <LabelBand {portrait} {viewport} />
    </div>

    {#each portrait.nodes as node, index (node.node_id)}
      {#if node.frames > 0 && pixels}
        <div class="mt-4">
          <div
            class="mb-1 flex flex-wrap items-baseline justify-between gap-2"
            style:padding-left="var(--gutter-left)"
            style:padding-right="var(--gutter-right)"
          >
            <span class="text-sm font-medium text-ink-900">{node.node_id}</span>
            <ColourBar low={node.amp_min} high={node.amp_max} />
          </div>
          <div style:padding-left="var(--gutter-left)" style:padding-right="var(--gutter-right)">
            <CsiHeatmap
              {pixels}
              {portrait}
              {index}
              {viewport}
              description={t('recording.heatmapAlt', { node: node.node_id })}
            />
          </div>
        </div>
      {:else}
        <p class="mt-4 rounded-sm bg-ink-50 py-6 text-center text-sm text-ink-500">
          {t('recording.nothingHeard', { node: node.node_id })}
        </p>
      {/if}
    {/each}

    <div class="mt-4">
      <FeatureChart
        {portrait}
        {feature}
        {viewport}
        onzoom={(next) => (viewport = next)}
        oncursor={(at) => (cursor = at)}
        ongutters={(left, right) => (gutters = { left, right })}
      />
    </div>

    {#if cursorAt !== null && cursorAt >= 0 && cursorAt <= 1}
      <div
        aria-hidden="true"
        class="pointer-events-none absolute inset-y-0 w-px bg-ink-900/30"
        style:left="calc(var(--gutter-left) + (100% - var(--gutter-left) - var(--gutter-right)) * {cursorAt})"
      ></div>
    {/if}
  </div>

  <p class="mt-2 text-xs text-ink-500">
    {t('portrait.resolution', { seconds: binSeconds(portrait).toFixed(2) })}
  </p>
</div>

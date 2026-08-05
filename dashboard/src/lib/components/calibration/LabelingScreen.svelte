<script lang="ts">
  import Square from '@lucide/svelte/icons/square';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';

  import { addLabel } from '$lib/api/calibration';
  import { DENSITY_CLASSES, type DensityClass } from '$lib/api/live';
  import { type ClassMapping } from '$lib/api/status';
  import { t } from '$lib/i18n/i18n.svelte';
  import {
    buttonLabel,
    buttonOrder,
    formatElapsed,
    isDiscardingFrames,
    secondsBetween,
  } from '$lib/calibration';
  import { DENSITY_SWATCH } from '$lib/live';

  let {
    classes,
    startedUs,
    nowUs,
    frames,
    silent,
    stopping,
    onstop,
  }: {
    classes: ClassMapping | null;
    startedUs: number;
    nowUs: number;
    frames: number;
    silent: boolean;
    stopping: boolean;
    onstop: () => void;
  } = $props();

  /* The active class and when it was marked are held here rather than read
     back from the appliance: the press is the event, and the screen has to
     answer it immediately — a labeller watching a queue cannot wait for a
     round trip to know the press registered. */
  let active = $state<DensityClass | null>(null);
  let markedUs = $state(0);
  let marks = $state(0);
  let failed = $state(false);
  let confirming = $state(false);

  const elapsed = $derived(secondsBetween(startedUs, nowUs));
  const held = $derived(active === null ? 0 : secondsBetween(markedUs, nowUs));
  const discarding = $derived(isDiscardingFrames(marks > 0, elapsed));

  async function mark(density: DensityClass) {
    const previous = active;
    active = density;
    markedUs = nowUs;
    const ok = await addLabel(DENSITY_CLASSES.indexOf(density));
    if (ok) {
      marks += 1;
      failed = false;
      return;
    }
    // The appliance refused it, so the screen must not claim it happened.
    active = previous;
    failed = true;
  }
</script>

<!-- Full frame while recording: the person holding the phone is watching a
     queue, not a dashboard. -->
<div class="fixed inset-0 z-50 flex flex-col bg-ink-900 text-white">
  <header class="flex items-baseline justify-between gap-3 px-4 pt-4 text-xs">
    <span class="flex items-center gap-2 font-medium tracking-wide uppercase">
      <!-- The one thing a glance has to answer: is this thing still on? -->
      <span class="relative flex size-2.5" aria-hidden="true">
        <span
          class="absolute inline-flex size-full animate-ping rounded-full bg-density-empty
                 opacity-75"
        ></span>
        <span class="relative inline-flex size-2.5 rounded-full bg-density-empty"></span>
      </span>
      {t('cal.recording')}
    </span>
    <span class="tabular-nums text-white/60">
      {t('cal.elapsed', { value: formatElapsed(elapsed) })} ·
      {t('cal.frames', { value: frames })} ·
      {t('cal.labels', { value: marks })}
    </span>
  </header>

  {#if silent}
    <!-- Labelling for an hour beside a dead sensor ruins the campaign and
         nothing else on this screen would say so. -->
    <p class="mx-4 mt-3 flex items-start gap-2 rounded-md bg-density-saturated/20 p-3 text-sm">
      <TriangleAlert size={16} class="mt-0.5 shrink-0" aria-hidden="true" />{t('cal.silent')}
    </p>
  {:else if discarding}
    <p class="mx-4 mt-3 flex items-start gap-2 rounded-md bg-density-medium/20 p-3 text-sm">
      <TriangleAlert size={16} class="mt-0.5 shrink-0" aria-hidden="true" />{t('cal.discarding')}
    </p>
  {:else if active === null}
    <p class="mx-4 mt-3 text-sm text-white/60">{t('cal.pressToBegin')}</p>
  {/if}

  {#if failed}
    <p role="status" class="mx-4 mt-3 text-sm text-density-saturated">{t('wizard.failed')}</p>
  {/if}

  <div class="flex min-h-0 flex-1 flex-col gap-2 p-4">
    {#each buttonOrder() as density (density)}
      <button
        type="button"
        onclick={() => void mark(density)}
        aria-pressed={active === density}
        class="flex flex-1 items-center gap-3 rounded-xl px-4 text-left text-lg font-medium
               transition-colors {active === density
          ? 'ring-2 ring-white'
          : 'opacity-70'} {DENSITY_SWATCH[density]}"
      >
        <span class="flex-1">{buttonLabel(density, classes, t(`class.${density}` as const))}</span>
        {#if active === density}
          <span class="text-sm font-normal tabular-nums text-white/80">
            {t('cal.since', { value: formatElapsed(held) })}
          </span>
        {/if}
      </button>
    {/each}
  </div>

  <div class="p-4 pt-0">
    <button
      type="button"
      onclick={() => (confirming = true)}
      disabled={stopping}
      class="flex w-full items-center justify-center gap-2 rounded-xl border border-white/30
             px-4 py-3 text-sm font-medium transition-colors hover:bg-white/10 disabled:opacity-50"
    >
      <Square size={16} aria-hidden="true" />
      {stopping ? t('cal.stopping') : t('cal.stop')}
    </button>
  </div>
</div>

{#if confirming}
  <!-- Sealing cannot be undone, and the press that seals sits under a thumb
       that has been tapping the same area for an hour. -->
  <div
    class="fixed inset-0 z-60 flex items-end justify-center bg-ink-900/80 p-4 sm:items-center"
    role="dialog"
    aria-modal="true"
  >
    <div class="w-full max-w-sm rounded-md bg-white p-5 text-ink-900">
      <h2 class="text-base font-semibold">{t('cal.confirmStop')}</h2>
      <p class="mt-2 text-sm text-ink-500">{t('cal.confirmStopLead')}</p>
      <div class="mt-5 flex flex-col gap-2">
        <button
          type="button"
          onclick={() => {
            confirming = false;
            onstop();
          }}
          class="rounded-md bg-mariam-600 px-4 py-3 text-sm font-medium text-white
                 transition-colors hover:bg-mariam-700"
        >
          {t('cal.confirmYes')}
        </button>
        <button
          type="button"
          onclick={() => (confirming = false)}
          class="rounded-md px-4 py-3 text-sm text-ink-500 transition-colors hover:bg-ink-100"
        >
          {t('cal.confirmNo')}
        </button>
      </div>
    </div>
  </div>
{/if}

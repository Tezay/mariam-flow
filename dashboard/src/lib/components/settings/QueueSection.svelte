<script lang="ts">
  import { untrack } from 'svelte';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';

  import { saveQueue } from '$lib/api/install';
  import { DENSITY_CLASSES } from '$lib/api/live';
  import { type ClassMapping, type Status, type WaitTuning } from '$lib/api/status';
  import { blankClasses, queueComplete } from '$lib/calibration';
  import { t } from '$lib/i18n/i18n.svelte';
  import Button from '$components/ui/Button.svelte';
  import Saved from '$components/ui/Saved.svelte';

  let {
    status,
    lead = null,
    advanced = false,
    submitLabel,
    onupdated,
  }: {
    status: Status;
    /** Shown above the fields where the question is met for the first time. */
    lead?: string | null;
    advanced?: boolean;
    submitLabel: string;
    onupdated: (status: Status) => void;
  } = $props();

  const DEFAULTS = { smoothing_tau_s: 30, hysteresis_margin: 0.15, min_confidence: 0.5 };

  // Read once: a draft from here on.
  let classes = $state<ClassMapping>({ ...blankClasses(), ...untrack(() => status.classes) });
  /* Left empty rather than defaulted when nothing has been described yet: a
     head count nobody entered would produce waiting times that look measured. */
  let people = $state<(number | null)[]>(
    untrack(() => status.wait?.people_per_class ?? [null, null, null, null]),
  );
  let rate = $state<number | null>(untrack(() => status.wait?.service_rate_per_min ?? null));
  let knobs = $state({ ...DEFAULTS, ...untrack(() => status.wait) });
  let saving = $state(false);
  let saved = $state(false);
  let failure = $state<string | null>(null);

  const ready = $derived(queueComplete(people, rate));

  async function submit() {
    if (!ready || rate === null) {
      return;
    }
    const wait: WaitTuning = {
      people_per_class: people.map((count) => count ?? 0) as WaitTuning['people_per_class'],
      service_rate_per_min: rate,
      smoothing_tau_s: knobs.smoothing_tau_s,
      hysteresis_margin: knobs.hysteresis_margin,
      min_confidence: knobs.min_confidence,
    };
    saving = true;
    const outcome = await saveQueue(classes, wait);
    saving = false;
    if (outcome.kind === 'ok') {
      failure = null;
      saved = true;
      setTimeout(() => (saved = false), 2500);
      onupdated(outcome.status);
      return;
    }
    failure =
      outcome.kind === 'refused'
        ? t('wizard.refused', { message: outcome.message })
        : t('wizard.failed');
  }
</script>

<div class="max-w-xl">
  {#if lead}
    <p class="mb-4 rounded-md bg-mariam-50 p-3 text-sm text-ink-900">{lead}</p>
  {/if}

  <p class="text-sm text-ink-500">{t('queue.lead')}</p>

  <div class="mt-4 overflow-hidden rounded-md ring-1 ring-ink-200">
    <div
      class="grid grid-cols-[minmax(0,1fr)_6rem] gap-3 border-b border-ink-100 bg-ink-50 px-4 py-2
             text-xs font-medium tracking-wide text-ink-500 uppercase"
    >
      <span>{t('queue.looksLike')}</span>
      <span>{t('queue.people')}</span>
    </div>
    {#each DENSITY_CLASSES as density, index (density)}
      <div class="grid grid-cols-[minmax(0,1fr)_6rem] items-end gap-3 px-4 py-3">
        <label class="block text-sm text-ink-900">
          <span class="text-xs font-medium tracking-wide text-ink-500 uppercase">
            {t(`class.${density}` as const)}
          </span>
          <input
            type="text"
            bind:value={classes[density]}
            placeholder={t(`queue.hint.${density}` as const)}
            class="mt-1 block w-full rounded-md border border-ink-200 px-3 py-2 text-base
                   font-normal text-ink-900"
          />
        </label>
        <input
          type="number"
          min="0"
          step="1"
          aria-label={t('queue.peopleAt', { level: t(`class.${density}` as const) })}
          bind:value={people[index]}
          class="block w-full rounded-md border border-ink-200 px-3 py-2 text-base
                 text-ink-900 tabular-nums"
        />
      </div>
    {/each}
  </div>

  <label class="mt-5 block text-sm font-medium text-ink-900">
    {t('queue.rate')}
    <span class="mt-1 flex items-center gap-2">
      <input
        type="number"
        min="0"
        step="0.5"
        bind:value={rate}
        class="w-28 rounded-md border border-ink-200 px-3 py-2 text-base font-normal
               text-ink-900 tabular-nums"
      />
      <span class="text-sm font-normal text-ink-500">{t('queue.ratePerMinute')}</span>
    </span>
  </label>
  <p class="mt-1 text-xs text-ink-500">{t('queue.rateHint')}</p>

  {#if advanced}
    <details class="group mt-5">
      <summary
        class="flex cursor-pointer items-center gap-1 text-sm text-ink-500 hover:text-ink-900"
      >
        <ChevronDown
          size={14}
          class="transition-transform group-open:rotate-180"
          aria-hidden="true"
        />
        {t('queue.advanced')}
      </summary>
      <div class="mt-3 space-y-3 border-l-2 border-ink-100 pl-4">
        <label class="block text-sm text-ink-900">
          {t('queue.smoothing')}
          <input
            type="number"
            min="1"
            step="1"
            bind:value={knobs.smoothing_tau_s}
            class="mt-1 block w-28 rounded-md border border-ink-200 px-3 py-2 text-base
                   text-ink-900 tabular-nums"
          />
        </label>
        <label class="block text-sm text-ink-900">
          {t('queue.hysteresis')}
          <input
            type="number"
            min="0"
            max="0.49"
            step="0.05"
            bind:value={knobs.hysteresis_margin}
            class="mt-1 block w-28 rounded-md border border-ink-200 px-3 py-2 text-base
                   text-ink-900 tabular-nums"
          />
        </label>
        <label class="block text-sm text-ink-900">
          {t('queue.confidence')}
          <input
            type="number"
            min="0"
            max="1"
            step="0.05"
            bind:value={knobs.min_confidence}
            class="mt-1 block w-28 rounded-md border border-ink-200 px-3 py-2 text-base
                   text-ink-900 tabular-nums"
          />
        </label>
      </div>
    </details>
  {/if}

  {#if failure}
    <p role="status" class="mt-3 text-sm text-danger">{failure}</p>
  {/if}

  <div class="mt-4">
    <Saved shown={saved} message={t('queue.saved')} />
    <Button disabled={saving || !ready} onclick={() => void submit()}>
      {saving ? t('wizard.saving') : submitLabel}
    </Button>
  </div>
</div>

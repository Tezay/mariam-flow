<script lang="ts">
  import CopyPlus from '@lucide/svelte/icons/copy-plus';
  import Plus from '@lucide/svelte/icons/plus';
  import X from '@lucide/svelte/icons/x';

  import {
    WEEKDAYS,
    fetchServiceWindow,
    saveServiceWindow,
    type ServiceWindow,
    type Weekday,
  } from '$lib/api';
  import { t } from '$lib/i18n/i18n.svelte';
  import Button from '$components/ui/Button.svelte';
  import {
    browserTimeZone,
    cloneWindow,
    copyDayToAll,
    emptyWindow,
    isNeverOpen,
    newInterval,
    timeZoneNames,
    validateWeek,
    type Problem,
  } from '$lib/schedule';

  /* The draft is a copy, never the loaded schedule itself, so "cancel" has
     something to go back to. `null` means no hours are declared, which is a
     state of its own: the appliance then estimates around the clock. */
  let loaded = $state<ServiceWindow | null>(null);
  let draft = $state<ServiceWindow | null>(null);
  let phase = $state<'loading' | 'ready' | 'saving'>('loading');
  let feedback = $state<{ tone: 'ok' | 'bad'; message: string } | null>(null);

  const zones = timeZoneNames();
  const detected = browserTimeZone();

  const problems = $derived(draft ? validateWeek(draft.weekly) : []);
  const dirty = $derived(JSON.stringify(draft) !== JSON.stringify(loaded));
  const neverOpen = $derived(draft !== null && isNeverOpen(draft.weekly));

  $effect(() => {
    void load();
  });

  async function load() {
    const window = await fetchServiceWindow();
    if (window === 'unreachable') {
      feedback = { tone: 'bad', message: t('hours.failed') };
      phase = 'ready';
      return;
    }
    loaded = window;
    draft = window ? cloneWindow(window) : null;
    phase = 'ready';
  }

  function declare() {
    draft = emptyWindow(detected);
    feedback = null;
  }

  function problemsOf(day: Weekday): Problem[] {
    return problems.filter((problem) => problem.day === day);
  }

  function describe(problem: Problem): string {
    if (problem.kind === 'time') {
      return t('hours.problem.time', { value: problem.value });
    }
    if (problem.kind === 'order') {
      return t('hours.problem.order', {
        from: problem.interval.from,
        to: problem.interval.to,
      });
    }
    return t('hours.problem.overlap', {
      first: `${problem.first.from}–${problem.first.to}`,
      second: `${problem.second.from}–${problem.second.to}`,
    });
  }

  function addInterval(day: Weekday) {
    if (!draft) {
      return;
    }
    draft.weekly[day] = [...draft.weekly[day], newInterval(draft.weekly[day])];
  }

  function removeInterval(day: Weekday, index: number) {
    if (!draft) {
      return;
    }
    draft.weekly[day] = draft.weekly[day].filter((_, at) => at !== index);
  }

  function applyToEveryDay(day: Weekday) {
    if (!draft) {
      return;
    }
    draft.weekly = copyDayToAll(draft.weekly, day);
    feedback = { tone: 'ok', message: t('hours.copiedToAll') };
  }

  function cancel() {
    draft = loaded ? cloneWindow(loaded) : null;
    feedback = null;
  }

  async function save(window: ServiceWindow | null) {
    phase = 'saving';
    const outcome = await saveServiceWindow(window);
    phase = 'ready';

    if (outcome.kind === 'ok') {
      loaded = window ? cloneWindow(window) : null;
      draft = window ? cloneWindow(window) : null;
      feedback = { tone: 'ok', message: t('hours.saved') };
      return;
    }
    feedback = {
      tone: 'bad',
      message:
        outcome.kind === 'refused'
          ? t('hours.refused', { message: outcome.message })
          : t('hours.failed'),
    };
  }
</script>

<section class="rounded-xl bg-white p-4 ring-1 ring-ink-100">
  <header>
    <h3 class="text-sm font-medium text-ink-900">{t('hours.title')}</h3>
    <p class="mt-1 text-sm text-ink-500">{t('hours.lead')}</p>
  </header>

  {#if phase === 'loading'}
    <p class="mt-4 text-sm text-ink-500">{t('app.loading')}</p>
  {:else if draft === null}
    <div class="mt-4">
      <p class="text-sm text-ink-500">{t('hours.enableLead')}</p>
      <div class="mt-3">
        <Button onclick={declare}>{t('hours.enable')}</Button>
      </div>
    </div>
  {:else}
    <div class="mt-4">
      <label class="block text-xs font-medium tracking-wide text-ink-500 uppercase">
        {t('hours.timezone')}
        <select
          bind:value={draft.timezone}
          class="mt-1 block w-full max-w-xs rounded-md border border-ink-200 bg-white px-2 py-1.5
                 text-sm font-normal tracking-normal text-ink-900 normal-case"
        >
          {#each zones as zone (zone)}
            <option value={zone}>{zone}</option>
          {/each}
        </select>
      </label>
      {#if draft.timezone === detected}
        <p class="mt-1 text-xs text-ink-500">{t('hours.timezoneDetected')}</p>
      {/if}
    </div>

    <div class="mt-4 divide-y divide-ink-100">
      {#each WEEKDAYS as day (day)}
        {@const dayProblems = problemsOf(day)}
        <!-- A group per day, so a screen reader announces it once rather
             than in every field label. -->
        <fieldset class="py-3">
          <legend class="text-sm font-medium text-ink-900">{t(`weekday.${day}` as const)}</legend>

          {#if draft.weekly[day].length === 0}
            <p class="mt-1 text-sm text-ink-500">{t('hours.closedDay')}</p>
          {:else}
            <ul class="mt-2 space-y-2">
              {#each draft.weekly[day] as interval, index (index)}
                <li class="flex flex-wrap items-center gap-2">
                  <!-- The value is always 24-hour `HH:MM`; only the way the
                       control paints it follows the reader's system, which is
                       why the header's clock toggle does not reach it. -->
                  <input
                    type="time"
                    bind:value={interval.from}
                    aria-label="{t(`weekday.${day}` as const)} — {t('hours.from')}"
                    class="rounded-md border border-ink-200 px-2 py-1 text-sm tabular-nums
                           text-ink-900"
                  />
                  <span aria-hidden="true" class="text-ink-500">–</span>
                  <input
                    type="time"
                    bind:value={interval.to}
                    aria-label="{t(`weekday.${day}` as const)} — {t('hours.to')}"
                    class="rounded-md border border-ink-200 px-2 py-1 text-sm tabular-nums
                           text-ink-900"
                  />
                  <button
                    type="button"
                    onclick={() => removeInterval(day, index)}
                    aria-label={t('hours.removeInterval')}
                    title={t('hours.removeInterval')}
                    class="rounded-md p-1.5 text-ink-500 transition-colors
                           hover:bg-ink-100 hover:text-ink-900"
                  >
                    <X size={16} aria-hidden="true" />
                  </button>
                </li>
              {/each}
            </ul>
          {/if}

          <div class="mt-2 flex flex-wrap gap-x-3 gap-y-1">
            <button
              type="button"
              onclick={() => addInterval(day)}
              class="inline-flex items-center gap-1.5 rounded-md px-2 py-1 text-xs text-mariam-600
                     transition-colors hover:bg-ink-100"
            >
              <Plus size={14} aria-hidden="true" />{t('hours.addInterval')}
            </button>
            <button
              type="button"
              onclick={() => applyToEveryDay(day)}
              class="inline-flex items-center gap-1.5 rounded-md px-2 py-1 text-xs text-ink-500
                     transition-colors hover:bg-ink-100 hover:text-ink-900"
            >
              <CopyPlus size={14} aria-hidden="true" />{t('hours.copyToAll')}
            </button>
          </div>

          {#if dayProblems.length > 0}
            <ul class="mt-2 space-y-1">
              {#each dayProblems as problem, index (index)}
                <li class="text-xs text-density-saturated">{describe(problem)}</li>
              {/each}
            </ul>
          {/if}
        </fieldset>
      {/each}
    </div>

    {#if neverOpen}
      <p class="mt-3 text-sm text-density-medium">{t('hours.neverOpen')}</p>
    {/if}

    {#if feedback}
      <p
        role="status"
        class="mt-3 text-sm {feedback.tone === 'ok' ? 'text-ink-500' : 'text-density-saturated'}"
      >
        {feedback.message}
      </p>
    {/if}

    <div class="mt-4 flex flex-wrap items-center gap-2">
      <Button
        disabled={phase === 'saving' || problems.length > 0 || !dirty}
        onclick={() => save(draft)}
      >
        {phase === 'saving' ? t('hours.saving') : t('hours.save')}
      </Button>
      <button
        type="button"
        onclick={cancel}
        disabled={phase === 'saving' || !dirty}
        class="rounded-md px-4 py-2 text-sm text-ink-500 transition-colors
               hover:bg-ink-100 hover:text-ink-900 disabled:text-ink-300"
      >
        {t('hours.cancel')}
      </button>
      {#if loaded !== null}
        <button
          type="button"
          onclick={() => save(null)}
          disabled={phase === 'saving'}
          title={t('hours.clearConfirm')}
          class="ml-auto rounded-md px-3 py-2 text-sm text-ink-500 transition-colors
                 hover:bg-ink-100 hover:text-density-saturated"
        >
          {t('hours.clear')}
        </button>
      {/if}
    </div>
  {/if}
</section>

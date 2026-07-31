<script lang="ts">
  import { untrack } from 'svelte';

  import {
    DENSITY_CLASSES,
    archiveUrl,
    deleteSession,
    fetchSessions,
    saveClasses,
    startCalibration,
    stopCalibration,
    subscribeLive,
    type ClassMapping,
    type LiveSnapshot,
    type RecordedSession,
    type Status,
  } from '$lib/api';
  import { blankClasses, defaultEnvironment, page, pageCount, PAGE_SIZE } from '$lib/calibration';
  import { formatClock } from '$lib/live';
  import { formattingLocale, hour12 } from '$lib/i18n/i18n.svelte';
  import { t } from '$lib/i18n/i18n.svelte';
  import { nodeLagSeconds } from '$lib/live';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import Download from '@lucide/svelte/icons/download';
  import Trash2 from '@lucide/svelte/icons/trash-2';

  import LabelingScreen from '$components/LabelingScreen.svelte';

  let { status, onupdated }: { status: Status; onupdated: (status: Status) => void } = $props();

  /** A node lagging the stream by more than this is treated as silent. */
  const SILENT_AFTER_US = 10_000_000;

  // Read once: drafts from here on.
  let classes = $state<ClassMapping>({ ...blankClasses(), ...untrack(() => status.classes) });
  // Pre-filled, because inventing a name while a queue forms is the last
  // thing anyone wants to do. Edited freely.
  let environment = $state(
    untrack(() => defaultEnvironment(status.site_name, new Date(), formattingLocale())),
  );
  let positions = $state<Record<string, string>>(
    untrack(() => Object.fromEntries(status.nodes.map((node) => [node.node_id, '']))),
  );

  let savingClasses = $state(false);
  let classesSaved = $state(false);
  let busy = $state(false);
  let failure = $state<string | null>(null);
  let snapshot = $state<LiveSnapshot | null>(null);
  let sessions = $state<RecordedSession[]>([]);
  let pageIndex = $state(0);
  let deleting = $state<string | null>(null);

  /* The live stream is the only thing that says whether frames are still
     arriving, which is what a labeller has to know before spending an hour
     marking a queue beside a sensor that has gone quiet. */
  $effect(() => {
    const close = subscribeLive((next) => (snapshot = next));
    return close;
  });

  /* The start time comes from the appliance, not from whenever this screen
     happened to load: a phone that reloads mid-capture must still show how
     long the capture has been running. */
  const recording = $derived(status.runtime.mode === 'calibrating');
  const startedUs = $derived(status.runtime.mode === 'calibrating' ? status.runtime.started_us : 0);
  const ready = $derived(status.readiness.nodes_paired);
  const nowUs = $derived(snapshot?.now_us ?? 0);
  const frames = $derived(snapshot?.stream.frames ?? 0);

  const silent = $derived.by(() => {
    const stream = snapshot?.stream;
    if (!stream) {
      return false;
    }
    return Object.values(stream.nodes).some(
      (node) => nodeLagSeconds(node.last_frame_us, stream.last_frame_us, SILENT_AFTER_US) !== null,
    );
  });

  const streaming = $derived(snapshot?.stream.running ?? false);
  const shown = $derived(page(sessions, pageIndex));
  const pages = $derived(pageCount(sessions.length));

  function recordedAt(session: RecordedSession): string {
    return session.recorded_at_us === undefined
      ? session.session_id
      : formatClock(session.recorded_at_us, formattingLocale(), hour12());
  }

  function recordedOn(session: RecordedSession): string {
    return session.recorded_at_us === undefined
      ? ''
      : new Date(session.recorded_at_us / 1000).toLocaleDateString(formattingLocale(), {
          day: 'numeric',
          month: 'short',
          year: 'numeric',
        });
  }

  async function remove(sessionId: string) {
    if (await deleteSession(sessionId)) {
      sessions = sessions.filter((session) => session.session_id !== sessionId);
    }
    deleting = null;
  }

  const classesDirty = $derived(
    JSON.stringify(classes) !== JSON.stringify({ ...blankClasses(), ...status.classes }),
  );

  async function describeClasses() {
    savingClasses = true;
    const outcome = await saveClasses(classes);
    savingClasses = false;
    if (outcome.kind === 'ok') {
      classesSaved = true;
      setTimeout(() => (classesSaved = false), 2500);
      onupdated(outcome.status);
    }
  }

  async function start() {
    busy = true;
    const outcome = await startCalibration({ environment, positions });
    busy = false;
    if (outcome.kind === 'ok') {
      failure = null;
      onupdated(outcome.status);
      return;
    }
    failure =
      outcome.kind === 'refused'
        ? t('wizard.refused', { message: outcome.message })
        : t('wizard.failed');
  }

  $effect(() => {
    // Reloaded whenever the capture state changes: stopping seals a session,
    // which is what the list is about.
    void recording;
    void fetchSessions().then((next) => (sessions = next));
  });

  async function stop() {
    busy = true;
    const outcome = await stopCalibration();
    busy = false;
    if (outcome.kind === 'ok') {
      failure = null;
      onupdated(outcome.status);
      return;
    }
    failure =
      outcome.kind === 'refused'
        ? t('wizard.refused', { message: outcome.message })
        : t('wizard.failed');
  }
</script>

{#if recording}
  <LabelingScreen
    classes={status.classes ?? null}
    {startedUs}
    {nowUs}
    {frames}
    {silent}
    stopping={busy}
    onstop={() => void stop()}
  />
{:else}
  <div class="max-w-2xl space-y-8">
    <p class="text-sm text-ink-500">{t('cal.lead')}</p>

    <!-- Three things of different natures, and the page says so: what is
         settled once, what is being started, and what already exists. -->
    <section>
      <h3 class="text-xs font-medium tracking-wide text-ink-500 uppercase">
        {t('cal.newRecording')}
      </h3>
      <div class="mt-2 rounded-xl bg-white p-5 ring-1 ring-ink-100">
        <label class="block text-sm font-medium text-ink-900">
          {t('cal.environment')}
          <input
            type="text"
            bind:value={environment}
            placeholder={t('cal.environmentHint')}
            class="mt-1 block w-full rounded-md border border-ink-200 px-3 py-2 text-base
                   font-normal text-ink-900"
          />
        </label>

        <details class="mt-4 group">
          <summary
            class="flex cursor-pointer items-center gap-1 text-sm text-ink-500 hover:text-ink-900"
          >
            <ChevronDown
              size={14}
              class="transition-transform group-open:rotate-180"
              aria-hidden="true"
            />
            {t('cal.positions')}
          </summary>
          <p class="mt-1 ml-5 text-xs text-ink-500">{t('cal.positionsHint')}</p>
          <div class="mt-2 ml-5 space-y-2">
            {#each status.nodes as node (node.node_id)}
              <label class="block text-sm text-ink-900">
                <span class="font-mono text-xs text-ink-500">{node.node_id}</span>
                <input
                  type="text"
                  bind:value={positions[node.node_id]}
                  class="mt-1 block w-full rounded-md border border-ink-200 px-3 py-2 text-base
                         font-normal text-ink-900"
                />
              </label>
            {/each}
          </div>
        </details>

        {#if failure}
          <p role="status" class="mt-3 text-sm text-density-saturated">{failure}</p>
        {/if}
        {#if !ready}
          <p class="mt-3 text-sm text-ink-500">{t('cal.notReady')}</p>
        {:else if !streaming}
          <p class="mt-3 text-sm text-density-medium">{t('cal.noStream')}</p>
        {/if}

        <button
          type="button"
          onclick={() => void start()}
          disabled={busy || !ready || !streaming || environment.trim().length === 0}
          class="mt-4 w-full rounded-md bg-mariam-600 px-4 py-3 text-sm font-medium text-white
                 transition-colors hover:bg-mariam-700 disabled:bg-ink-200 disabled:text-ink-500
                 sm:w-auto"
        >
          {busy ? t('cal.starting') : t('cal.start')}
        </button>
      </div>
    </section>

    <section>
      <h3 class="text-xs font-medium tracking-wide text-ink-500 uppercase">{t('cal.history')}</h3>
      <div class="mt-2 rounded-xl bg-white ring-1 ring-ink-100">
        {#if sessions.length === 0}
          <p class="p-5 text-sm text-ink-500">{t('cal.historyEmpty')}</p>
        {:else}
          <ul class="divide-y divide-ink-100">
            {#each shown as session (session.session_id)}
              <li class="flex flex-wrap items-center justify-between gap-3 p-4">
                <span class="min-w-0">
                  <span class="block truncate text-sm text-ink-900">
                    {session.environment || session.session_id}
                  </span>
                  <span class="block text-xs text-ink-500">
                    {recordedOn(session)}
                    {recordedAt(session)} ·
                    {t('cal.size', { value: (session.bytes / 1_048_576).toFixed(1) })}
                  </span>
                </span>
                <span class="flex shrink-0 items-center gap-1">
                  {#if session.sealed}
                    <a
                      href={archiveUrl(session.session_id)}
                      download
                      class="inline-flex items-center gap-1.5 rounded-md px-2 py-1 text-xs
                             text-mariam-600 transition-colors hover:bg-ink-100"
                    >
                      <Download size={14} aria-hidden="true" />{t('cal.export')}
                    </a>
                  {:else}
                    <span class="px-2 text-xs text-density-medium">{t('cal.unfinished')}</span>
                  {/if}
                  <button
                    type="button"
                    onclick={() => (deleting = session.session_id)}
                    aria-label={t('cal.delete')}
                    title={t('cal.delete')}
                    class="rounded-md p-1.5 text-ink-500 transition-colors hover:bg-ink-100
                           hover:text-density-saturated"
                  >
                    <Trash2 size={14} aria-hidden="true" />
                  </button>
                </span>
              </li>
            {/each}
          </ul>

          {#if sessions.length > PAGE_SIZE}
            <!-- Paged rather than scrolled: a site records for weeks, and a
                 list that only grows stops being readable long before it
                 stops being useful. -->
            <div class="flex items-center justify-between gap-3 border-t border-ink-100 p-3">
              <button
                type="button"
                onclick={() => (pageIndex = Math.max(0, pageIndex - 1))}
                disabled={pageIndex === 0}
                class="rounded-md px-2 py-1 text-xs text-ink-500 transition-colors
                       hover:bg-ink-100 disabled:text-ink-200"
              >
                {t('cal.previous')}
              </button>
              <span class="text-xs text-ink-500">
                {t('cal.page', { current: pageIndex + 1, total: pages })}
              </span>
              <button
                type="button"
                onclick={() => (pageIndex = Math.min(pages - 1, pageIndex + 1))}
                disabled={pageIndex >= pages - 1}
                class="rounded-md px-2 py-1 text-xs text-ink-500 transition-colors
                       hover:bg-ink-100 disabled:text-ink-200"
              >
                {t('cal.next')}
              </button>
            </div>
          {/if}
        {/if}
      </div>
    </section>

    <!-- Last: settled once per site, and rarely touched again. -->
    <section>
      <h3 class="text-xs font-medium tracking-wide text-ink-500 uppercase">{t('cal.classes')}</h3>
      <div class="mt-2 rounded-xl bg-ink-50 p-5 ring-1 ring-ink-200">
        <p class="text-xs text-ink-500">{t('cal.classesLead')}</p>
        <div class="mt-3 space-y-2">
          {#each DENSITY_CLASSES as density (density)}
            <label class="flex items-center gap-3 text-sm text-ink-900">
              <span class="w-24 shrink-0">{t(`class.${density}` as const)}</span>
              <input
                type="text"
                bind:value={classes[density]}
                class="min-w-0 flex-1 rounded-md border border-ink-200 bg-white px-3 py-2 text-base
                       font-normal text-ink-900"
              />
            </label>
          {/each}
        </div>
        <div class="mt-3 flex items-center gap-3">
          <button
            type="button"
            onclick={() => void describeClasses()}
            disabled={savingClasses || !classesDirty}
            class="rounded-md bg-mariam-600 px-3 py-2 text-sm font-medium text-white
                   transition-colors hover:bg-mariam-700 disabled:bg-ink-200 disabled:text-ink-500"
          >
            {savingClasses ? t('wizard.saving') : t('settings.save')}
          </button>
          {#if classesSaved}
            <span class="text-xs text-ink-500">{t('cal.classesSaved')}</span>
          {/if}
        </div>
      </div>
    </section>
  </div>

  {#if deleting}
    <div
      class="fixed inset-0 z-50 flex items-end justify-center bg-ink-900/60 p-4 sm:items-center"
      role="dialog"
      aria-modal="true"
    >
      <div class="w-full max-w-sm rounded-xl bg-white p-5">
        <h2 class="text-base font-semibold text-ink-900">{t('cal.confirmDelete')}</h2>
        <p class="mt-2 text-sm text-ink-500">{t('cal.confirmDeleteLead')}</p>
        <p class="mt-2 font-mono text-xs text-ink-500">{deleting}</p>
        <div class="mt-5 flex flex-col gap-2">
          <button
            type="button"
            onclick={() => void remove(deleting ?? '')}
            class="rounded-md bg-density-saturated px-4 py-3 text-sm font-medium text-white
                   transition-colors hover:opacity-90"
          >
            {t('cal.delete')}
          </button>
          <button
            type="button"
            onclick={() => (deleting = null)}
            class="rounded-md px-4 py-3 text-sm text-ink-500 transition-colors hover:bg-ink-100"
          >
            {t('hours.cancel')}
          </button>
        </div>
      </div>
    </div>
  {/if}
{/if}

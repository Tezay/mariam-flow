<script lang="ts">
  import ArrowLeft from '@lucide/svelte/icons/arrow-left';
  import CircleAlert from '@lucide/svelte/icons/circle-alert';
  import Download from '@lucide/svelte/icons/download';

  import { type RecordedSession, archiveUrl } from '$lib/api/calibration';
  import { type Portrait, fetchHeatmap, fetchPortrait } from '$lib/api/portrait';
  import { formatDay, formatElapsed } from '$lib/calibration';
  import { concerns, durationSeconds } from '$lib/portrait';
  import { formattingLocale, t } from '$lib/i18n/i18n.svelte';
  import ClassStatistics from '$components/analysis/ClassStatistics.svelte';
  import RecordingTimeline from '$components/analysis/RecordingTimeline.svelte';
  import Button from '$components/ui/Button.svelte';
  import Disclosure from '$components/ui/Disclosure.svelte';
  import StatTile from '$components/ui/StatTile.svelte';

  let {
    session,
    onback,
  }: {
    session: RecordedSession;
    onback: () => void;
  } = $props();

  let portrait = $state<Portrait | null>(null);
  let pixels = $state<Uint8Array | null>(null);
  let waiting = $state(true);
  let feature = $state('motion_energy');

  /** Densest signal first, so the first thing shown is the one that moves. */
  const FEATURES = [
    'motion_energy',
    'amp_std',
    'amp_mean',
    'subcarrier_corr',
    'rssi_mean',
    'rssi_std',
    'frame_rate',
  ];

  const issues = $derived(portrait ? concerns(portrait) : []);

  $effect(() => {
    const id = session.session_id;
    let live = true;
    /* The appliance answers that it has started rather than holding the
       request open, so the screen asks again until it has an answer. */
    const poll = async () => {
      while (live) {
        const outcome = await fetchPortrait(id);
        if (outcome.kind === 'ready') {
          portrait = outcome.portrait;
          pixels = await fetchHeatmap(id);
          waiting = false;
          return;
        }
        if (outcome.kind === 'missing') {
          waiting = false;
          return;
        }
        await new Promise((resume) => setTimeout(resume, 1000));
      }
    };
    void poll();
    return () => {
      live = false;
    };
  });
</script>

<section>
  <Button variant="quiet" size="sm" onclick={onback}>
    <ArrowLeft size={14} aria-hidden="true" />
    {t('recording.back')}
  </Button>

  <header class="mt-3 flex flex-wrap items-start justify-between gap-4">
    <div class="min-w-0">
      <h2 class="text-2xl font-semibold tracking-tight wrap-break-word text-ink-900">
        {session.environment || session.session_id}
      </h2>
      <p class="mt-1 font-mono text-xs wrap-break-word text-ink-500">{session.session_id}</p>
    </div>
    <Button variant="outline" href={archiveUrl(session.session_id)} download>
      <Download size={14} aria-hidden="true" />
      {t('cal.export')}
    </Button>
  </header>

  {#if waiting}
    <p role="status" class="mt-6 rounded-md bg-white p-5 text-sm text-ink-500 ring-1 ring-ink-200">
      {t('recording.describing')}
    </p>
  {:else if !portrait}
    <p class="mt-6 rounded-md bg-ink-100 p-5 text-sm text-ink-500">
      {t('recording.noPortrait')}
    </p>
  {:else}
    <div class="mt-6 grid grid-cols-2 gap-3 lg:grid-cols-4">
      <StatTile label={t('recording.duration')} value={formatElapsed(durationSeconds(portrait))} />
      <StatTile
        label={t('recording.recorded')}
        value={session.recorded_at_us ? formatDay(session.recorded_at_us, formattingLocale()) : '—'}
      />
      <StatTile
        label={t('recording.receivers')}
        value={`${portrait.nodes.filter((node) => node.frames > 0).length}/${portrait.nodes.length}`}
        hint={t('recording.streaming')}
      />
      <StatTile label={t('recording.marks')} value={`${portrait.labels.length}`} />
    </div>

    {#if issues.length > 0}
      <ul class="mt-4 space-y-2">
        {#each issues as issue, index (index)}
          <li class="flex items-start gap-2 rounded-md bg-ink-100 px-4 py-3 text-sm text-ink-900">
            <CircleAlert size={16} class="mt-0.5 shrink-0 text-density-medium" aria-hidden="true" />
            {t(`concern.${issue.key}` as const, {
              node: issue.node ?? '',
              value: issue.value ?? 0,
            })}
          </li>
        {/each}
      </ul>
    {/if}

    <h3 class="mt-8 text-[0.9375rem] leading-6 font-semibold text-ink-900">
      {t('recording.timeline')}
    </h3>
    <p class="mt-0.5 mb-3 max-w-prose text-sm text-ink-500">{t('recording.timelineLead')}</p>

    <div class="mb-3 flex flex-wrap gap-1">
      {#each FEATURES as name (name)}
        <button
          type="button"
          onclick={() => (feature = name)}
          aria-pressed={feature === name}
          class="rounded-md px-2.5 py-1 font-mono text-xs transition-colors
                 {feature === name
            ? 'bg-mariam-600 text-white'
            : 'bg-ink-100 text-ink-700 hover:bg-ink-200'}"
        >
          {name}
        </button>
      {/each}
    </div>

    <div class="rounded-md bg-white p-4 ring-1 ring-ink-200">
      <RecordingTimeline {portrait} {pixels} {feature} />
    </div>

    <div class="mt-4">
      <Disclosure title={t('portrait.perLevel')} open>
        <div class="space-y-6 p-4">
          <p class="max-w-prose text-sm text-ink-500">{t('portrait.perLevelLead')}</p>
          {#each portrait.nodes.filter((node) => node.frames > 0) as node (node.node_id)}
            <div>
              <p class="mb-2 text-sm font-medium text-ink-900">{node.node_id}</p>
              <ClassStatistics {portrait} {node} {feature} />
            </div>
          {/each}
        </div>
      </Disclosure>
    </div>
  {/if}
</section>

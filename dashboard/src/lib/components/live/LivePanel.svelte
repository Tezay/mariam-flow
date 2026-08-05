<script lang="ts">
  import Cpu from '@lucide/svelte/icons/cpu';
  import Radio from '@lucide/svelte/icons/radio';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';

  import {
    type LiveSnapshot,
    type MinuteSummary,
    fetchHistory,
    subscribeLive,
  } from '$lib/api/live';
  import { formattingLocale, hour12, t } from '$lib/i18n/i18n.svelte';
  import { DENSITY_SWATCH } from '$lib/live';
  import { receiverState } from '$lib/sensors';
  import { formatNextChange } from '$lib/schedule';
  import HistoryFigure from '$components/live/HistoryFigure.svelte';

  let { modelName = null }: { modelName?: string | null } = $props();

  /** How far back the short history reaches. */
  const HISTORY_MINUTES = 60;

  let snapshot = $state<LiveSnapshot | null>(null);
  let history = $state<MinuteSummary[]>([]);
  let dropped = $state(false);

  $effect(() => {
    const close = subscribeLive(
      (next) => {
        snapshot = next;
        dropped = false;
      },
      () => (dropped = true),
    );
    return close;
  });

  // The history changes once a minute; polling it at that cadence costs
  // nothing and keeps the live stream carrying only the live state.
  $effect(() => {
    let cancelled = false;
    const load = async () => {
      const rows = await fetchHistory(HISTORY_MINUTES);
      if (!cancelled) {
        history = rows;
      }
    };
    void load();
    const timer = setInterval(() => void load(), 60_000);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  });

  const estimate = $derived(snapshot?.estimate ?? null);
  const stream = $derived(snapshot?.stream ?? null);

  const nodes = $derived(Object.entries(stream?.nodes ?? {}));

  /* Closed is reported, never inferred from a missing estimate: outside
     service hours there is no estimate *and* nothing wrong, which is not
     what "not estimating" means anywhere else on this screen. */
  const closed = $derived(snapshot !== null && !snapshot.service.open);

  /** When the service next changes, phrased, or nothing if it never does. */
  const nextChange = $derived.by(() => {
    const at = snapshot?.service.changes_at_us;
    if (at === undefined || !snapshot) {
      return null;
    }
    return formatNextChange(at, snapshot.now_us, formattingLocale(), hour12());
  });
</script>

<div class="space-y-4" class:opacity-60={dropped}>
  <!-- The hero: one number, the thing the product exists to say. -->
  <section class="rounded-md bg-white p-5 ring-1 ring-ink-100">
    {#if closed}
      <p class="text-xs font-medium uppercase tracking-wide text-ink-500">{t('live.wait')}</p>
      <p class="mt-1 text-4xl font-semibold text-ink-900">{t('live.closed')}</p>
      {#if nextChange}
        <p class="mt-2 text-sm text-ink-900">{t('live.opensAt', { when: nextChange })}</p>
      {/if}
      <p class="mt-2 text-sm text-ink-500">{t('live.closedLead')}</p>
    {:else if estimate}
      <p class="text-xs font-medium uppercase tracking-wide text-ink-500">{t('live.wait')}</p>
      <p class="mt-1 flex items-baseline gap-2">
        <span class="text-5xl font-semibold text-ink-900">{estimate.wait_minutes.toFixed(1)}</span>
        <span class="text-lg text-ink-500">{t('live.minutesShort')}</span>
      </p>
      <p class="mt-2 flex items-center gap-2 text-sm">
        <span class="size-3 rounded-sm {DENSITY_SWATCH[estimate.class]}"></span>
        <span class="text-ink-900">{t(`class.${estimate.class}` as const)}</span>
        <span class="text-ink-500">
          · {t('live.confidence', { value: Math.round(estimate.confidence * 100) })}
        </span>
      </p>

      {#if !estimate.reliable}
        <!-- Shown rather than hidden: this is the administration surface,
             where an operator needs to see what the model produced *and*
             that it is not trustworthy. Masking belongs to the public
             estimate, which does it already. -->
        <p
          class="mt-3 flex items-start gap-2 rounded-md bg-ink-50 p-3 text-sm text-ink-700"
          role="status"
        >
          <TriangleAlert size={16} class="mt-0.5 shrink-0" aria-hidden="true" />
          {t('live.unreliable')}
        </p>
      {/if}

      {#if nextChange}
        <p class="mt-2 text-xs text-ink-500">{t('live.closesAt', { when: nextChange })}</p>
      {/if}
      {#if modelName}
        <!-- Quiet, but present: when an estimate looks wrong, the first
             question is which model produced it. -->
        <p class="mt-3 flex items-center gap-1.5 text-xs text-ink-500">
          <Cpu size={12} aria-hidden="true" />{t('live.model', { name: modelName })}
        </p>
      {/if}
    {:else}
      <p class="text-sm text-ink-500">
        {stream?.running ? t('live.warmingUp') : t('live.notEstimating')}
      </p>
    {/if}
  </section>

  <HistoryFigure minutes={history} />

  <section class="rounded-md bg-white p-4 ring-1 ring-ink-100">
    <h3 class="text-xs font-medium uppercase tracking-wide text-ink-500">{t('live.stream')}</h3>
    {#if nodes.length === 0}
      <p class="mt-2 text-sm text-ink-500">{t('live.noNodes')}</p>
    {:else}
      <ul class="mt-2 divide-y divide-ink-100">
        {#each nodes as [nodeId, node] (nodeId)}
          {@const state = receiverState(node, stream ?? undefined, snapshot?.now_us)}
          <li class="flex items-center justify-between gap-3 py-2">
            <span class="flex items-center gap-2">
              <Radio
                size={16}
                class={state.kind === 'streaming' ? 'text-density-empty' : 'text-density-saturated'}
                aria-hidden="true"
              />
              <span class="font-mono text-sm text-ink-900">{nodeId}</span>
            </span>
            {#if state.kind === 'streaming'}
              <span class="text-sm tabular-nums text-ink-500">
                {t('live.framesPerSecond', { value: state.framesPerSecond.toFixed(0) })}
              </span>
            {:else if state.kind === 'silent'}
              <span class="text-sm text-danger">{t('live.silent', { seconds: state.seconds })}</span
              >
            {:else}
              <span class="text-sm text-density-medium">{t('sensors.neverHeard')}</span>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </section>
</div>

<script lang="ts">
  import { untrack } from 'svelte';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import Circle from '@lucide/svelte/icons/circle';

  import { startCalibration, stopCalibration } from '$lib/api/calibration';
  import { subscribeLive, type LiveSnapshot } from '$lib/api/live';
  import { type Status } from '$lib/api/status';
  import { defaultEnvironment } from '$lib/calibration';
  import { formattingLocale, t } from '$lib/i18n/i18n.svelte';
  import { knownPositions, receiverState } from '$lib/sensors';
  import Button from '$components/ui/Button.svelte';
  import LabelingScreen from '$components/calibration/LabelingScreen.svelte';

  let {
    status,
    lead = null,
    positionsOpen = false,
    onupdated,
    onrecorded,
  }: {
    status: Status;
    /** Shown above the form where the operator meets labelling for the first time. */
    lead?: string | null;
    positionsOpen?: boolean;
    onupdated: (status: Status) => void;
    onrecorded?: () => void;
  } = $props();

  let environment = $state(
    untrack(() => defaultEnvironment(status.site_name, new Date(), formattingLocale())),
  );
  let positions = $state<Record<string, string>>(untrack(() => knownPositions(status.nodes)));
  let snapshot = $state<LiveSnapshot | null>(null);
  let busy = $state(false);
  let stopping = $state(false);
  let failure = $state<string | null>(null);

  /* The live stream is the only thing that says whether frames are still
     arriving, which a labeller has to know before spending an hour marking a
     queue beside a sensor that has gone quiet. */
  $effect(() => subscribeLive((next) => (snapshot = next)));

  const recording = $derived(status.runtime.mode === 'calibrating');
  const startedUs = $derived(status.runtime.mode === 'calibrating' ? status.runtime.started_us : 0);
  const streaming = $derived(snapshot?.stream.running ?? false);
  const silent = $derived.by(() => {
    const stream = snapshot?.stream;
    if (!stream) {
      return false;
    }
    return Object.values(stream.nodes).some(
      (node) => receiverState(node, stream, snapshot?.now_us).kind !== 'streaming',
    );
  });

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

  async function stop() {
    stopping = true;
    const outcome = await stopCalibration();
    stopping = false;
    if (outcome.kind === 'ok') {
      onupdated(outcome.status);
      onrecorded?.();
    }
  }
</script>

{#if recording}
  <LabelingScreen
    classes={status.classes ?? null}
    {startedUs}
    nowUs={snapshot?.now_us ?? 0}
    frames={snapshot?.stream.frames ?? 0}
    {silent}
    {stopping}
    onstop={() => void stop()}
  />
{:else}
  <div class="rounded-md bg-white p-5 ring-1 ring-ink-200">
    {#if lead}
      <p class="mb-4 rounded-md bg-mariam-50 p-3 text-sm text-ink-900">{lead}</p>
    {/if}

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

    <details class="group mt-4" open={positionsOpen}>
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
      <p role="status" class="mt-3 text-sm text-danger">{failure}</p>
    {/if}
    {#if !status.readiness.nodes_paired}
      <p class="mt-3 text-sm text-ink-500">{t('cal.notReady')}</p>
    {:else if !streaming}
      <p class="mt-3 text-sm text-density-medium">{t('cal.noStream')}</p>
    {/if}

    <div class="mt-4">
      <Button
        variant="danger"
        disabled={busy ||
          !status.readiness.nodes_paired ||
          !streaming ||
          environment.trim().length === 0}
        onclick={() => void start()}
      >
        <Circle size={14} class="fill-current" aria-hidden="true" />
        {busy ? t('cal.starting') : t('cal.start')}
      </Button>
    </div>
  </div>
{/if}

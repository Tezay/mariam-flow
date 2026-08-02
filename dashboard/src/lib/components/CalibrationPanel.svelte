<script lang="ts">
  import { untrack } from 'svelte';

  import {
    fetchSessions,
    stopCalibration,
    subscribeLive,
    type LiveSnapshot,
    type RecordedSession,
    type Status,
    type StoredModel,
  } from '$lib/api';
  import { defaultEnvironment } from '$lib/calibration';
  import { formattingLocale, t } from '$lib/i18n/i18n.svelte';
  import { knownPositions, receiverState } from '$lib/sensors';
  import LabelingScreen from '$components/LabelingScreen.svelte';
  import ModelsSection from '$components/ModelsSection.svelte';
  import RecordingsSection from '$components/RecordingsSection.svelte';

  let {
    status,
    models,
    onupdated,
    onlibrarychanged,
  }: {
    status: Status;
    models: StoredModel[];
    onupdated: (status: Status) => void;
    onlibrarychanged: () => void;
  } = $props();

  let environment = $state(
    untrack(() => defaultEnvironment(status.site_name, new Date(), formattingLocale())),
  );
  let positions = $state<Record<string, string>>(untrack(() => knownPositions(status.nodes)));
  let snapshot = $state<LiveSnapshot | null>(null);
  let sessions = $state<RecordedSession[]>([]);
  let stopping = $state(false);

  /* The live stream is the only thing that says whether frames are still
     arriving, which a labeller has to know before spending an hour marking a
     queue beside a sensor that has gone quiet. */
  $effect(() => {
    const close = subscribeLive((next) => (snapshot = next));
    return close;
  });

  const recording = $derived(status.runtime.mode === 'calibrating');
  const startedUs = $derived(status.runtime.mode === 'calibrating' ? status.runtime.started_us : 0);

  $effect(() => {
    void recording;
    void reloadSessions();
  });

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

  async function reloadSessions() {
    sessions = await fetchSessions();
  }

  async function stop() {
    stopping = true;
    const outcome = await stopCalibration();
    stopping = false;
    if (outcome.kind === 'ok') {
      onupdated(outcome.status);
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
  <!-- Stacked in the order they are used, side by side once there is room:
       neither is a step of the other. -->
  <div class="grid gap-10 xl:grid-cols-2 xl:items-start xl:gap-8">
    <ModelsSection {models} {onupdated} onchanged={onlibrarychanged} />
    <RecordingsSection
      nodes={status.nodes}
      {sessions}
      ready={status.readiness.nodes_paired}
      {streaming}
      bind:environment
      bind:positions
      {onupdated}
      onchanged={() => void reloadSessions()}
    />
  </div>
{/if}

<script lang="ts">
  import { type RecordedSession, fetchSessions } from '$lib/api/calibration';
  import { type StoredModel } from '$lib/api/models';
  import { type Status } from '$lib/api/status';
  import CaptureSession from '$components/calibration/CaptureSession.svelte';
  import ModelComparison from '$components/calibration/ModelComparison.svelte';
  import ModelDetail from '$components/calibration/ModelDetail.svelte';
  import ModelsSection from '$components/calibration/ModelsSection.svelte';
  import RecordingDetail from '$components/calibration/RecordingDetail.svelte';
  import RecordingsSection from '$components/calibration/RecordingsSection.svelte';

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

  let sessions = $state<RecordedSession[]>([]);
  /* What is being read, held here rather than in either list: a detail
     replaces the whole surface, and the lists are what it returns to. */
  let inspecting = $state<string | null>(null);
  let reading = $state<string | null>(null);
  let comparing = $state<{ reference: string; candidate: string } | null>(null);

  const recording = $derived(status.runtime.mode === 'calibrating');
  const inService = $derived(models.find((model) => model.active) ?? null);
  const opened = $derived(models.find((model) => model.id === inspecting) ?? null);
  const read = $derived(sessions.find((session) => session.session_id === reading) ?? null);

  /* Resolved against the library on every render rather than captured when the
     comparison opened: activating one of the two from here changes both. */
  const pair = $derived.by(() => {
    const chosen = comparing;
    if (!chosen) {
      return null;
    }
    const reference = models.find((model) => model.id === chosen.reference);
    const candidate = models.find((model) => model.id === chosen.candidate);
    return reference && candidate ? { reference, candidate } : null;
  });

  function compare(reference: string, candidate: string) {
    inspecting = candidate;
    comparing = { reference, candidate };
  }

  $effect(() => {
    void recording;
    void reloadSessions();
  });

  async function reloadSessions() {
    sessions = await fetchSessions();
  }
</script>

{#if pair}
  <ModelComparison
    reference={pair.reference}
    candidate={pair.candidate}
    onback={() => (comparing = null)}
    {onupdated}
    onchanged={onlibrarychanged}
  />
{:else if opened}
  <ModelDetail
    model={opened}
    recordings={sessions}
    {inService}
    onback={() => (inspecting = null)}
    {onupdated}
    onchanged={onlibrarychanged}
    oncompare={compare}
  />
{:else if read}
  <RecordingDetail session={read} onback={() => (reading = null)} />
{:else}
  <CaptureSession {status} {onupdated} onrecorded={() => void reloadSessions()} />

  {#if !recording}
    <!-- Stacked in the order they are used, side by side once there is room:
         neither is a step of the other. -->
    <div class="mt-10 grid gap-10 xl:grid-cols-2 xl:items-start xl:gap-8">
      <ModelsSection
        {models}
        {onupdated}
        onchanged={onlibrarychanged}
        onopen={(model) => (inspecting = model.id)}
        oncompare={compare}
      />
      <RecordingsSection
        {sessions}
        onchanged={() => void reloadSessions()}
        onopen={(session) => (reading = session.session_id)}
      />
    </div>
  {/if}
{/if}

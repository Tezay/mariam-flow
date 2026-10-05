<script lang="ts">
  import { type RecordedSession, fetchSessions } from '$lib/api/calibration';
  import { type StoredModel } from '$lib/api/models';
  import { type Status } from '$lib/api/status';
  import CaptureSession from '$components/calibration/CaptureSession.svelte';
  import ImportModel from '$components/calibration/ImportModel.svelte';
  import ModelComparison from '$components/calibration/ModelComparison.svelte';
  import ModelDetail from '$components/calibration/ModelDetail.svelte';
  import ModelInService from '$components/calibration/ModelInService.svelte';
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
  /* Held as identifiers rather than models: the import replaces the library,
     and a copy captured before it would still claim to be active. */
  let imported = $state<{ over: string; as: string } | null>(null);

  const recording = $derived(status.runtime.mode === 'calibrating');
  const inService = $derived(models.find((model) => model.active) ?? null);
  const replaced = $derived.by(() => {
    const last = imported;
    if (!last || inService?.id !== last.as || !inService.has_evaluation) {
      return null;
    }
    const previous = models.find((model) => model.id === last.over);
    return previous?.has_evaluation ? previous : null;
  });
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

  function accept(next: Status) {
    imported =
      inService && next.active_model ? { over: inService.id, as: next.active_model } : null;
    onupdated(next);
    onlibrarychanged();
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
  <ModelInService
    model={inService}
    {replaced}
    onopen={(model) => (inspecting = model.id)}
    oncompare={compare}
    onchanged={onlibrarychanged}
  />

  <!-- In the order they are read on a phone, where they stack. -->
  <div class="mt-10 grid gap-10 xl:grid-cols-2 xl:items-start xl:gap-8">
    <RecordingsSection
      {sessions}
      onchanged={() => void reloadSessions()}
      onopen={(session) => (reading = session.session_id)}
    >
      {#snippet actions()}
        <CaptureSession {status} {onupdated} onrecorded={() => void reloadSessions()} launcher />
        <ImportModel onimported={accept} />
      {/snippet}
    </RecordingsSection>
    <ModelsSection
      {models}
      {onupdated}
      onchanged={onlibrarychanged}
      onopen={(model) => (inspecting = model.id)}
    />
  </div>
{/if}

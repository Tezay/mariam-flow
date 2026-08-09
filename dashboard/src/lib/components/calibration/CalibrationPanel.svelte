<script lang="ts">
  import { type RecordedSession, fetchSessions } from '$lib/api/calibration';
  import { type StoredModel } from '$lib/api/models';
  import { type Status } from '$lib/api/status';
  import CaptureSession from '$components/calibration/CaptureSession.svelte';
  import ModelDetail from '$components/calibration/ModelDetail.svelte';
  import ModelsSection from '$components/calibration/ModelsSection.svelte';
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
  /* Which model is being read, held here rather than in the list: the detail
     replaces the whole surface, and the list is what it returns to. */
  let inspecting = $state<string | null>(null);

  const recording = $derived(status.runtime.mode === 'calibrating');
  const opened = $derived(models.find((model) => model.id === inspecting) ?? null);

  $effect(() => {
    void recording;
    void reloadSessions();
  });

  async function reloadSessions() {
    sessions = await fetchSessions();
  }
</script>

{#if opened}
  <ModelDetail
    model={opened}
    recordings={sessions}
    onback={() => (inspecting = null)}
    {onupdated}
    onchanged={onlibrarychanged}
  />
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
      />
      <RecordingsSection {sessions} onchanged={() => void reloadSessions()} />
    </div>
  {/if}
{/if}

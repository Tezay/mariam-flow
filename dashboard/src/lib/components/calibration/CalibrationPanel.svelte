<script lang="ts">
  import { type RecordedSession, fetchSessions } from '$lib/api/calibration';
  import { type StoredModel } from '$lib/api/models';
  import { type Status } from '$lib/api/status';
  import CaptureSession from '$components/calibration/CaptureSession.svelte';
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

  const recording = $derived(status.runtime.mode === 'calibrating');

  $effect(() => {
    void recording;
    void reloadSessions();
  });

  async function reloadSessions() {
    sessions = await fetchSessions();
  }
</script>

<CaptureSession {status} {onupdated} onrecorded={() => void reloadSessions()} />

{#if !recording}
  <!-- Stacked in the order they are used, side by side once there is room:
       neither is a step of the other. -->
  <div class="mt-10 grid gap-10 xl:grid-cols-2 xl:items-start xl:gap-8">
    <ModelsSection {models} {onupdated} onchanged={onlibrarychanged} />
    <RecordingsSection {sessions} onchanged={() => void reloadSessions()} />
  </div>
{/if}

<script lang="ts">
  import Plus from '@lucide/svelte/icons/plus';

  import { stopCalibration } from '$lib/api/calibration';
  import { subscribeLive, type LiveSnapshot } from '$lib/api/live';
  import { type Status } from '$lib/api/status';
  import { t } from '$lib/i18n/i18n.svelte';
  import { quietReceivers } from '$lib/sensors';
  import Button from '$components/ui/Button.svelte';
  import Modal from '$components/ui/Modal.svelte';
  import CaptureForm from '$components/calibration/CaptureForm.svelte';
  import LabelingScreen from '$components/calibration/LabelingScreen.svelte';

  let {
    status,
    lead = null,
    launcher = false,
    onupdated,
    onrecorded,
  }: {
    status: Status;
    /** Shown above the form where the operator meets labelling for the first time. */
    lead?: string | null;
    /** A button opening the form, where recording is one action among others. */
    launcher?: boolean;
    onupdated: (status: Status) => void;
    onrecorded?: () => void;
  } = $props();

  let snapshot = $state<LiveSnapshot | null>(null);
  let open = $state(false);
  let stopping = $state(false);

  /* The live stream is the only thing that says whether frames are still
     arriving, which a labeller has to know before spending an hour marking a
     queue beside a sensor that has gone quiet. */
  $effect(() => subscribeLive((next) => (snapshot = next)));

  const recording = $derived(status.runtime.mode === 'calibrating');
  const startedUs = $derived(status.runtime.mode === 'calibrating' ? status.runtime.started_us : 0);
  const silent = $derived(
    snapshot !== null && quietReceivers(status.nodes, snapshot.stream, snapshot.now_us).length > 0,
  );

  function started(next: Status) {
    open = false;
    onupdated(next);
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
{:else if launcher}
  <Button onclick={() => (open = true)}>
    <Plus size={14} aria-hidden="true" />{t('cal.new')}
  </Button>
  {#if open}
    <Modal title={t('cal.new')} wide oncancel={() => (open = false)}>
      <CaptureForm {status} {snapshot} dialog onstarted={started} />
    </Modal>
  {/if}
{:else}
  <div class="rounded-md bg-white p-5 ring-1 ring-ink-200">
    {#if lead}
      <p class="mb-4 rounded-md bg-mariam-50 p-3 text-sm text-ink-900">{lead}</p>
    {/if}
    <CaptureForm {status} {snapshot} onstarted={onupdated} />
  </div>
{/if}

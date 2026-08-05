<script lang="ts">
  import { type Status } from '$lib/api/status';
  import { t } from '$lib/i18n/i18n.svelte';
  import CaptureSession from '$components/calibration/CaptureSession.svelte';

  let { status, onupdated }: { status: Status; onupdated: (status: Status) => void } = $props();
</script>

<CaptureSession {status} lead={t('cal.wizardLead')} positionsOpen {onupdated} />

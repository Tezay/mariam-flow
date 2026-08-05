<script lang="ts">
  import { type Status } from '$lib/api/status';
  import { t } from '$lib/i18n/i18n.svelte';
  import QueueSection from '$components/settings/QueueSection.svelte';

  let { status, onupdated }: { status: Status; onupdated: (status: Status) => void } = $props();
</script>

<QueueSection {status} lead={t('queue.wizardLead')} submitLabel={t('site.submit')} {onupdated} />

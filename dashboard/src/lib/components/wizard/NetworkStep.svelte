<script lang="ts">
  import { untrack } from 'svelte';

  import type { NetworkSurvey, Status, Uplink } from '$lib/api';
  import { t } from '$lib/i18n/i18n.svelte';
  import { requiresAdministrator, verdict } from '$lib/network';
  import NetworkHandout from '$components/network/NetworkHandout.svelte';
  import NetworkInterview from '$components/network/NetworkInterview.svelte';
  import UplinkForm from '$components/network/UplinkForm.svelte';

  let {
    status,
    initialMode,
    initialSsid,
    onupdated,
  }: {
    status: Status;
    initialMode: Uplink['mode'];
    initialSsid: string | null;
    onupdated: (status: Status) => void;
  } = $props();

  const BLANK: NetworkSurvey = {
    authentication: 'unknown',
    registration_required: false,
    fixed_address: false,
  };

  /* Seeded from what the appliance already holds: an installer who answered
     these, stepped back and returned must not be asked again — and must not
     be shown blanks while the appliance holds their answers. */
  let survey = $state<NetworkSurvey>({ ...BLANK, ...untrack(() => status.survey) });

  const offline = $derived(status.uplink.mode === 'offline');
  const answered = $derived(verdict(survey));
  const needed = $derived(requiresAdministrator(survey, offline));
</script>

<div class="space-y-6">
  <p class="text-sm text-ink-500">{t('net.lead')}</p>

  <!-- The interview records itself as it is answered. No callback here: the
       step is finished by the uplink decision below, not by a survey. -->
  <NetworkInterview bind:survey />

  <div class="border-t border-ink-100 pt-6">
    <UplinkForm
      {initialMode}
      {initialSsid}
      verdict={answered}
      submitLabel={t('net.submit')}
      {onupdated}
    />
  </div>

  {#if needed}
    <NetworkHandout {status} {survey} {offline} />
  {/if}
</div>

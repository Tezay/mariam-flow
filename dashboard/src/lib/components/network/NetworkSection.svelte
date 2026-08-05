<script lang="ts">
  import { untrack } from 'svelte';

  import { type NetworkSurvey, type Status } from '$lib/api/status';
  import { t } from '$lib/i18n/i18n.svelte';
  import { requiresAdministrator, verdict } from '$lib/network';
  import NetworkHandout from '$components/network/NetworkHandout.svelte';
  import NetworkInterview from '$components/network/NetworkInterview.svelte';
  import UplinkForm from '$components/network/UplinkForm.svelte';

  let { status, onupdated }: { status: Status; onupdated: (status: Status) => void } = $props();

  const BLANK: NetworkSurvey = {
    authentication: 'unknown',
    registration_required: false,
    fixed_address: false,
  };

  // Read once: this is a draft from here on, and it records itself.
  let survey = $state<NetworkSurvey>({ ...BLANK, ...untrack(() => status.survey) });

  const offline = $derived(status.uplink.mode === 'offline');
  const answered = $derived(verdict(survey));
  const needed = $derived(requiresAdministrator(survey, offline));
</script>

<div class="max-w-xl space-y-6">
  <p class="text-sm text-ink-900">
    {t('status.uplink')} ·
    <span class="text-ink-500">
      {t(`uplink.${status.uplink.mode}` as const)}{status.uplink.ssid
        ? ` · ${status.uplink.ssid}`
        : ''}
    </span>
  </p>

  <NetworkInterview bind:survey onsaved={onupdated} />

  <div class="border-t border-ink-100 pt-6">
    <UplinkForm
      initialMode={status.uplink.mode}
      initialSsid={status.uplink.ssid ?? null}
      verdict={answered}
      submitLabel={t('settings.save')}
      {onupdated}
    />
  </div>

  {#if needed}
    <NetworkHandout {status} {survey} {offline} />
  {/if}
</div>

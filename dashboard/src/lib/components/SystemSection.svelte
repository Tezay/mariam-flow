<script lang="ts">
  import type { Status } from '$lib/api';
  import { t } from '$lib/i18n/i18n.svelte';

  let { status }: { status: Status } = $props();

  const runtimeLabel = $derived(
    status.runtime.mode === 'calibrating'
      ? t('runtime.calibrating')
      : status.runtime.mode === 'live'
        ? t('runtime.live')
        : t('runtime.idle'),
  );
</script>

<dl class="divide-y divide-ink-100">
  <div class="flex items-baseline justify-between gap-3 pb-2">
    <dt class="text-sm text-ink-500">{t('status.activity')}</dt>
    <dd class="text-sm text-ink-900">{runtimeLabel}</dd>
  </div>
  <div class="flex items-baseline justify-between gap-3 py-2">
    <dt class="text-sm text-ink-500">{t('status.model')}</dt>
    <dd class="text-sm text-ink-900">
      {status.model_installed ? t('status.modelInstalled') : t('status.modelMissing')}
    </dd>
  </div>
  <div class="flex items-baseline justify-between gap-3 pt-2">
    <dt class="text-sm text-ink-500">{t('status.sensorAp')}</dt>
    <dd class="font-mono text-sm text-ink-900">
      {status.sensor_ap.ssid}
      <span class="text-ink-500"
        >({t('status.channel', { channel: status.sensor_ap.channel })})</span
      >
    </dd>
  </div>
</dl>

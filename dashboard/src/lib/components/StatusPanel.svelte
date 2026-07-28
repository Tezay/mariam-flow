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

  const uplinkLabel = $derived.by(() => {
    const label = t(`uplink.${status.uplink.mode}` as const);
    return status.uplink.ssid ? `${label} · ${status.uplink.ssid}` : label;
  });
</script>

<section class="space-y-4">
  <dl class="grid grid-cols-2 gap-3">
    <div class="rounded-lg bg-white p-4">
      <dt class="text-xs font-medium uppercase tracking-wide text-ink-500">{t('status.site')}</dt>
      <dd class="mt-1 truncate text-base font-medium text-ink-900">
        {status.site_name ?? t('status.siteUnnamed')}
      </dd>
    </div>
    <div class="rounded-lg bg-white p-4">
      <dt class="text-xs font-medium uppercase tracking-wide text-ink-500">{t('status.kit')}</dt>
      <dd class="mt-1 truncate font-mono text-base text-ink-900">{status.kit_id}</dd>
    </div>
    <div class="rounded-lg bg-white p-4">
      <dt class="text-xs font-medium uppercase tracking-wide text-ink-500">
        {t('status.activity')}
      </dt>
      <dd class="mt-1 text-base text-ink-900">{runtimeLabel}</dd>
    </div>
    <div class="rounded-lg bg-white p-4">
      <dt class="text-xs font-medium uppercase tracking-wide text-ink-500">{t('status.model')}</dt>
      <dd class="mt-1 text-base text-ink-900">
        {status.model_installed ? t('status.modelInstalled') : t('status.modelMissing')}
      </dd>
    </div>
  </dl>

  <div class="rounded-lg bg-white p-4">
    <h3 class="text-xs font-medium uppercase tracking-wide text-ink-500">{t('status.network')}</h3>
    <p class="mt-2 text-sm text-ink-900">
      {t('status.sensorAp')} · <span class="font-mono">{status.sensor_ap.ssid}</span>
      <span class="text-ink-500"
        >({t('status.channel', { channel: status.sensor_ap.channel })})</span
      >
    </p>
    <p class="mt-1 text-sm text-ink-900">{t('status.uplink')} · {uplinkLabel}</p>
  </div>

  <div class="rounded-lg bg-white p-4">
    <h3 class="text-xs font-medium uppercase tracking-wide text-ink-500">{t('nodes.title')}</h3>
    {#if status.nodes.length === 0}
      <p class="mt-2 text-sm text-ink-500">{t('nodes.none')}</p>
    {:else}
      <ul class="mt-2 divide-y divide-ink-100">
        {#each status.nodes as node (node.node_id)}
          <li class="flex items-baseline justify-between gap-3 py-2">
            <span class="font-mono text-sm text-ink-900">{node.node_id}</span>
            <span class="text-sm text-ink-500">{t(`nodes.role.${node.role}` as const)}</span>
            <span class="font-mono text-xs text-ink-500">{node.address ?? node.mac}</span>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</section>

<script lang="ts">
  import { fetchSystem, type Status, type SystemReport } from '$lib/api';
  import { t } from '$lib/i18n/i18n.svelte';
  import { asMegabytes, formatUptime } from '$lib/system';

  let { status }: { status: Status } = $props();

  /** Refreshed while the section is open: uptime and heat are the point. */
  const REFRESH_MS = 10_000;

  let report = $state<SystemReport | null>(null);

  $effect(() => {
    let cancelled = false;
    const load = async () => {
      const next = await fetchSystem();
      if (!cancelled) {
        report = next;
      }
    };
    void load();
    const timer = setInterval(() => void load(), REFRESH_MS);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  });

  const runtimeLabel = $derived(
    status.runtime.mode === 'calibrating'
      ? t('runtime.calibrating')
      : status.runtime.mode === 'live'
        ? t('runtime.live')
        : t('runtime.idle'),
  );

  const memory = $derived.by(() => {
    const total = report?.memory_total_kb;
    const available = report?.memory_available_kb;
    return total && available
      ? t('system.memoryValue', {
          available: asMegabytes(available),
          total: asMegabytes(total),
        })
      : null;
  });

  /* Every machine fact is optional: the same binary is developed on a laptop
     that reports none of them, so the screen says so rather than showing a
     blank where a value belongs. */
  const machine = $derived([
    { label: t('system.model'), value: report?.model ?? null, mono: false },
    { label: t('system.os'), value: report?.os ?? null, mono: false },
    { label: t('system.kernel'), value: report?.kernel ?? null, mono: true },
    {
      label: t('system.uptime'),
      value: report?.uptime_s === undefined ? null : formatUptime(report.uptime_s),
      mono: true,
    },
    {
      label: t('system.load'),
      value: report?.load_1m === undefined ? null : report.load_1m.toFixed(2),
      mono: true,
    },
    { label: t('system.memory'), value: memory, mono: true },
    {
      label: t('system.temperature'),
      value: report?.temperature_c === undefined ? null : `${report.temperature_c.toFixed(1)} °C`,
      mono: true,
    },
  ]);

  const anyReported = $derived(machine.some((row) => row.value !== null));
</script>

<dl class="divide-y divide-ink-100">
  <div class="flex flex-wrap items-baseline justify-between gap-3 pb-2">
    <dt class="text-sm text-ink-500">{t('status.activity')}</dt>
    <dd class="text-sm text-ink-900">{runtimeLabel}</dd>
  </div>
  <div class="flex flex-wrap items-baseline justify-between gap-3 py-2">
    <dt class="text-sm text-ink-500">{t('status.model')}</dt>
    <dd class="text-sm text-ink-900">
      {status.model_installed ? t('status.modelInstalled') : t('status.modelMissing')}
    </dd>
  </div>
  <div class="flex flex-wrap items-baseline justify-between gap-3 py-2">
    <dt class="text-sm text-ink-500">{t('status.sensorAp')}</dt>
    <dd class="font-mono text-sm text-ink-900">
      {status.sensor_ap.ssid}
      <span class="text-ink-500">
        ({t('status.channel', { channel: status.sensor_ap.channel })})
      </span>
    </dd>
  </div>

  {#each machine as row (row.label)}
    <div class="flex flex-wrap items-baseline justify-between gap-3 py-2">
      <dt class="text-sm text-ink-500">{row.label}</dt>
      <dd
        class="text-sm {row.mono ? 'font-mono' : ''} {row.value ? 'text-ink-900' : 'text-ink-300'}"
      >
        {row.value ?? t('system.unknown')}
      </dd>
    </div>
  {/each}
</dl>

{#if !anyReported}
  <p class="mt-3 text-xs text-ink-500">{t('system.unknownLead')}</p>
{/if}

<script lang="ts">
  import Activity from '@lucide/svelte/icons/activity';
  import LogOut from '@lucide/svelte/icons/log-out';
  import Radio from '@lucide/svelte/icons/radio';
  import Settings from '@lucide/svelte/icons/settings';
  import Target from '@lucide/svelte/icons/target';

  import type { Status } from '$lib/api';
  import { hour12, locale, t, toggleHourCycle, toggleLocale } from '$lib/i18n/i18n.svelte';
  import CalibrationPanel from '$components/CalibrationPanel.svelte';
  import LivePanel from '$components/LivePanel.svelte';
  import SettingsShell from '$components/SettingsShell.svelte';

  let {
    status,
    onsignout,
    onupdated,
  }: { status: Status; onsignout: () => void; onupdated: (status: Status) => void } = $props();

  type Tab = 'live' | 'nodes' | 'calibration' | 'settings';
  let active = $state<Tab>('live');

  const tabs = [
    { id: 'live' as const, icon: Activity },
    { id: 'nodes' as const, icon: Radio },
    { id: 'calibration' as const, icon: Target },
    { id: 'settings' as const, icon: Settings },
  ];
</script>

<!-- Tabs sit at the bottom on a phone, where a thumb reaches them, and
     become a side rail once there is room. One layout, two shapes. -->
<div class="flex min-h-dvh flex-col bg-ink-50 sm:flex-row">
  <nav
    aria-label={t('app.name')}
    class="order-2 flex shrink-0 border-t border-ink-200 bg-white
           sm:order-1 sm:w-24 sm:flex-col sm:border-t-0 sm:border-r"
  >
    {#each tabs as tab (tab.id)}
      {@const Icon = tab.icon}
      <button
        type="button"
        aria-current={active === tab.id ? 'page' : undefined}
        onclick={() => (active = tab.id)}
        class="flex flex-1 flex-col items-center gap-1 px-2 py-3 text-xs transition-colors
               sm:flex-none {active === tab.id
          ? 'text-mariam-600'
          : 'text-ink-500 hover:text-ink-900'}"
      >
        <Icon size={20} aria-hidden="true" />
        {t(`tab.${tab.id}` as const)}
      </button>
    {/each}
  </nav>

  <div class="order-1 flex min-w-0 flex-1 flex-col sm:order-2">
    <header class="flex items-center justify-between gap-3 px-4 py-3">
      <h1 class="truncate text-base font-semibold text-mariam-600">
        {status.site_name ?? t('app.name')}
      </h1>
      <div class="flex shrink-0 items-center gap-1">
        <button
          type="button"
          onclick={toggleLocale}
          title={t('header.language')}
          aria-label={t('header.language')}
          class="rounded-md px-2 py-1.5 text-xs font-medium text-ink-500 uppercase
                 transition-colors hover:bg-ink-100 hover:text-ink-900"
        >
          {locale() === 'fr' ? 'EN' : 'FR'}
        </button>
        <button
          type="button"
          onclick={toggleHourCycle}
          title={hour12() ? t('header.clock24') : t('header.clock')}
          aria-label={hour12() ? t('header.clock24') : t('header.clock')}
          class="rounded-md px-2 py-1.5 text-xs font-medium text-ink-500
                 transition-colors hover:bg-ink-100 hover:text-ink-900"
        >
          {hour12() ? '24h' : '12h'}
        </button>
        <button
          type="button"
          onclick={onsignout}
          aria-label={t('shell.signOut')}
          class="rounded-md p-2 text-ink-500 transition-colors hover:bg-ink-100 hover:text-ink-900"
        >
          <LogOut size={18} aria-hidden="true" />
        </button>
      </div>
    </header>

    <main class="flex-1 px-4 pb-6">
      {#if active === 'live'}
        <LivePanel />
      {:else if active === 'calibration'}
        <CalibrationPanel {status} {onupdated} />
      {:else if active === 'settings'}
        <SettingsShell {status} {onupdated} />
      {:else}
        <p class="rounded-lg bg-white p-4 text-sm text-ink-500">{t('wizard.comingNext')}</p>
      {/if}
    </main>
  </div>
</div>

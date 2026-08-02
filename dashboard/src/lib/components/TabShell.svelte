<script lang="ts">
  import Activity from '@lucide/svelte/icons/activity';
  import LogOut from '@lucide/svelte/icons/log-out';
  import Radio from '@lucide/svelte/icons/radio';
  import Settings from '@lucide/svelte/icons/settings';
  import Target from '@lucide/svelte/icons/target';

  import { fetchModels, type Status, type StoredModel } from '$lib/api';
  import { modelName } from '$lib/calibration';
  import { hour12, locale, t, toggleHourCycle, toggleLocale } from '$lib/i18n/i18n.svelte';
  import CalibrationPanel from '$components/CalibrationPanel.svelte';
  import LivePanel from '$components/LivePanel.svelte';
  import SensorsPanel from '$components/SensorsPanel.svelte';
  import SettingsShell from '$components/SettingsShell.svelte';

  let {
    status,
    onsignout,
    onupdated,
  }: { status: Status; onsignout: () => void; onupdated: (status: Status) => void } = $props();

  /* The library is held here rather than fetched by each screen that shows
     it: renaming a model on one tab must not leave another naming it the old
     way, and there is only one list to reload when it changes. */
  let models = $state<StoredModel[]>([]);

  $effect(() => {
    void status.active_model;
    void reloadModels();
  });

  async function reloadModels() {
    models = await fetchModels();
  }

  const activeModel = $derived(models.find((model) => model.active) ?? null);

  type Tab = 'live' | 'nodes' | 'calibration' | 'settings';
  let active = $state<Tab>('live');
  let surface = $state<HTMLElement | null>(null);

  /* Tabs are reachable while the content is scrolled, which is the whole
     point of pinning them — so a tab has to open at its own top rather than
     wherever the previous one had been left. */
  $effect(() => {
    void active;
    surface?.scrollTo({ top: 0 });
  });

  const tabs = [
    { id: 'live' as const, icon: Activity },
    { id: 'nodes' as const, icon: Radio },
    { id: 'calibration' as const, icon: Target },
    { id: 'settings' as const, icon: Settings },
  ];
</script>

<!-- Tabs sit at the bottom on a phone, where a thumb reaches them, and
     become a side rail once there is room. One layout, two shapes.

     The shell is sized to the viewport and only the content beneath the tabs
     scrolls, which is what keeps them in place without a fixed position and a
     padding that would have to agree with their height from somewhere else. -->
<div class="app-shell flex h-dvh flex-col bg-ink-50 sm:flex-row">
  <nav
    aria-label={t('app.name')}
    class="order-2 flex shrink-0 border-t border-ink-200 bg-white
           pb-[env(safe-area-inset-bottom)] sm:order-1 sm:w-24 sm:flex-col sm:overflow-y-auto
           sm:border-t-0 sm:border-r sm:pb-0"
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

  <div
    bind:this={surface}
    class="app-scroll order-1 min-h-0 min-w-0 flex-1 overflow-y-auto sm:order-2"
  >
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

    <main class="px-4 pb-6">
      {#if active === 'live'}
        <LivePanel modelName={activeModel ? modelName(activeModel) : null} />
      {:else if active === 'calibration'}
        <CalibrationPanel
          {status}
          {models}
          {onupdated}
          onlibrarychanged={() => void reloadModels()}
        />
      {:else if active === 'nodes'}
        <SensorsPanel {status} {onupdated} />
      {:else}
        <SettingsShell {status} {onupdated} />
      {/if}
    </main>
  </div>
</div>

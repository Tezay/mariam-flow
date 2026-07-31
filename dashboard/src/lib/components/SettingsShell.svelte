<script lang="ts">
  import ChevronLeft from '@lucide/svelte/icons/chevron-left';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';

  import type { Status } from '$lib/api';
  import { t } from '$lib/i18n/i18n.svelte';
  import NetworkSection from '$components/NetworkSection.svelte';
  import NodesSection from '$components/NodesSection.svelte';
  import ServiceHoursPanel from '$components/ServiceHoursPanel.svelte';
  import SiteSection from '$components/SiteSection.svelte';
  import SystemSection from '$components/SystemSection.svelte';

  let { status, onupdated }: { status: Status; onupdated: (status: Status) => void } = $props();

  const SECTIONS = ['site', 'nodes', 'network', 'hours', 'system'] as const;
  type Section = (typeof SECTIONS)[number];

  /* Nothing is selected to begin with, which is what makes one component
     serve both shapes: a phone shows the list until a section is chosen, a
     wide screen falls back to the first one because it has room for both. */
  let chosen = $state<Section | null>(null);
  const section = $derived(chosen ?? 'site');
</script>

<div class="lg:grid lg:grid-cols-[14rem_minmax(0,1fr)] lg:gap-8">
  <!-- The list is the whole screen on a phone until something is picked, and
       a permanent rail once there is room for it beside the detail. -->
  <nav class:hidden={chosen !== null} class="lg:block!" aria-label={t('tab.settings')}>
    <ul
      class="divide-y divide-ink-100 overflow-hidden rounded-lg bg-white lg:divide-y-0
               lg:bg-transparent lg:space-y-1"
    >
      {#each SECTIONS as name (name)}
        <li>
          <button
            type="button"
            onclick={() => (chosen = name)}
            aria-current={chosen === name ? 'true' : undefined}
            class="flex w-full items-center justify-between gap-2 px-4 py-3 text-left text-sm
                   transition-colors lg:rounded-md lg:py-2 {section === name
              ? 'lg:bg-white lg:font-medium lg:text-ink-900'
              : 'text-ink-900 hover:bg-ink-100 lg:text-ink-500'}"
          >
            {t(`settings.${name}` as const)}
            <ChevronRight size={16} class="text-ink-300 lg:hidden" aria-hidden="true" />
          </button>
        </li>
      {/each}
    </ul>
  </nav>

  <section class:hidden={chosen === null} class="lg:block!">
    <button
      type="button"
      onclick={() => (chosen = null)}
      class="mb-3 inline-flex items-center gap-1 text-xs text-ink-500 transition-colors
             hover:text-ink-900 lg:hidden"
    >
      <ChevronLeft size={14} aria-hidden="true" />{t('settings.back')}
    </button>

    <div class="rounded-lg bg-white p-4">
      <h2 class="text-sm font-medium text-ink-900">{t(`settings.${section}` as const)}</h2>
      <div class="mt-4">
        {#if section === 'site'}
          <SiteSection {status} {onupdated} />
        {:else if section === 'nodes'}
          <NodesSection {status} />
        {:else if section === 'network'}
          <NetworkSection {status} {onupdated} />
        {:else if section === 'hours'}
          <ServiceHoursPanel />
        {:else}
          <SystemSection {status} />
        {/if}
      </div>
    </div>
  </section>
</div>

<script lang="ts">
  import type { Snippet } from 'svelte';

  import { t } from '$lib/i18n/i18n.svelte';

  let {
    title,
    lead,
    oncancel,
    children,
  }: { title: string; lead?: string; oncancel: () => void; children: Snippet } = $props();
</script>

<!-- Bottom sheet on a phone, centred on a wide screen: the same component,
     placed where the hand that dismisses it already is. -->
<div
  class="fixed inset-0 z-50 flex items-end justify-center bg-ink-900/60 p-4 sm:items-center"
  role="dialog"
  aria-modal="true"
  aria-label={title}
>
  <div class="w-full max-w-sm rounded-lg bg-white p-5 ring-1 ring-ink-200 sm:rounded-md">
    <h2 class="text-base font-semibold text-ink-900">{title}</h2>
    {#if lead}
      <p class="mt-2 text-sm text-ink-500">{lead}</p>
    {/if}
    <div class="mt-5 flex flex-col gap-2">
      {@render children()}
      <button
        type="button"
        onclick={oncancel}
        class="rounded-md px-4 py-3 text-sm text-ink-500 transition-colors hover:bg-ink-100"
      >
        {t('hours.cancel')}
      </button>
    </div>
  </div>
</div>

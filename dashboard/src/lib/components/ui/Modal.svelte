<script lang="ts">
  import type { Snippet } from 'svelte';

  import { t } from '$lib/i18n/i18n.svelte';

  let {
    title,
    lead,
    wide = false,
    oncancel,
    children,
  }: {
    title: string;
    lead?: string;
    /** Room for a short form rather than one question. */
    wide?: boolean;
    oncancel: () => void;
    children: Snippet;
  } = $props();
</script>

<svelte:window
  onkeydown={(event) => {
    if (event.key === 'Escape') {
      oncancel();
    }
  }}
/>

<!-- Bottom sheet on a phone, centred on a wide screen: the same component,
     placed where the hand that dismisses it already is. -->
<div
  class="fixed inset-0 z-50 flex items-end justify-center bg-ink-900/60 p-4 sm:items-center"
  role="dialog"
  aria-modal="true"
  aria-label={title}
>
  <!-- Bounded by the viewport and scrolled inside: a sheet taller than a
       phone would otherwise put its own buttons out of reach. -->
  <div
    class="max-h-full w-full overflow-y-auto rounded-lg bg-white p-5 ring-1 ring-ink-200
           sm:rounded-md {wide ? 'max-w-lg' : 'max-w-sm'}"
  >
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

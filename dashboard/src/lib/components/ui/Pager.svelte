<script lang="ts">
  import { PAGE_SIZE, pageCount } from '$lib/paging';
  import { t } from '$lib/i18n/i18n.svelte';
  import Button from '$components/ui/Button.svelte';

  let { total, index = $bindable() }: { total: number; index: number } = $props();

  const pages = $derived(pageCount(total));

  /* Clamped here rather than by every caller: removing the last item of the
     last page must not leave the reader in front of an empty list. */
  $effect(() => {
    if (index > pages - 1) {
      index = pages - 1;
    }
  });
</script>

{#if total > PAGE_SIZE}
  <div class="flex items-center justify-between gap-3 border-t border-ink-100 p-3">
    <Button variant="quiet" size="sm" disabled={index === 0} onclick={() => (index -= 1)}>
      {t('cal.previous')}
    </Button>
    <span class="text-xs text-ink-500">
      {t('cal.page', { current: index + 1, total: pages })}
    </span>
    <Button variant="quiet" size="sm" disabled={index >= pages - 1} onclick={() => (index += 1)}>
      {t('cal.next')}
    </Button>
  </div>
{/if}

<script lang="ts">
  import Clock from '@lucide/svelte/icons/clock';

  import { type LiveSnapshot, subscribeLive } from '$lib/api/live';
  import { formattingLocale, hour12, t } from '$lib/i18n/i18n.svelte';
  import { clocksDisagree } from '$lib/system';

  let heard = $state<{ snapshot: LiveSnapshot; at: number } | null>(null);

  $effect(() => subscribeLive((snapshot) => (heard = { snapshot, at: Date.now() })));

  const disagreeing = $derived(heard !== null && clocksDisagree(heard.snapshot.now_us, heard.at));

  function moment(ms: number): string {
    return new Date(ms).toLocaleString(formattingLocale(), {
      day: 'numeric',
      month: 'long',
      hour: '2-digit',
      minute: '2-digit',
      hour12: hour12(),
    });
  }
</script>

{#if heard && disagreeing}
  <div
    role="alert"
    class="sticky top-0 z-70 flex shrink-0 items-start gap-2 bg-density-low px-4 py-3 text-sm
           text-ink-900"
  >
    <Clock size={16} class="mt-0.5 shrink-0" aria-hidden="true" />
    <p>
      {t('app.clock', {
        appliance: moment(heard.snapshot.now_us / 1000),
        device: moment(heard.at),
      })}
    </p>
  </div>
{/if}

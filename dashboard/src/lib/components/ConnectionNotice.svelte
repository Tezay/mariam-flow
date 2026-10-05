<script lang="ts">
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';

  import { formattingLocale, hour12, t } from '$lib/i18n/i18n.svelte';
  import { formatClock } from '$lib/live';

  let { since }: { since: number | null } = $props();
</script>

<!-- Above everything, the full-frame surfaces included: a labeller in the
     middle of a recording is the one who most needs to know. -->
<div
  role="alert"
  class="sticky top-0 z-70 flex shrink-0 items-start gap-2 bg-danger px-4 py-3 text-sm text-white"
>
  <TriangleAlert size={16} class="mt-0.5 shrink-0" aria-hidden="true" />
  <p>
    <span class="font-medium">
      {since === null
        ? t('app.unreachable')
        : t('app.lost', { when: formatClock(since * 1000, formattingLocale(), hour12()) })}
    </span>
    <span class="text-white/80">
      {since === null ? t('app.retrying') : t('app.lostStale')}
    </span>
  </p>
</div>

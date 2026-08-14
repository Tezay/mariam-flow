<script lang="ts">
  import { viridisGradient } from '$lib/colormap';
  import { t } from '$lib/i18n/i18n.svelte';

  let { low, high }: { low: number; high: number } = $props();
</script>

<!-- A heatmap without its scale is a picture, not a measurement: the same bar
     the Python session report carries. -->
<div class="flex items-center gap-2">
  <span class="text-xs tabular-nums text-ink-500">{low.toFixed(1)}</span>
  <span class="h-2 w-24 rounded-xs ring-1 ring-ink-200" style:background-image={viridisGradient()}
  ></span>
  <span class="text-xs tabular-nums text-ink-500">{high.toFixed(1)}</span>
  <span class="text-xs text-ink-500">{t('portrait.amplitude')}</span>
</div>

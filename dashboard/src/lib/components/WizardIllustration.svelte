<script lang="ts">
  import type { Stage } from '$lib/api';
  import { t } from '$lib/i18n/i18n.svelte';

  let { stage }: { stage: Stage } = $props();

  /* The frame each step's illustration will occupy. It holds its space
     already, so dropping the artwork in later changes nothing around it —
     a placeholder that collapses would move every screen when filled. */
</script>

<div
  class="flex aspect-square w-full items-center justify-center rounded-2xl bg-mariam-50
         ring-1 ring-mariam-100 ring-inset lg:aspect-4/5"
  role="img"
  aria-label={t(`wizard.stage.${stage}` as const)}
>
  <span class="font-mono text-xs text-mariam-600/40">{stage}</span>
</div>

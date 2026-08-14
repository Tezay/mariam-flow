<script lang="ts">
  import { type Portrait } from '$lib/api/portrait';
  import { DENSITY_CLASSES } from '$lib/api/live';
  import { type Viewport, fractionIn, labelSegments, seconds } from '$lib/portrait';
  import { t } from '$lib/i18n/i18n.svelte';

  let { portrait, viewport }: { portrait: Portrait; viewport: Viewport } = $props();

  const segments = $derived(
    labelSegments(portrait).map((segment) => {
      const from = fractionIn(viewport, seconds(portrait, segment.from_us));
      const to = fractionIn(viewport, seconds(portrait, segment.to_us));
      return { density: segment.density, from, to };
    }),
  );

  const shown = $derived(new Set(segments.map((segment) => segment.density)));
</script>

<!-- Overflow hidden rather than clamped positions: a stretch running past the
     edge keeps its true width, so zooming does not stretch it. -->
<div class="relative h-6 w-full overflow-hidden rounded-sm bg-ink-100">
  {#each segments as segment, index (index)}
    <div
      class="absolute inset-y-0"
      style:left="{segment.from * 100}%"
      style:width="{(segment.to - segment.from) * 100}%"
      style:background-color="var(--color-density-{segment.density})"
    ></div>
  {/each}
</div>

<ul class="mt-2 flex flex-wrap gap-x-4 gap-y-1">
  {#each DENSITY_CLASSES as density (density)}
    {#if shown.has(density)}
      <li class="flex items-center gap-1.5 text-xs text-ink-500">
        <span
          class="size-2.5 shrink-0 rounded-xs"
          style:background-color="var(--color-density-{density})"
        ></span>
        {t(`class.${density}` as const)}
      </li>
    {/if}
  {/each}
</ul>

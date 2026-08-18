<script lang="ts">
  import ArrowDown from '@lucide/svelte/icons/arrow-down';
  import ArrowUp from '@lucide/svelte/icons/arrow-up';
  import Minus from '@lucide/svelte/icons/minus';

  import { type Delta, type Direction, formatPoints } from '$lib/analysis';
  import { t } from '$lib/i18n/i18n.svelte';
  import { type MessageKey } from '$lib/i18n/messages';

  /* Only for measures where more is better: one arrow cannot serve a figure
     like the baseline, which rises when the corpus gets easier. */
  let { delta }: { delta: Delta } = $props();

  const MARKS: Record<Direction, { icon: typeof ArrowUp; tone: string; word: MessageKey }> = {
    up: { icon: ArrowUp, tone: 'text-success', word: 'compare.better' },
    down: { icon: ArrowDown, tone: 'text-danger', word: 'compare.worse' },
    level: { icon: Minus, tone: 'text-ink-500', word: 'compare.same' },
  };

  const mark = $derived(MARKS[delta.direction]);
  const Icon = $derived(mark.icon);
  const points = $derived(Math.abs(Math.round(delta.value * 100)));
</script>

<span class="inline-flex items-center gap-0.5 text-xs font-medium tabular-nums {mark.tone}">
  <Icon size={12} aria-hidden="true" />
  <span aria-hidden="true">{formatPoints(delta.value)}</span>
  <span class="sr-only">{t(mark.word, { points })}</span>
</span>

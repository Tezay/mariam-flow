<script lang="ts">
  import Check from '@lucide/svelte/icons/check';
  import Minus from '@lucide/svelte/icons/minus';
  import X from '@lucide/svelte/icons/x';

  import { type Verdict } from '$lib/analysis';
  import { t } from '$lib/i18n/i18n.svelte';

  let { verdict }: { verdict: Verdict } = $props();

  const MARKS: Record<Verdict, { icon: typeof Check; tone: string }> = {
    good: { icon: Check, tone: 'text-density-empty' },
    fair: { icon: Minus, tone: 'text-density-low' },
    poor: { icon: X, tone: 'text-density-saturated' },
  };

  const mark = $derived(MARKS[verdict]);
  const Icon = $derived(mark.icon);
</script>

<!-- Icon and word travel together, so a reading cannot be rendered in colour
     alone by a caller that forgets the second half. -->
<Icon size={16} class="shrink-0 {mark.tone}" aria-hidden="true" />
<span class="sr-only">{t(`analysis.verdict.${verdict}` as const)}</span>

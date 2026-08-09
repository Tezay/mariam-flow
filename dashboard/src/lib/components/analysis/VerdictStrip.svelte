<script lang="ts">
  import Check from '@lucide/svelte/icons/check';
  import Minus from '@lucide/svelte/icons/minus';
  import X from '@lucide/svelte/icons/x';

  import { formatShare, type Reading, type ReadingKey, type Verdict } from '$lib/analysis';
  import { t } from '$lib/i18n/i18n.svelte';

  let {
    rows,
  }: {
    rows: { key: ReadingKey; reading: Reading }[];
  } = $props();

  const MARKS: Record<Verdict, { icon: typeof Check; tone: string }> = {
    good: { icon: Check, tone: 'text-density-empty' },
    fair: { icon: Minus, tone: 'text-density-low' },
    poor: { icon: X, tone: 'text-density-saturated' },
  };
</script>

<ul class="divide-y divide-ink-100 overflow-hidden rounded-md bg-white ring-1 ring-ink-200">
  {#each rows as row (row.key)}
    {@const mark = MARKS[row.reading.verdict]}
    {@const Icon = mark.icon}
    <li class="flex items-center gap-3 px-4 py-3">
      <!-- The mark repeats what the wording already says, so the row survives
           being read without colour. -->
      <Icon size={16} class="shrink-0 {mark.tone}" aria-hidden="true" />
      <span class="min-w-0 flex-1">
        <span class="block text-sm text-ink-900">{t(`analysis.${row.key}` as const)}</span>
        <span class="block text-xs text-ink-500">{t(`analysis.${row.key}.lead` as const)}</span>
      </span>
      <span class="shrink-0 text-sm font-semibold tabular-nums text-ink-900">
        {formatShare(row.reading.value)}
      </span>
      <span class="sr-only">{t(`analysis.verdict.${row.reading.verdict}` as const)}</span>
    </li>
  {/each}
</ul>

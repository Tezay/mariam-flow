<script lang="ts">
  import { formatShare, type Reading, type ReadingKey } from '$lib/analysis';
  import { t } from '$lib/i18n/i18n.svelte';
  import VerdictMark from '$components/analysis/VerdictMark.svelte';

  let {
    rows,
  }: {
    rows: { key: ReadingKey; reading: Reading }[];
  } = $props();
</script>

<ul class="divide-y divide-ink-100 overflow-hidden rounded-md bg-white ring-1 ring-ink-200">
  {#each rows as row (row.key)}
    <li class="flex items-center gap-3 px-4 py-3">
      <VerdictMark verdict={row.reading.verdict} />
      <span class="min-w-0 flex-1">
        <span class="block text-sm text-ink-900">{t(`analysis.${row.key}` as const)}</span>
        <span class="block text-xs text-ink-500">{t(`analysis.${row.key}.lead` as const)}</span>
      </span>
      <span class="shrink-0 text-sm font-semibold tabular-nums text-ink-900">
        {formatShare(row.reading.value)}
      </span>
    </li>
  {/each}
</ul>

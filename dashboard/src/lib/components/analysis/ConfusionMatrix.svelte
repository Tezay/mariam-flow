<script lang="ts">
  import { rowShare } from '$lib/analysis';
  import { DENSITY_CLASSES } from '$lib/api/live';
  import { t } from '$lib/i18n/i18n.svelte';

  let { confusion }: { confusion: number[][] } = $props();

  const rows = $derived(
    DENSITY_CLASSES.map((truth, row) => ({
      truth,
      cells: DENSITY_CLASSES.map((predicted, column) => ({
        predicted,
        count: confusion[row]?.[column] ?? 0,
        share: rowShare(confusion, row, column),
        diagonal: row === column,
      })),
    })),
  );
</script>

<figure class="m-0 overflow-x-auto">
  <table class="w-full min-w-88 border-collapse text-sm">
    <caption class="caption-bottom pt-3 text-left text-xs text-ink-500">
      {t('analysis.matrix.caption')}
    </caption>
    <thead>
      <tr>
        <td class="p-1"></td>
        {#each DENSITY_CLASSES as density (density)}
          <th
            scope="col"
            class="p-1 text-center text-xs font-medium tracking-wide text-ink-500 uppercase"
          >
            {t(`class.${density}` as const)}
          </th>
        {/each}
      </tr>
    </thead>
    <tbody>
      {#each rows as row (row.truth)}
        <tr>
          <th
            scope="row"
            class="py-1 pr-2 text-right text-xs font-medium tracking-wide text-ink-500 uppercase"
          >
            {t(`class.${row.truth}` as const)}
          </th>
          {#each row.cells as cell (cell.predicted)}
            <td class="p-0.5">
              <!-- Intensity carries the share, weight carries the diagonal:
                   colour alone would leave the reading to whoever can see it. -->
              <span
                class="flex h-11 items-center justify-center rounded-sm tabular-nums
                       {cell.diagonal ? 'font-semibold' : 'font-normal'}
                       {cell.share > 0.55 ? 'text-white' : 'text-ink-900'}"
                style:background-color="color-mix(in srgb, var(--color-mariam-600) {Math.round(
                  cell.share * 100,
                )}%, var(--color-ink-50))"
              >
                {cell.count}
              </span>
            </td>
          {/each}
        </tr>
      {/each}
    </tbody>
  </table>
</figure>

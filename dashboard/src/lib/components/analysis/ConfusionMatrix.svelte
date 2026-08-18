<script lang="ts">
  import { rowShare } from '$lib/analysis';
  import { DENSITY_CLASSES } from '$lib/api/live';
  import { t } from '$lib/i18n/i18n.svelte';

  let {
    confusion,
    captionShown = true,
  }: {
    confusion: number[][];
    /**
     * Whether the orientation note is drawn or only spoken.
     *
     * Matrices read side by side share one orientation; it stays in the
     * accessibility tree either way.
     */
    captionShown?: boolean;
  } = $props();

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
  <!-- Fixed layout, or the column under the longest class name is the widest. -->
  <table class="w-full min-w-88 table-fixed border-collapse text-sm">
    <caption
      class="caption-bottom pt-3 text-left text-xs text-ink-500 {captionShown ? '' : 'sr-only'}"
    >
      {t('analysis.matrix.caption')}
    </caption>
    <thead>
      <tr>
        <td class="w-20 p-1"></td>
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
              <!-- The share leads, since counts from runs of different sizes
                   cannot be compared; the count stays, since a share alone
                   hides how few windows it rests on. Intensity repeats the
                   share and weight the diagonal, so neither needs colour. -->
              <span
                class="flex h-12 flex-col items-center justify-center rounded-sm leading-tight
                       tabular-nums
                       {cell.diagonal ? 'font-semibold' : 'font-normal'}
                       {cell.share > 0.55 ? 'text-white' : 'text-ink-900'}"
                style:background-color="color-mix(in srgb, var(--color-mariam-600) {Math.round(
                  cell.share * 100,
                )}%, var(--color-ink-50))"
              >
                <span>{Math.round(cell.share * 100)}<span class="text-[0.6875rem]">%</span></span>
                <span class="text-[0.6875rem] font-normal opacity-70">{cell.count}</span>
              </span>
            </td>
          {/each}
        </tr>
      {/each}
    </tbody>
  </table>
</figure>

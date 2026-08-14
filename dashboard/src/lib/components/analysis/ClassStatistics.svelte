<script lang="ts">
  import { type NodePortrait, type Portrait } from '$lib/api/portrait';
  import { type ClassStat, classStatistics, overlaps } from '$lib/portrait';
  import { t } from '$lib/i18n/i18n.svelte';

  let {
    portrait,
    node,
    feature,
  }: {
    portrait: Portrait;
    node: NodePortrait;
    feature: string;
  } = $props();

  const stats = $derived(classStatistics(portrait, node, feature));

  /** Levels whose spread runs into the next one's, which no threshold splits. */
  const blurred = $derived(
    new Set(
      stats.flatMap((stat, index) => {
        const next: ClassStat | undefined = stats[index + 1];
        return next && overlaps(stat, next) ? [stat.density, next.density] : [];
      }),
    ),
  );

  const widest = $derived(Math.max(...stats.map((stat) => stat.mean + stat.deviation), 1));
</script>

{#if stats.length > 0}
  <table class="w-full border-collapse text-sm">
    <caption class="sr-only">{t('portrait.statsCaption', { node: node.node_id })}</caption>
    <thead>
      <tr>
        <th scope="col" class="pb-1 text-left text-xs font-medium text-ink-500">
          {t('portrait.level')}
        </th>
        <th scope="col" class="pb-1 text-right text-xs font-medium text-ink-500">
          {t('portrait.mean')}
        </th>
        <th scope="col" class="pb-1 text-right text-xs font-medium text-ink-500">
          {t('portrait.windows')}
        </th>
        <th scope="col" class="w-1/3 pb-1"><span class="sr-only">{t('portrait.spread')}</span></th>
      </tr>
    </thead>
    <tbody>
      {#each stats as stat (stat.density)}
        <tr class="border-t border-ink-100">
          <th scope="row" class="py-1.5 text-left font-normal text-ink-900">
            <span class="flex items-center gap-1.5">
              <span
                class="size-2.5 shrink-0 rounded-xs"
                style:background-color="var(--color-density-{stat.density})"
              ></span>
              {t(`class.${stat.density}` as const)}
            </span>
          </th>
          <td class="py-1.5 text-right tabular-nums text-ink-900">
            {stat.mean.toFixed(2)}
            <span class="text-ink-500">± {stat.deviation.toFixed(2)}</span>
          </td>
          <td class="py-1.5 text-right tabular-nums text-ink-500">{stat.samples}</td>
          <td class="py-1.5 pl-3">
            <!-- Mean and spread drawn to one scale: two levels whose bars meet
                 are two levels this measurement does not tell apart. -->
            <span class="relative flex h-2.5 w-full items-center">
              <span
                class="absolute h-1 rounded-full"
                style:left="{Math.max(0, ((stat.mean - stat.deviation) / widest) * 100)}%"
                style:width="{((2 * stat.deviation) / widest) * 100}%"
                style:background-color="var(--color-density-{stat.density})"
                style:opacity="0.35"
              ></span>
              <span
                class="absolute size-2 rounded-full"
                style:left="calc({(stat.mean / widest) * 100}% - 0.25rem)"
                style:background-color="var(--color-density-{stat.density})"
              ></span>
            </span>
          </td>
        </tr>
      {/each}
    </tbody>
  </table>

  {#if blurred.size > 0}
    <p class="mt-3 text-sm text-ink-500">
      {t('portrait.overlapping', {
        levels: [...blurred].map((density) => t(`class.${density}` as const)).join(', '),
      })}
    </p>
  {:else}
    <p class="mt-3 text-sm text-ink-500">{t('portrait.separated')}</p>
  {/if}
{/if}

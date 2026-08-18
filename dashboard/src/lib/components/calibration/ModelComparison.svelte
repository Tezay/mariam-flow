<script lang="ts">
  import ArrowLeft from '@lucide/svelte/icons/arrow-left';
  import BadgeCheck from '@lucide/svelte/icons/badge-check';
  import CircleAlert from '@lucide/svelte/icons/circle-alert';

  import {
    type Comparison,
    type Delta,
    type Readings,
    type Verdict,
    comparison,
    formatPoints,
    formatShare,
    readings,
    levelWindows,
    recall,
    recordingRows,
  } from '$lib/analysis';
  import { DENSITY_CLASSES } from '$lib/api/live';
  import {
    type Evaluation,
    type ModelDetail,
    type StoredModel,
    fetchModel,
    useModel,
  } from '$lib/api/models';
  import { type Status } from '$lib/api/status';
  import { formatWindow, modelName } from '$lib/calibration';
  import { formattingLocale, t } from '$lib/i18n/i18n.svelte';
  import ConfusionMatrix from '$components/analysis/ConfusionMatrix.svelte';
  import DeltaTag from '$components/analysis/DeltaTag.svelte';
  import VerdictMark from '$components/analysis/VerdictMark.svelte';
  import Button from '$components/ui/Button.svelte';

  let {
    reference,
    candidate,
    onback,
    onupdated,
    onchanged,
  }: {
    reference: StoredModel;
    candidate: StoredModel;
    onback: () => void;
    onupdated: (status: Status) => void;
    onchanged: () => void;
  } = $props();

  type Side = { model: StoredModel; evaluation: Evaluation; scores: Readings };
  type Cell = { text: string; verdict?: Verdict; support?: number };
  type Row = { label: string; reference: Cell; candidate: Cell; change?: Delta };

  let details = $state<(ModelDetail | null)[]>([]);
  let busy = $state(false);

  $effect(() => {
    const ids = [reference.id, candidate.id];
    void (async () => {
      details = await Promise.all(ids.map((id) => fetchModel(id)));
    })();
  });

  function side(model: StoredModel, detail: ModelDetail | null | undefined): Side | null {
    return detail?.evaluation
      ? { model, evaluation: detail.evaluation, scores: readings(detail.evaluation) }
      : null;
  }

  const left = $derived(side(reference, details[0]));
  const right = $derived(side(candidate, details[1]));
  const change = $derived<Comparison | null>(
    left && right ? comparison(left.evaluation, right.evaluation) : null,
  );
  const recordings = $derived(
    left && right ? recordingRows(left.evaluation, right.evaluation) : [],
  );

  function count(value: number): string {
    return value.toLocaleString(formattingLocale());
  }

  function reading(side: Side, key: 'presence' | 'ordering'): Cell {
    return { text: formatShare(side.scores[key].value), verdict: side.scores[key].verdict };
  }

  /* Half of eight windows and half of two hundred print the same and are not. */
  function found(side: Side, level: number): Cell {
    return {
      text: formatShare(recall(side.evaluation.confusion, level)),
      support: levelWindows(side.evaluation.confusion, level),
    };
  }

  function groups(a: Side, b: Side, diff: Comparison): { title: string; rows: Row[] }[] {
    return [
      {
        title: t('compare.group.scores'),
        rows: [
          {
            label: t('compare.row.accuracy'),
            reference: {
              text: formatShare(a.evaluation.accuracy),
              verdict: a.scores.exact.verdict,
            },
            candidate: {
              text: formatShare(b.evaluation.accuracy),
              verdict: b.scores.exact.verdict,
            },
            change: diff.exact,
          },
          {
            label: t('compare.row.lift'),
            reference: { text: formatPoints(a.scores.lift) },
            candidate: { text: formatPoints(b.scores.lift) },
            change: diff.lift,
          },
          {
            label: t('compare.row.presence'),
            reference: reading(a, 'presence'),
            candidate: reading(b, 'presence'),
            change: diff.presence,
          },
          {
            label: t('compare.row.ordering'),
            reference: reading(a, 'ordering'),
            candidate: reading(b, 'ordering'),
            change: diff.ordering,
          },
        ],
      },
      {
        title: t('compare.group.perClass'),
        rows: DENSITY_CLASSES.map((density, level) => ({
          label: t(`class.${density}` as const),
          reference: found(a, level),
          candidate: found(b, level),
          change: diff.perClass[level],
        })),
      },
      {
        title: t('compare.group.evaluation'),
        rows: [
          {
            label: t('compare.row.windows'),
            reference: { text: count(a.evaluation.windows) },
            candidate: { text: count(b.evaluation.windows) },
          },
          {
            label: t('compare.row.recordings'),
            reference: { text: count(a.evaluation.sessions.length) },
            candidate: { text: count(b.evaluation.sessions.length) },
          },
          {
            label: t('compare.row.splits'),
            reference: { text: count(a.evaluation.splits) },
            candidate: { text: count(b.evaluation.splits) },
          },
        ],
      },
      {
        title: t('compare.group.model'),
        rows: [
          {
            label: t('compare.row.window'),
            reference: { text: t('model.window', { value: formatWindow(a.model.window_us) }) },
            candidate: { text: t('model.window', { value: formatWindow(b.model.window_us) }) },
          },
          {
            label: t('compare.row.receivers'),
            reference: { text: count(a.model.receivers) },
            candidate: { text: count(b.model.receivers) },
          },
          {
            label: t('compare.row.trained'),
            reference: { text: a.model.manifest?.trained_at || '—' },
            candidate: { text: b.model.manifest?.trained_at || '—' },
          },
        ],
      },
    ];
  }

  function panes(a: Side, b: Side): { side: Side; role: string }[] {
    return [
      { side: a, role: t('compare.reference') },
      { side: b, role: t('compare.candidate') },
    ];
  }

  function pair(row: Row, a: Side, b: Side): { side: Side; cell: Cell; change?: Delta }[] {
    return [
      { side: a, cell: row.reference },
      { side: b, cell: row.candidate, change: row.change },
    ];
  }

  async function put(id: string) {
    busy = true;
    const outcome = await useModel(id);
    busy = false;
    if (outcome.kind === 'ok') {
      onupdated(outcome.status);
      onchanged();
    }
  }
</script>

{#snippet head(model: StoredModel, role: string)}
  <span class="block text-xs font-medium tracking-wide text-ink-500 uppercase">{role}</span>
  <span class="mt-0.5 block text-sm font-semibold wrap-break-word text-ink-900">
    {modelName(model)}
  </span>
  <span class="mt-1.5 block">
    {#if model.active}
      <span class="inline-flex items-center gap-1 text-xs font-medium text-mariam-700">
        <BadgeCheck size={13} aria-hidden="true" />{t('model.inService')}
      </span>
    {:else}
      <Button size="sm" variant="outline" disabled={busy} onclick={() => void put(model.id)}>
        {t('model.use')}
      </Button>
    {/if}
  </span>
{/snippet}

{#snippet value(cell: Cell, change: Delta | undefined)}
  <span class="flex items-center justify-end gap-1.5">
    {#if cell.verdict}<VerdictMark verdict={cell.verdict} />{/if}
    <span class="font-medium tabular-nums text-ink-900">{cell.text}</span>
    {#if cell.support !== undefined}
      <span class="text-xs tabular-nums text-ink-500">
        ({count(cell.support)})<span class="sr-only"> {t('compare.windowsHeld')}</span>
      </span>
    {/if}
    <!-- Reserved on both sides, so the figures themselves stay in one column
         instead of shifting by the width of a difference. -->
    <span class="w-10 shrink-0 text-right">
      {#if change}<DeltaTag delta={change} />{/if}
    </span>
  </span>
{/snippet}

<section>
  <Button variant="quiet" size="sm" onclick={onback}>
    <ArrowLeft size={14} aria-hidden="true" />
    {t('compare.back')}
  </Button>

  <h2 class="mt-3 text-2xl font-semibold tracking-tight text-ink-900">{t('compare.title')}</h2>
  <p class="mt-1 max-w-prose text-sm text-ink-500">{t('compare.lead')}</p>

  {#if left && right && change}
    {#if !change.likeForLike}
      <p class="mt-4 flex items-start gap-2 rounded-md bg-ink-100 px-4 py-3 text-sm text-ink-700">
        <CircleAlert size={16} class="mt-0.5 shrink-0 text-density-medium" aria-hidden="true" />
        <span>
          <span class="font-medium text-ink-900">{t('compare.differentRecordings')}</span>
          {t('compare.differentRecordingsLead', {
            shared: change.recordings.shared,
            total: recordings.length,
          })}
        </span>
      </p>
    {/if}

    {@const bands = groups(left, right, change)}

    <!-- Two renderings of one row model: a table where three columns fit, a
         stack where they do not — a phone cuts the figures otherwise. -->
    <div class="mt-5 hidden overflow-clip rounded-md bg-white ring-1 ring-ink-200 md:block">
      <table aria-label={t('compare.title')} class="w-full table-fixed border-collapse text-sm">
        <thead>
          <tr class="border-b border-ink-200">
            <th scope="col" class="w-2/5 px-3 py-3">
              <span class="sr-only">{t('compare.measure')}</span>
            </th>
            <th scope="col" class="px-3 py-3 text-right align-top">
              {@render head(left.model, t('compare.reference'))}
            </th>
            <th scope="col" class="px-3 py-3 text-right align-top">
              {@render head(right.model, t('compare.candidate'))}
            </th>
          </tr>
        </thead>

        {#each bands as group (group.title)}
          <tbody>
            <tr class="border-y border-ink-100 bg-ink-50">
              <th
                scope="colgroup"
                class="px-3 py-1.5 text-left text-xs font-medium tracking-wide text-ink-500
                       uppercase"
              >
                {group.title}
              </th>
              {#each [left, right] as shown (shown.model.id)}
                <td aria-hidden="true" class="truncate px-3 py-1.5 text-right text-xs text-ink-500">
                  {modelName(shown.model)}
                </td>
              {/each}
            </tr>
            {#each group.rows as row (row.label)}
              <tr class="border-b border-ink-100 last:border-0">
                <th scope="row" class="px-3 py-2.5 text-left font-normal text-ink-700">
                  {row.label}
                </th>
                <td class="px-3 py-2.5">{@render value(row.reference, undefined)}</td>
                <td class="px-3 py-2.5">{@render value(row.candidate, row.change)}</td>
              </tr>
            {/each}
          </tbody>
        {/each}
      </table>
    </div>

    <div class="mt-5 md:hidden">
      <div class="flex gap-3">
        {#each panes(left, right) as pane (pane.side.model.id)}
          <div class="min-w-0 flex-1 rounded-md bg-white p-3 ring-1 ring-ink-200">
            {@render head(pane.side.model, pane.role)}
          </div>
        {/each}
      </div>

      {#each bands as group (group.title)}
        <h4 class="mt-4 mb-1.5 px-1 text-xs font-medium tracking-wide text-ink-500 uppercase">
          {group.title}
        </h4>
        <ul
          aria-label={group.title}
          class="divide-y divide-ink-100 overflow-clip rounded-md bg-white ring-1 ring-ink-200"
        >
          {#each group.rows as row (row.label)}
            <li class="px-3 py-2.5">
              <p class="text-xs text-ink-500">{row.label}</p>
              {#each pair(row, left, right) as entry (entry.side.model.id)}
                <p class="mt-1 flex items-center justify-between gap-3">
                  <span class="min-w-0 truncate text-sm text-ink-700">
                    {modelName(entry.side.model)}
                  </span>
                  {@render value(entry.cell, entry.change)}
                </p>
              {/each}
            </li>
          {/each}
        </ul>
      {/each}
    </div>

    <h3 class="mt-8 text-[0.9375rem] leading-6 font-semibold text-ink-900">
      {t('compare.section.matrices')}
    </h3>
    <p class="mt-0.5 mb-3 max-w-prose text-sm text-ink-500">{t('analysis.matrix.caption')}</p>
    <div class="grid gap-4 lg:grid-cols-2 lg:items-start">
      {#each [left, right] as shown (shown.model.id)}
        <div class="rounded-md bg-white p-4 ring-1 ring-ink-200">
          <h4 class="mb-3 text-xs font-medium tracking-wide wrap-break-word text-ink-500 uppercase">
            {modelName(shown.model)}
          </h4>
          <ConfusionMatrix confusion={shown.evaluation.confusion} captionShown={false} />
        </div>
      {/each}
    </div>

    <h3 class="mt-8 text-[0.9375rem] leading-6 font-semibold text-ink-900">
      {t('compare.section.recordings')}
    </h3>
    <p class="mt-0.5 mb-3 max-w-prose text-sm text-ink-500">{t('compare.recordingsLead')}</p>
    <div class="overflow-clip rounded-md bg-white ring-1 ring-ink-200">
      <table aria-label={t('compare.section.recordings')} class="w-full border-collapse text-sm">
        <thead>
          <tr class="border-b border-ink-200">
            <th scope="col" class="p-3 text-left text-xs font-medium tracking-wide text-ink-500">
              <span class="sr-only">{t('compare.section.recordings')}</span>
            </th>
            {#each [left, right] as shown (shown.model.id)}
              <th
                scope="col"
                class="p-3 text-right text-xs font-medium tracking-wide wrap-break-word
                       text-ink-500"
              >
                {modelName(shown.model)}
              </th>
            {/each}
          </tr>
        </thead>
        <tbody class="divide-y divide-ink-100">
          {#each recordings as row (row.session_id)}
            <tr>
              <th
                scope="row"
                class="px-3 py-2 text-left font-mono text-xs font-normal text-ink-700"
              >
                {row.session_id}
              </th>
              {#each [row.reference, row.candidate] as windows, column (column)}
                <td class="px-3 py-2 text-right tabular-nums text-ink-900">
                  {#if windows === null}
                    <span aria-hidden="true" class="text-ink-300">—</span>
                    <span class="sr-only">{t('compare.notUsed')}</span>
                  {:else}
                    {t('model.detail.windowsCount', { count: windows })}
                  {/if}
                </td>
              {/each}
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {:else if details.length > 0}
    <div class="mt-6 flex items-start gap-2 rounded-md bg-ink-100 p-5">
      <CircleAlert size={16} class="mt-0.5 shrink-0 text-density-medium" aria-hidden="true" />
      <span>
        <span class="block text-sm font-medium text-ink-900">{t('compare.unreadable')}</span>
        <span class="block text-sm text-ink-500">{t('compare.unreadableLead')}</span>
      </span>
    </div>
  {/if}
</section>

<script lang="ts">
  import ArrowLeft from '@lucide/svelte/icons/arrow-left';
  import BadgeCheck from '@lucide/svelte/icons/badge-check';
  import CircleAlert from '@lucide/svelte/icons/circle-alert';
  import Scale from '@lucide/svelte/icons/scale';

  import { formatPoints, formatShare, readings } from '$lib/analysis';
  import { type RecordedSession } from '$lib/api/calibration';
  import { type ModelDetail, type StoredModel, fetchModel, useModel } from '$lib/api/models';
  import { type Status } from '$lib/api/status';
  import { formatDay, formatWindow, modelName } from '$lib/calibration';
  import { formattingLocale, t } from '$lib/i18n/i18n.svelte';
  import ConfusionMatrix from '$components/analysis/ConfusionMatrix.svelte';
  import VerdictStrip from '$components/analysis/VerdictStrip.svelte';
  import Button from '$components/ui/Button.svelte';
  import Disclosure from '$components/ui/Disclosure.svelte';
  import StatTile from '$components/ui/StatTile.svelte';

  let {
    model,
    recordings,
    inService,
    onback,
    onupdated,
    onchanged,
    oncompare,
  }: {
    model: StoredModel;
    recordings: RecordedSession[];
    inService: StoredModel | null;
    onback: () => void;
    onupdated: (status: Status) => void;
    onchanged: () => void;
    oncompare: (reference: string, candidate: string) => void;
  } = $props();

  let detail = $state<ModelDetail | null>(null);
  let busy = $state(false);

  $effect(() => {
    const id = model.id;
    void (async () => {
      detail = await fetchModel(id);
    })();
  });

  const evaluation = $derived(detail?.evaluation ?? null);
  const scores = $derived(evaluation ? readings(evaluation) : null);

  /* Offered only once both sides can answer: a comparison against a bundle
     that carries no scores has nothing to draw. */
  const against = $derived(
    !model.active && evaluation && inService?.has_evaluation ? inService : null,
  );

  /* A capture has its own lifetime: it is often exported and removed long
     before the model trained on it comes back. */
  const captures = $derived(
    (evaluation?.sessions ?? []).map((session) => ({
      ...session,
      held: recordings.some((recording) => recording.session_id === session.session_id),
    })),
  );

  async function put() {
    busy = true;
    const outcome = await useModel(model.id);
    busy = false;
    if (outcome.kind === 'ok') {
      onupdated(outcome.status);
      onchanged();
    }
  }
</script>

<section>
  <Button variant="quiet" size="sm" onclick={onback}>
    <ArrowLeft size={14} aria-hidden="true" />
    {t('model.back')}
  </Button>

  <header class="mt-3 flex flex-wrap items-start justify-between gap-4">
    <div class="min-w-0">
      <h2 class="text-2xl font-semibold tracking-tight wrap-break-word text-ink-900">
        {modelName(model)}
      </h2>
      <p class="mt-1 text-sm text-ink-500">
        {#if model.manifest?.trained_at}
          {t('model.trained', { when: model.manifest.trained_at })} ·
        {/if}
        {t('model.importedOn', { when: formatDay(model.imported_at_us, formattingLocale()) })} ·
        {t('model.receivers', { count: model.receivers })} ·
        {t('model.window', { value: formatWindow(model.window_us) })}
      </p>
    </div>
    {#if model.active}
      <p
        class="flex shrink-0 items-center gap-1.5 rounded-md bg-mariam-50 px-2.5 py-1.5 text-xs
               font-medium text-mariam-700"
      >
        <BadgeCheck size={14} aria-hidden="true" />{t('model.inService')}
      </p>
    {:else}
      <div class="flex shrink-0 flex-wrap items-center gap-2">
        {#if against}
          <Button variant="outline" onclick={() => oncompare(against.id, model.id)}>
            <Scale size={14} aria-hidden="true" />
            {t('compare.open')}
          </Button>
        {/if}
        <Button disabled={busy} onclick={() => void put()}>{t('model.use')}</Button>
      </div>
    {/if}
  </header>

  {#if evaluation && scores}
    <div class="mt-6 grid grid-cols-2 gap-3 lg:grid-cols-4">
      <StatTile
        label={t('model.detail.accuracy')}
        value={formatShare(evaluation.accuracy)}
        hint={t('model.detail.lift', { points: formatPoints(scores.lift) })}
      />
      <StatTile
        label={t('model.detail.baseline')}
        value={formatShare(evaluation.baseline_accuracy)}
        hint={t('model.detail.baselineHint')}
      />
      <StatTile
        label={t('model.detail.windows')}
        value={evaluation.windows.toLocaleString(formattingLocale())}
      />
      <StatTile
        label={t('model.detail.sessions')}
        value={`${evaluation.sessions.length}`}
        hint={t('model.detail.splits', { count: evaluation.splits })}
      />
    </div>

    <h3 class="mt-8 text-[0.9375rem] leading-6 font-semibold text-ink-900">
      {t('model.detail.whatItCanDo')}
    </h3>
    <p class="mt-0.5 mb-3 max-w-prose text-sm text-ink-500">{t('model.detail.whatItCanDoLead')}</p>
    <VerdictStrip
      rows={[
        { key: 'presence', reading: scores.presence },
        { key: 'ordering', reading: scores.ordering },
        { key: 'exact', reading: scores.exact },
      ]}
    />

    <!-- Open: the strip above is a summary of this matrix, so folding it away
         would put the evidence behind its own conclusion. -->
    <div class="mt-4">
      <Disclosure title={t('model.detail.matrix')} open>
        <div class="p-4">
          <ConfusionMatrix confusion={evaluation.confusion} />
        </div>
      </Disclosure>
    </div>

    <div class="mt-3">
      <Disclosure title={t('model.detail.captures', { count: captures.length })}>
        <ul class="divide-y divide-ink-100">
          {#each captures as capture (capture.session_id)}
            <li class="flex flex-wrap items-baseline justify-between gap-2 px-4 py-3">
              <span class="min-w-0">
                <span class="block font-mono text-xs wrap-break-word text-ink-900">
                  {capture.session_id}
                </span>
                {#if !capture.held}
                  <span class="block text-xs text-ink-500">{t('model.detail.captureGone')}</span>
                {/if}
              </span>
              <span class="shrink-0 text-xs tabular-nums text-ink-500">
                {t('model.detail.windowsCount', { count: capture.windows })}
              </span>
            </li>
          {/each}
        </ul>
      </Disclosure>
    </div>
  {:else if detail}
    <div class="mt-6 flex items-start gap-2 rounded-md bg-ink-100 p-5">
      <CircleAlert size={16} class="mt-0.5 shrink-0 text-density-medium" aria-hidden="true" />
      <span>
        <span class="block text-sm font-medium text-ink-900">
          {t('model.detail.noEvaluation')}
        </span>
        <span class="block text-sm text-ink-500">{t('model.detail.noEvaluationLead')}</span>
      </span>
    </div>
  {/if}
</section>

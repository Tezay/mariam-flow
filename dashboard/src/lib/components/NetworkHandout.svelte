<script lang="ts">
  import Download from '@lucide/svelte/icons/download';
  import Send from '@lucide/svelte/icons/send';
  import Printer from '@lucide/svelte/icons/printer';

  import type { NetworkSurvey, Status } from '$lib/api';
  import { t } from '$lib/i18n/i18n.svelte';
  import { applianceFacts, asksFor, handoutMarkdown, type Handout } from '$lib/network';

  let {
    status,
    survey,
    offline,
  }: { status: Status; survey: NetworkSurvey | null; offline: boolean } = $props();

  const facts = $derived(
    applianceFacts(status).map((row) => ({
      label: t(`handout.${row.label}` as const),
      value: row.value,
    })),
  );

  const asks = $derived(asksFor(survey, offline).map((ask) => t(`handout.ask.${ask}` as const)));

  const document_ = $derived<Handout>({
    title: t('handout.title'),
    facts,
    asks,
    sections: [
      { heading: t('handout.flows'), body: t('handout.flowsLead') },
      { heading: t('handout.privacy'), body: t('handout.privacy') },
    ],
  });

  function download() {
    const blob = new Blob([handoutMarkdown(document_)], { type: 'text/markdown' });
    const url = URL.createObjectURL(blob);
    const link = window.document.createElement('a');
    link.href = url;
    link.download = `mariam-flow-${status.kit_id}-network.md`;
    link.click();
    URL.revokeObjectURL(url);
  }
</script>

<!-- Framed as a document for someone else rather than another settings block:
     it is meant to leave the screen and reach the site's network team. -->
<section
  class="print-sheet rounded-xl border-2 border-dashed border-mariam-200 bg-white p-5
         print:border-0 print:p-0"
>
  <p
    class="inline-flex items-center gap-1.5 rounded-full bg-mariam-50 px-3 py-1 text-xs
           font-medium text-mariam-700 print:hidden"
  >
    <Send size={13} aria-hidden="true" />{t('handout.forWhom')}
  </p>

  <header class="mt-3 flex flex-wrap items-start justify-between gap-3">
    <div>
      <h3 class="text-base font-semibold text-ink-900">{t('handout.title')}</h3>
      <p class="mt-1 text-xs text-ink-500">{t('handout.lead')}</p>
    </div>
    <!-- Hidden from the printed sheet: buttons on paper are noise. -->
    <div class="flex shrink-0 gap-2 print:hidden">
      <button
        type="button"
        onclick={() => window.print()}
        class="inline-flex items-center gap-1.5 rounded-md px-2 py-1 text-xs text-ink-500
               transition-colors hover:bg-ink-100 hover:text-ink-900
               disabled:text-ink-300 disabled:hover:bg-transparent"
      >
        <Printer size={14} aria-hidden="true" />{t('handout.print')}
      </button>
      <button
        type="button"
        onclick={download}
        class="inline-flex items-center gap-1.5 rounded-md px-2 py-1 text-xs text-mariam-600
               transition-colors hover:bg-ink-100
               disabled:text-ink-300 disabled:hover:bg-transparent"
      >
        <Download size={14} aria-hidden="true" />{t('handout.download')}
      </button>
    </div>
  </header>

  <h4 class="mt-4 text-xs font-medium tracking-wide text-ink-500 uppercase">
    {t('handout.appliance')}
  </h4>
  <dl class="mt-2 divide-y divide-ink-100">
    {#each facts as row (row.label)}
      <div class="flex flex-wrap items-baseline justify-between gap-2 py-2">
        <dt class="text-sm text-ink-500">{row.label}</dt>
        <dd class="font-mono text-sm text-ink-900">
          {#if row.value}
            {row.value}
          {:else}
            <span class="text-ink-300">____________________</span>
          {/if}
        </dd>
      </div>
    {/each}
  </dl>
  <p class="mt-1 text-xs text-ink-500">{t('handout.uplinkMacHint')}</p>

  {#if asks.length > 0}
    <h4 class="mt-5 text-xs font-medium tracking-wide text-ink-500 uppercase">
      {t('handout.asks')}
    </h4>
    <ul class="mt-2 space-y-2">
      {#each asks as ask (ask)}
        <li class="flex items-start gap-2 text-sm text-ink-900">
          <span class="mt-1 size-3 shrink-0 rounded-xs border border-ink-300" aria-hidden="true"
          ></span>
          {ask}
        </li>
      {/each}
    </ul>
  {/if}

  <h4 class="mt-5 text-xs font-medium tracking-wide text-ink-500 uppercase">
    {t('handout.flows')}
  </h4>
  <p class="mt-2 text-sm text-ink-700">{t('handout.flowsLead')}</p>
  <p class="mt-3 rounded-md bg-ink-50 p-3 text-sm text-ink-700">{t('handout.privacy')}</p>
</section>

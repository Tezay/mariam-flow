<script lang="ts">
  import BadgeCheck from '@lucide/svelte/icons/badge-check';
  import ChartNoAxesColumn from '@lucide/svelte/icons/chart-no-axes-column';
  import CircleAlert from '@lucide/svelte/icons/circle-alert';
  import Cpu from '@lucide/svelte/icons/cpu';
  import Pencil from '@lucide/svelte/icons/pencil';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import Upload from '@lucide/svelte/icons/upload';

  import {
    type StoredModel,
    forgetModel,
    importModel,
    renameModel,
    useModel,
  } from '$lib/api/models';
  import { type Status } from '$lib/api/status';
  import { formatDay, formatWindow, modelName } from '$lib/calibration';
  import { page } from '$lib/paging';
  import { formattingLocale, t } from '$lib/i18n/i18n.svelte';
  import Button from '$components/ui/Button.svelte';
  import ConfirmDialog from '$components/ui/ConfirmDialog.svelte';
  import NameDialog from '$components/ui/NameDialog.svelte';
  import Pager from '$components/ui/Pager.svelte';
  import SectionHeader from '$components/ui/SectionHeader.svelte';

  let {
    models,
    onupdated,
    onchanged,
    onopen,
  }: {
    models: StoredModel[];
    onupdated: (status: Status) => void;
    onchanged: () => void;
    onopen: (model: StoredModel) => void;
  } = $props();

  let bundle = $state<File | null>(null);
  let busy = $state(false);
  let failure = $state<string | null>(null);
  let forgetting = $state<StoredModel | null>(null);
  let renaming = $state<StoredModel | null>(null);
  let pageIndex = $state(0);

  const active = $derived(models.find((model) => model.active) ?? null);
  const others = $derived(models.filter((model) => !model.active));
  const shown = $derived(page(others, pageIndex));

  function named(model: StoredModel): string {
    return model.manifest ? modelName(model) : t('model.anonymous');
  }

  async function bringIn() {
    if (!bundle) {
      return;
    }
    busy = true;
    const outcome = await importModel(bundle);
    busy = false;
    if (outcome.kind === 'ok') {
      failure = null;
      bundle = null;
      onupdated(outcome.status);
      onchanged();
      return;
    }
    // The appliance's own words: a mismatched receiver count is exactly what
    // the operator needs to read, and rewording it would lose the numbers.
    failure =
      outcome.kind === 'refused'
        ? t('wizard.refused', { message: outcome.message })
        : t('wizard.failed');
  }

  async function put(id: string) {
    busy = true;
    const outcome = await useModel(id);
    busy = false;
    if (outcome.kind === 'ok') {
      failure = null;
      onupdated(outcome.status);
      onchanged();
    }
  }

  async function rename(name: string) {
    if (renaming && (await renameModel(renaming.id, name))) {
      onchanged();
    }
    renaming = null;
  }

  async function forget() {
    if (forgetting && (await forgetModel(forgetting.id))) {
      onchanged();
    }
    forgetting = null;
  }
</script>

<section>
  <SectionHeader icon={Cpu} title={t('models.title')} lead={t('models.lead')} />

  <!-- One surface: the model in service heads the list it belongs to rather
       than floating above it, so there is nothing to open or close. -->
  <div class="mt-4 overflow-hidden rounded-md bg-white ring-1 ring-ink-200">
    {#if active}
      <div class="bg-mariam-600 p-5 text-white">
        <div class="flex items-start justify-between gap-3">
          <p
            class="flex items-center gap-1.5 text-xs font-medium tracking-wide text-white/70
                   uppercase"
          >
            <BadgeCheck size={14} aria-hidden="true" />{t('model.inService')}
          </p>
          <button
            type="button"
            onclick={() => (renaming = active)}
            aria-label={t('model.rename')}
            title={t('model.rename')}
            class="-mt-1 -mr-1 rounded-md p-1.5 text-white/70 transition-colors
                   hover:bg-white/10 hover:text-white"
          >
            <Pencil size={14} aria-hidden="true" />
          </button>
        </div>
        <h3 class="mt-2 text-xl font-semibold wrap-break-word">{named(active)}</h3>
        <p class="mt-1 text-sm text-white/80">
          {#if active.manifest?.trained_at}
            {t('model.trained', { when: active.manifest.trained_at })} ·
          {/if}
          {t('model.importedOn', { when: formatDay(active.imported_at_us, formattingLocale()) })}
        </p>
        <p class="mt-0.5 text-sm text-white/60">
          {t('model.window', { value: formatWindow(active.window_us) })} ·
          {t('model.receivers', { count: active.receivers })}
        </p>
        <div class="mt-3">
          <Button variant="outline" size="sm" onclick={() => onopen(active)}>
            <ChartNoAxesColumn size={14} aria-hidden="true" />
            {t('model.inspect')}
          </Button>
        </div>
      </div>
    {:else}
      <div class="flex items-start gap-2 bg-ink-100 p-5">
        <CircleAlert size={16} class="mt-0.5 shrink-0 text-density-medium" aria-hidden="true" />
        <span>
          <span class="block text-sm font-medium text-ink-900">{t('model.noneTitle')}</span>
          <span class="block text-sm text-ink-500">{t('model.noneLead')}</span>
        </span>
      </div>
    {/if}

    {#if others.length > 0}
      <h3 class="px-5 pt-4 pb-2 text-xs font-medium tracking-wide text-ink-500 uppercase">
        {t('model.library')}
      </h3>
      <ul class="divide-y divide-ink-100">
        {#each shown as model (model.id)}
          <li class="flex flex-wrap items-center justify-between gap-3 px-5 py-3">
            <span class="min-w-0 flex-1">
              <span class="block text-sm font-medium wrap-break-word text-ink-900"
                >{named(model)}</span
              >
              <span class="block text-xs text-ink-500">
                {t('model.importedOn', {
                  when: formatDay(model.imported_at_us, formattingLocale()),
                })} · {t('model.window', { value: formatWindow(model.window_us) })}
              </span>
            </span>
            <span class="flex shrink-0 items-center gap-1">
              <Button
                variant="quiet"
                size="icon"
                label={t('model.inspect')}
                title={t('model.inspect')}
                onclick={() => onopen(model)}
              >
                <ChartNoAxesColumn size={14} aria-hidden="true" />
              </Button>
              <Button
                variant="outline"
                size="sm"
                disabled={busy}
                onclick={() => void put(model.id)}
              >
                {t('model.use')}
              </Button>
              <Button
                variant="quiet"
                size="icon"
                label={t('model.rename')}
                title={t('model.rename')}
                onclick={() => (renaming = model)}
              >
                <Pencil size={14} aria-hidden="true" />
              </Button>
              <Button
                variant="quiet"
                size="icon"
                label={t('model.forget')}
                title={t('model.forget')}
                onclick={() => (forgetting = model)}
              >
                <Trash2 size={14} aria-hidden="true" />
              </Button>
            </span>
          </li>
        {/each}
      </ul>
      <Pager total={others.length} bind:index={pageIndex} />
    {/if}

    <div class="border-t border-ink-100 p-5">
      <label class="block">
        <span class="sr-only">{t('model.choose')}</span>
        <input
          type="file"
          accept=".gz,.tgz,application/gzip"
          onchange={(event) => (bundle = event.currentTarget.files?.[0] ?? null)}
          class="block w-full text-sm text-ink-500 file:mr-3 file:rounded-md file:border-0
                 file:bg-ink-100 file:px-3 file:py-2 file:text-sm file:font-medium
                 file:text-ink-900 hover:file:bg-ink-200"
        />
      </label>

      {#if failure}
        <p role="status" class="mt-3 text-sm text-danger">{failure}</p>
      {/if}

      <div class="mt-3">
        <Button disabled={busy || bundle === null} onclick={() => void bringIn()}>
          <Upload size={14} aria-hidden="true" />
          {busy ? t('model.importing') : t('model.import')}
        </Button>
      </div>
    </div>
  </div>
</section>

{#if renaming}
  <NameDialog
    title={t('model.rename')}
    initial={renaming.manifest ? modelName(renaming) : ''}
    onrename={(name) => void rename(name)}
    oncancel={() => (renaming = null)}
  />
{/if}

{#if forgetting}
  <ConfirmDialog
    title={t('model.confirmForget')}
    lead={t('model.confirmForgetLead')}
    confirmLabel={t('model.forget')}
    onconfirm={() => void forget()}
    oncancel={() => (forgetting = null)}
  />
{/if}

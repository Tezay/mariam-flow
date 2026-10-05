<script lang="ts">
  import Archive from '@lucide/svelte/icons/archive';
  import ChartNoAxesColumn from '@lucide/svelte/icons/chart-no-axes-column';
  import Pencil from '@lucide/svelte/icons/pencil';
  import Trash2 from '@lucide/svelte/icons/trash-2';

  import { type StoredModel, forgetModel, renameModel, useModel } from '$lib/api/models';
  import { type Status } from '$lib/api/status';
  import { formatDay, formatTrained, modelName } from '$lib/calibration';
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

  let busy = $state(false);
  let forgetting = $state<StoredModel | null>(null);
  let renaming = $state<StoredModel | null>(null);
  let pageIndex = $state(0);

  const kept = $derived(models.filter((model) => !model.active));
  const shown = $derived(page(kept, pageIndex));

  function named(model: StoredModel): string {
    return model.manifest ? modelName(model) : t('model.anonymous');
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
  <SectionHeader icon={Archive} title={t('model.library')} lead={t('model.libraryLead')} />

  <div class="mt-4 overflow-hidden rounded-md bg-white ring-1 ring-ink-200">
    {#if kept.length === 0}
      <p class="p-5 text-sm text-ink-500">{t('model.libraryEmpty')}</p>
    {:else}
      <ul class="divide-y divide-ink-100">
        {#each shown as model (model.id)}
          <li
            class="flex flex-col gap-2 px-5 py-3 sm:flex-row sm:items-center sm:justify-between
                   sm:gap-3"
          >
            <span class="min-w-0 flex-1">
              <span class="block text-sm font-medium wrap-break-word text-ink-900"
                >{named(model)}</span
              >
              <span class="block text-xs text-ink-500">
                {#if model.manifest?.trained_at}
                  {t('model.trained', {
                    when: formatTrained(model.manifest.trained_at, formattingLocale()),
                  })} ·
                {/if}
                {t('model.importedOn', {
                  when: formatDay(model.imported_at_us, formattingLocale()),
                })}
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
      <Pager total={kept.length} bind:index={pageIndex} />
    {/if}
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

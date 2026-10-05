<script lang="ts">
  import BadgeCheck from '@lucide/svelte/icons/badge-check';
  import ChartNoAxesColumn from '@lucide/svelte/icons/chart-no-axes-column';
  import CircleAlert from '@lucide/svelte/icons/circle-alert';
  import Pencil from '@lucide/svelte/icons/pencil';
  import Scale from '@lucide/svelte/icons/scale';

  import { type StoredModel, renameModel } from '$lib/api/models';
  import { formatDay, formatTrained, modelName } from '$lib/calibration';
  import { formattingLocale, t } from '$lib/i18n/i18n.svelte';
  import Button from '$components/ui/Button.svelte';
  import NameDialog from '$components/ui/NameDialog.svelte';

  let {
    model,
    replaced = null,
    onopen,
    oncompare,
    onchanged,
  }: {
    model: StoredModel | null;
    /** The model this one has just been imported over, if it can be compared. */
    replaced?: StoredModel | null;
    onopen: (model: StoredModel) => void;
    oncompare: (reference: string, candidate: string) => void;
    onchanged: () => void;
  } = $props();

  let renaming = $state(false);

  async function rename(name: string) {
    if (model && (await renameModel(model.id, name))) {
      onchanged();
    }
    renaming = false;
  }
</script>

{#if model}
  {@const current = model}
  <section class="rounded-md bg-mariam-600 p-5 text-white" aria-label={t('model.inService')}>
    <div class="flex items-start justify-between gap-3">
      <p
        class="flex items-center gap-1.5 text-xs font-medium tracking-wide text-white/70
               uppercase"
      >
        <BadgeCheck size={14} aria-hidden="true" />{t('model.inService')}
      </p>
      <button
        type="button"
        onclick={() => (renaming = true)}
        aria-label={t('model.rename')}
        title={t('model.rename')}
        class="-mt-1 -mr-1 rounded-md p-1.5 text-white/70 transition-colors
               hover:bg-white/10 hover:text-white"
      >
        <Pencil size={14} aria-hidden="true" />
      </button>
    </div>
    <h2 class="mt-2 text-xl font-semibold wrap-break-word">
      {current.manifest ? modelName(current) : t('model.anonymous')}
    </h2>
    {#if current.manifest}
      <p class="mt-1 text-sm text-white/80">
        {t('model.trained', {
          when: formatTrained(current.manifest.trained_at, formattingLocale()),
        })} ·
        {current.manifest.sessions === 1
          ? t('model.fromOneRecording')
          : t('model.fromRecordings', { count: current.manifest.sessions })}
      </p>
    {/if}
    <p class="mt-0.5 text-sm text-white/60">
      {t('model.importedOn', { when: formatDay(current.imported_at_us, formattingLocale()) })}
    </p>

    <!-- An import puts the new model straight into service, over one that
         was working. -->
    {#if replaced}
      <p role="status" class="mt-3 text-sm text-white/80">{t('model.imported')}</p>
    {/if}
    <div class="mt-3 flex flex-wrap gap-2">
      <Button variant="outline" size="sm" onclick={() => onopen(current)}>
        <ChartNoAxesColumn size={14} aria-hidden="true" />{t('model.inspect')}
      </Button>
      {#if replaced}
        {@const previous = replaced}
        <Button variant="outline" size="sm" onclick={() => oncompare(previous.id, current.id)}>
          <Scale size={14} aria-hidden="true" />{t('compare.openReplaced')}
        </Button>
      {/if}
    </div>
  </section>

  {#if renaming}
    <NameDialog
      title={t('model.rename')}
      initial={current.manifest ? modelName(current) : ''}
      onrename={(name) => void rename(name)}
      oncancel={() => (renaming = false)}
    />
  {/if}
{:else}
  <section class="flex items-start gap-2 rounded-md bg-ink-100 p-5">
    <CircleAlert size={16} class="mt-0.5 shrink-0 text-density-medium" aria-hidden="true" />
    <span>
      <span class="block text-sm font-medium text-ink-900">{t('model.noneTitle')}</span>
      <span class="block text-sm text-ink-500">{t('model.noneLead')}</span>
    </span>
  </section>
{/if}

<script lang="ts">
  import { untrack } from 'svelte';

  import { saveSite, type Status } from '$lib/api';
  import { t } from '$lib/i18n/i18n.svelte';

  let { status, onupdated }: { status: Status; onupdated: (status: Status) => void } = $props();

  // Read once: the field is a draft, and a status refresh must not reset it
  // under whoever is typing.
  let name = $state(untrack(() => status.site_name) ?? '');
  let saving = $state(false);
  let feedback = $state<{ tone: 'ok' | 'bad'; message: string } | null>(null);

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    saving = true;
    const outcome = await saveSite(name);
    saving = false;

    if (outcome.kind === 'ok') {
      feedback = { tone: 'ok', message: t('settings.siteSaved') };
      onupdated(outcome.status);
      return;
    }
    feedback = {
      tone: 'bad',
      message:
        outcome.kind === 'refused'
          ? t('wizard.refused', { message: outcome.message })
          : t('wizard.failed'),
    };
  }
</script>

<form onsubmit={submit} class="max-w-md">
  <label class="block text-sm font-medium text-ink-900">
    {t('site.label')}
    <input
      type="text"
      bind:value={name}
      required
      class="mt-1 block w-full rounded-md border border-ink-200 bg-white px-3 py-2 text-base
             font-normal text-ink-900"
    />
  </label>

  {#if feedback}
    <p
      role="status"
      class="mt-3 text-sm {feedback.tone === 'ok' ? 'text-ink-500' : 'text-density-saturated'}"
    >
      {feedback.message}
    </p>
  {/if}

  <button
    type="submit"
    disabled={saving || name.trim().length === 0 || name === status.site_name}
    class="mt-4 rounded-md bg-mariam-600 px-3 py-2 text-sm font-medium text-white
           transition-colors hover:bg-mariam-700 disabled:bg-ink-200 disabled:text-ink-500"
  >
    {saving ? t('wizard.saving') : t('settings.save')}
  </button>
</form>

<dl class="mt-6 border-t border-ink-100 pt-4">
  <div class="flex items-baseline justify-between gap-3">
    <dt class="text-sm text-ink-500">{t('status.kit')}</dt>
    <dd class="font-mono text-sm text-ink-900">{status.kit_id}</dd>
  </div>
</dl>

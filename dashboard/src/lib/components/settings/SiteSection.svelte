<script lang="ts">
  import { untrack } from 'svelte';

  import { saveSite, type Status } from '$lib/api';
  import { t } from '$lib/i18n/i18n.svelte';
  import Button from '$components/ui/Button.svelte';
  import Saved from '$components/ui/Saved.svelte';

  let { status, onupdated }: { status: Status; onupdated: (status: Status) => void } = $props();

  // Read once: the field is a draft, and a status refresh must not reset it
  // under whoever is typing.
  let name = $state(untrack(() => status.site_name) ?? '');
  let saving = $state(false);
  let saved = $state(false);
  let failure = $state<string | null>(null);

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    saving = true;
    const outcome = await saveSite(name);
    saving = false;

    if (outcome.kind === 'ok') {
      failure = null;
      saved = true;
      setTimeout(() => (saved = false), 2500);
      onupdated(outcome.status);
      return;
    }
    saved = false;
    failure =
      outcome.kind === 'refused'
        ? t('wizard.refused', { message: outcome.message })
        : t('wizard.failed');
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

  {#if failure}
    <p role="status" class="mt-3 text-sm text-danger">{failure}</p>
  {/if}

  <div class="mt-3">
    <Saved shown={saved} message={t('settings.siteSaved')} />
    <Button
      type="submit"
      disabled={saving || name.trim().length === 0 || name === status.site_name}
    >
      {saving ? t('wizard.saving') : t('settings.save')}
    </Button>
  </div>
</form>

<dl class="mt-6 border-t border-ink-100 pt-4">
  <div class="flex items-baseline justify-between gap-3">
    <dt class="text-sm text-ink-500">{t('status.kit')}</dt>
    <dd class="font-mono text-sm text-ink-900">{status.kit_id}</dd>
  </div>
</dl>

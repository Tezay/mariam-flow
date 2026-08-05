<script lang="ts">
  import { untrack } from 'svelte';

  import { saveSite } from '$lib/api/install';
  import { type Status } from '$lib/api/status';
  import { t } from '$lib/i18n/i18n.svelte';
  import Button from '$components/ui/Button.svelte';

  let {
    initialName,
    onupdated,
  }: { initialName: string | null; onupdated: (status: Status) => void } = $props();

  // Read once, on purpose: the field is a draft from here on, and a status
  // refresh must not reset it under the installer.
  let name = $state(untrack(() => initialName) ?? '');
  let saving = $state(false);
  let failure = $state<string | null>(null);

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    saving = true;
    const outcome = await saveSite(name);
    saving = false;

    if (outcome.kind === 'ok') {
      failure = null;
      onupdated(outcome.status);
      return;
    }
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
      placeholder={t('site.placeholder')}
      required
      class="mt-1 block w-full rounded-md border border-ink-200 bg-white px-3 py-2 text-base
             font-normal text-ink-900"
    />
  </label>
  <p class="mt-1 text-xs text-ink-500">{t('site.hint')}</p>

  {#if failure}
    <p role="status" class="mt-3 text-sm text-danger">{failure}</p>
  {/if}

  <div class="mt-4">
    <Button type="submit" disabled={saving || name.trim().length === 0}>
      {saving ? t('wizard.saving') : t('site.submit')}
    </Button>
  </div>
</form>

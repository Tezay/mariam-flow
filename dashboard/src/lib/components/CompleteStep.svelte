<script lang="ts">
  import { setInstallation, type Status } from '$lib/api';
  import { t } from '$lib/i18n/i18n.svelte';

  let { onupdated }: { onupdated: (status: Status) => void } = $props();

  let saving = $state(false);
  let failure = $state<string | null>(null);

  async function finish() {
    saving = true;
    const outcome = await setInstallation(true);
    saving = false;

    if (outcome.kind === 'ok') {
      failure = null;
      onupdated(outcome.status);
      return;
    }
    // A refusal here names the step still outstanding, which is more useful
    // than anything this screen could say on its own.
    failure =
      outcome.kind === 'refused'
        ? t('wizard.refused', { message: outcome.message })
        : t('wizard.failed');
  }
</script>

<div class="max-w-md">
  <p class="text-sm text-ink-500">{t('done.lead')}</p>

  {#if failure}
    <p role="status" class="mt-3 text-sm text-density-saturated">{failure}</p>
  {/if}

  <button
    type="button"
    onclick={() => void finish()}
    disabled={saving}
    class="mt-4 rounded-md bg-mariam-600 px-4 py-2 text-sm font-medium text-white
           transition-colors hover:bg-mariam-700 disabled:bg-ink-200 disabled:text-ink-500"
  >
    {saving ? t('wizard.saving') : t('done.submit')}
  </button>
</div>

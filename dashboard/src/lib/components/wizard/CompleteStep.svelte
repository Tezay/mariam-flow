<script lang="ts">
  import { setInstallation } from '$lib/api/install';
  import { type Status } from '$lib/api/status';
  import { t } from '$lib/i18n/i18n.svelte';
  import Button from '$components/ui/Button.svelte';

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
    <p role="status" class="mt-3 text-sm text-danger">{failure}</p>
  {/if}

  <div class="mt-4">
    <Button disabled={saving} onclick={() => void finish()}>
      {saving ? t('wizard.saving') : t('done.submit')}
    </Button>
  </div>
</div>

<script lang="ts">
  import Upload from '@lucide/svelte/icons/upload';

  import { importModel } from '$lib/api/models';
  import { type Status } from '$lib/api/status';
  import { t } from '$lib/i18n/i18n.svelte';
  import Button from '$components/ui/Button.svelte';
  import Modal from '$components/ui/Modal.svelte';

  let {
    onimported,
  }: {
    onimported: (status: Status) => void;
  } = $props();

  let open = $state(false);
  let bundle = $state<File | null>(null);
  let busy = $state(false);
  let failure = $state<string | null>(null);

  function close() {
    open = false;
    bundle = null;
    failure = null;
  }

  async function bringIn() {
    if (!bundle) {
      return;
    }
    busy = true;
    const outcome = await importModel(bundle);
    busy = false;
    if (outcome.kind === 'ok') {
      close();
      onimported(outcome.status);
      return;
    }
    // The appliance's own words: a mismatched receiver count is exactly what
    // the operator needs to read, and rewording it would lose the numbers.
    failure =
      outcome.kind === 'refused'
        ? t('wizard.refused', { message: outcome.message })
        : t('wizard.failed');
  }
</script>

<Button variant="outline" onclick={() => (open = true)}>
  <Upload size={14} aria-hidden="true" />{t('model.importOpen')}
</Button>

{#if open}
  <Modal title={t('model.importOpen')} lead={t('model.importLead')} oncancel={close}>
    <label class="-mt-1 block">
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
      <p role="status" class="text-sm text-danger">{failure}</p>
    {/if}
    <Button size="block" disabled={busy || bundle === null} onclick={() => void bringIn()}>
      {busy ? t('model.importing') : t('model.import')}
    </Button>
  </Modal>
{/if}

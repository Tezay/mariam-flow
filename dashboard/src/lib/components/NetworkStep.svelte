<script lang="ts">
  import { untrack } from 'svelte';

  import { saveUplink, type Status, type Uplink } from '$lib/api';
  import { t } from '$lib/i18n/i18n.svelte';
  import { canSubmitUplink, uplinkBody } from '$lib/wizard';

  type Choice = 'offline' | 'wifi' | 'ethernet';

  let {
    initialMode,
    initialSsid,
    onupdated,
  }: {
    initialMode: Uplink['mode'];
    initialSsid: string | null;
    onupdated: (status: Status) => void;
  } = $props();

  const CHOICES: Choice[] = ['wifi', 'ethernet', 'offline'];

  // Read once, on purpose: these are a draft from here on, and a status
  // refresh must not reset them under the installer.
  const stored = untrack(() => ({ mode: initialMode, ssid: initialSsid }));

  let choice = $state<Choice>(stored.mode === 'undecided' ? 'wifi' : stored.mode);
  // The appliance never returns a stored passphrase, so a revisit starts this
  // field empty rather than pretending to hold the old one.
  let wifi = $state({ ssid: stored.ssid ?? '', passphrase: '' });
  let saving = $state(false);
  let failure = $state<string | null>(null);

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    saving = true;
    const outcome = await saveUplink(uplinkBody(choice, wifi));
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
  <p class="text-sm text-ink-500">{t('net.lead')}</p>

  <fieldset class="mt-4 space-y-2">
    <legend class="sr-only">{t('wizard.stage.network')}</legend>
    {#each CHOICES as option (option)}
      <label
        class="flex cursor-pointer items-start gap-3 rounded-lg bg-white p-3 {choice === option
          ? 'ring-2 ring-mariam-600'
          : ''}"
      >
        <input
          type="radio"
          value={option}
          bind:group={choice}
          class="mt-1 accent-mariam-600"
          name="uplink"
        />
        <span>
          <span class="block text-sm font-medium text-ink-900">{t(`net.${option}` as const)}</span>
          <span class="block text-xs text-ink-500">{t(`net.${option}Hint` as const)}</span>
        </span>
      </label>
    {/each}
  </fieldset>

  {#if choice === 'wifi'}
    <div class="mt-4 space-y-3">
      <label class="block text-sm font-medium text-ink-900">
        {t('net.ssid')}
        <input
          type="text"
          bind:value={wifi.ssid}
          required
          class="mt-1 block w-full rounded-md border border-ink-200 bg-white px-3 py-2 text-base
                 font-normal text-ink-900"
        />
      </label>
      <label class="block text-sm font-medium text-ink-900">
        {t('net.passphrase')}
        <input
          type="password"
          bind:value={wifi.passphrase}
          class="mt-1 block w-full rounded-md border border-ink-200 bg-white px-3 py-2 text-base
                 font-normal text-ink-900"
        />
      </label>
      <p class="text-xs text-ink-500">{t('net.passphraseHint')}</p>
    </div>
  {/if}

  {#if failure}
    <p role="status" class="mt-3 text-sm text-density-saturated">{failure}</p>
  {/if}

  <button
    type="submit"
    disabled={saving || !canSubmitUplink(choice, wifi)}
    class="mt-4 rounded-md bg-mariam-600 px-4 py-2 text-sm font-medium text-white
           transition-colors hover:bg-mariam-700 disabled:bg-ink-200 disabled:text-ink-500"
  >
    {saving ? t('wizard.saving') : t('net.submit')}
  </button>
</form>

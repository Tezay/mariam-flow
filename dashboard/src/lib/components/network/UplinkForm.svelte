<script lang="ts">
  import { untrack } from 'svelte';

  import { saveUplink } from '$lib/api/install';
  import { type Status, type Uplink } from '$lib/api/status';
  import { t } from '$lib/i18n/i18n.svelte';
  import { uplinkBlocked, type Verdict } from '$lib/network';
  import { canSubmitUplink, uplinkBody } from '$lib/wizard';
  import Button from '$components/ui/Button.svelte';

  type Choice = 'offline' | 'wifi' | 'ethernet';

  let {
    initialMode,
    initialSsid,
    verdict,
    submitLabel,
    onupdated,
  }: {
    initialMode: Uplink['mode'];
    initialSsid: string | null;
    verdict: Verdict;
    submitLabel: string;
    onupdated: (status: Status) => void;
  } = $props();

  const CHOICES: Choice[] = ['wifi', 'ethernet', 'offline'];

  // Read once: these are a draft from here on.
  const stored = untrack(() => ({ mode: initialMode, ssid: initialSsid }));
  let choice = $state<Choice>(stored.mode === 'undecided' ? 'wifi' : stored.mode);
  // The appliance never returns a stored passphrase, so this starts empty
  // rather than pretending to hold the old one.
  let wifi = $state({ ssid: stored.ssid ?? '', passphrase: '' });
  let saving = $state(false);
  let failure = $state<string | null>(null);

  /* Credentials are only asked for once the interview says the appliance can
     act on them. Offering an empty field for a network that needs an account
     would invite a secret that no code can use. */
  const canJoin = $derived(verdict.kind === 'joinable');
  const wifiChosen = $derived(choice === 'wifi');

  const blocked = $derived(uplinkBlocked(choice, verdict));

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

<form onsubmit={submit}>
  <fieldset class="space-y-2">
    <legend class="sr-only">{t('wizard.stage.network')}</legend>
    {#each CHOICES as option (option)}
      <label
        class="flex cursor-pointer items-start gap-3 rounded-lg border p-3 transition-colors
               {choice === option
          ? 'border-mariam-600 bg-mariam-50'
          : 'border-ink-200 bg-white hover:border-ink-300'}"
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

  {#if wifiChosen}
    <div class="mt-4">
      <h4 class="text-xs font-medium tracking-wide text-ink-500 uppercase">
        {t('ask.credentials')}
      </h4>
      {#if canJoin}
        <div class="mt-2 space-y-3">
          <label class="block text-sm font-medium text-ink-900">
            {t('net.ssid')}
            <input
              type="text"
              bind:value={wifi.ssid}
              required
              class="mt-1 block w-full rounded-md border border-ink-200 bg-white px-3 py-2
                     text-base font-normal text-ink-900"
            />
          </label>
          <label class="block text-sm font-medium text-ink-900">
            {t('net.passphrase')}
            <input
              type="password"
              bind:value={wifi.passphrase}
              class="mt-1 block w-full rounded-md border border-ink-200 bg-white px-3 py-2
                     text-base font-normal text-ink-900"
            />
          </label>
          <p class="text-xs text-ink-500">{t('net.passphraseHint')}</p>
        </div>
      {:else if verdict.kind === 'unanswered'}
        <p class="mt-2 text-sm text-ink-500">{t('ask.credentialsPending')}</p>
      {:else}
        <p class="mt-2 text-sm text-ink-500">{t('ask.unsupported')}</p>
      {/if}
    </div>
  {/if}

  {#if failure}
    <p role="status" class="mt-3 text-sm text-danger">{failure}</p>
  {:else if blocked}
    <p class="mt-3 text-sm text-density-medium">
      {blocked === 'unanswered' ? t('net.answerFirst') : t('net.cannotJoin')}
    </p>
  {/if}

  <div class="mt-4">
    <Button
      type="submit"
      disabled={saving || blocked !== null || (wifiChosen && !canSubmitUplink('wifi', wifi))}
    >
      {saving ? t('wizard.saving') : submitLabel}
    </Button>
  </div>
</form>

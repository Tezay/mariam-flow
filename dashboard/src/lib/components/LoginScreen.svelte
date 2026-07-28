<script lang="ts">
  import ScanLine from '@lucide/svelte/icons/scan-line';

  import { login, takeSecretFromFragment, type LoginOutcome } from '$lib/api';
  import { t } from '$lib/i18n/i18n.svelte';
  import LocaleToggle from '$components/LocaleToggle.svelte';

  let { onauthenticated }: { onauthenticated: () => void } = $props();

  let secret = $state('');
  let busy = $state(false);
  let outcome = $state<LoginOutcome | null>(null);
  let scanned = $state(false);

  async function attempt(candidate: string) {
    if (busy || candidate.trim() === '') {
      return;
    }
    busy = true;
    outcome = await login(candidate);
    busy = false;
    if (outcome.kind === 'ok') {
      secret = '';
      onauthenticated();
    }
  }

  // A secret handed over by the label's QR code is used straight away, so
  // scanning it is the whole interaction.
  const fromQr = takeSecretFromFragment();
  if (fromQr) {
    scanned = true;
    secret = fromQr;
    void attempt(fromQr);
  }

  const message = $derived.by(() => {
    if (!outcome) return null;
    if (outcome.kind === 'invalid') return t('login.invalid');
    if (outcome.kind === 'throttled') return t('login.throttled', { seconds: outcome.seconds });
    if (outcome.kind === 'error') return t('login.failed');
    return null;
  });
</script>

<main class="flex min-h-dvh flex-col items-center justify-center bg-ink-50 px-4 py-10">
  <div class="w-full max-w-sm">
    <h1 class="text-2xl font-semibold tracking-tight text-mariam-600">{t('app.name')}</h1>
    <h2 class="mt-6 text-lg font-medium text-ink-900">{t('login.title')}</h2>
    <p class="mt-1 text-sm text-ink-500">{t('login.lead')}</p>

    {#if scanned}
      <p class="mt-4 flex items-center gap-2 text-sm text-mariam-600">
        <ScanLine size={16} aria-hidden="true" />
        {t('login.scanned')}
      </p>
    {/if}

    <form
      class="mt-6"
      onsubmit={(event) => {
        event.preventDefault();
        void attempt(secret);
      }}
    >
      <label class="block text-sm font-medium text-ink-700" for="secret">
        {t('login.secret')}
      </label>
      <input
        id="secret"
        name="secret"
        bind:value={secret}
        type="text"
        inputmode="text"
        autocomplete="off"
        autocapitalize="characters"
        autocorrect="off"
        spellcheck="false"
        placeholder={t('login.secretPlaceholder')}
        disabled={busy}
        aria-invalid={message !== null}
        aria-describedby={message ? 'secret-error' : undefined}
        class="mt-2 w-full rounded-md border border-ink-200 bg-white px-3 py-3 font-mono
               text-lg tracking-wider text-ink-900 placeholder:text-ink-200
               focus:border-mariam-600 focus:outline-none disabled:opacity-60"
      />

      {#if message}
        <p id="secret-error" role="alert" class="mt-2 text-sm text-danger">{message}</p>
      {/if}

      <button
        type="submit"
        disabled={busy || secret.trim() === ''}
        class="mt-4 w-full rounded-md bg-mariam-600 px-4 py-3 text-base font-medium text-white
               transition-colors hover:bg-mariam-700 disabled:cursor-not-allowed
               disabled:bg-ink-200 disabled:text-ink-500"
      >
        {busy ? t('login.working') : t('login.submit')}
      </button>
    </form>

    <div class="mt-8 flex justify-center">
      <LocaleToggle />
    </div>
  </div>
</main>

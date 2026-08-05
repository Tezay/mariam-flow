<script lang="ts">
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';

  import { saveNetworkSurvey } from '$lib/api/install';
  import { type NetworkSurvey, type Status } from '$lib/api/status';
  import { t } from '$lib/i18n/i18n.svelte';
  import { AUTHENTICATION_CHOICES, verdict } from '$lib/network';
  import Saved from '$components/ui/Saved.svelte';

  let {
    survey = $bindable(),
    onsaved,
  }: { survey: NetworkSurvey; onsaved?: (status: Status) => void } = $props();

  /* Answers record themselves. They are statements about the site rather than
     values being composed, so there is nothing to review before committing —
     and a Save button here sat next to another one for the credentials, which
     asked the reader to work out which was which. */
  let confirmed = $state(false);
  let failure = $state<string | null>(null);
  let pending: ReturnType<typeof setTimeout> | undefined;

  const outcome = $derived(verdict(survey));

  async function record() {
    const outcome_ = await saveNetworkSurvey(survey);
    if (outcome_.kind === 'ok') {
      failure = null;
      confirmed = true;
      clearTimeout(pending);
      pending = setTimeout(() => (confirmed = false), 2500);
      onsaved?.(outcome_.status);
      return;
    }
    confirmed = false;
    failure =
      outcome_.kind === 'refused'
        ? t('wizard.refused', { message: outcome_.message })
        : t('wizard.failed');
  }
</script>

<fieldset>
  <legend class="text-sm font-medium text-ink-900">{t('ask.title')}</legend>
  <p class="mt-1 text-xs text-ink-500">{t('ask.lead')}</p>

  <div class="mt-3 space-y-2">
    {#each AUTHENTICATION_CHOICES as choice (choice)}
      <label
        class="flex cursor-pointer items-center gap-3 rounded-lg border p-3 text-sm
               transition-colors {survey.authentication === choice
          ? 'border-mariam-600 bg-mariam-50'
          : 'border-ink-200 bg-white hover:border-ink-300'}"
      >
        <input
          type="radio"
          name="site-authentication"
          value={choice}
          bind:group={survey.authentication}
          onchange={() => void record()}
          class="accent-mariam-600"
        />
        <span class="text-ink-900">{t(`ask.${choice}` as const)}</span>
      </label>
    {/each}
  </div>
</fieldset>

<div class="mt-4 space-y-4">
  <div>
    <label class="flex items-start gap-3 text-sm text-ink-900">
      <input
        type="checkbox"
        bind:checked={survey.registration_required}
        onchange={() => void record()}
        class="mt-1 accent-mariam-600"
      />
      {t('ask.registration')}
    </label>
    <p class="mt-1 ml-7 text-xs text-ink-500">{t('ask.registrationHint')}</p>
  </div>
  <div>
    <label class="flex items-start gap-3 text-sm text-ink-900">
      <input
        type="checkbox"
        bind:checked={survey.fixed_address}
        onchange={() => void record()}
        class="mt-1 accent-mariam-600"
      />
      {t('ask.fixedAddress')}
    </label>
    <p class="mt-1 ml-7 text-xs text-ink-500">{t('ask.fixedAddressHint')}</p>
  </div>
</div>

<div class="mt-4">
  {#if failure}
    <p role="status" class="text-sm text-danger">{failure}</p>
  {:else}
    <Saved shown={confirmed} message={t('ask.recorded')} />
  {/if}
</div>

{#if outcome.kind === 'joinable'}
  <p class="mt-2 flex items-start gap-2 rounded-md bg-ink-50 p-3 text-sm text-ink-700">
    <CircleCheck size={16} class="mt-0.5 shrink-0 text-success" aria-hidden="true" />
    {t('ask.joinable')}
  </p>
{:else if outcome.kind === 'needs-administrator'}
  <!-- Said plainly rather than hidden behind a disabled field: the installer
       has to leave with something to hand to the site's network team. -->
  <p class="mt-2 flex items-start gap-2 rounded-md bg-ink-50 p-3 text-sm text-ink-700">
    <TriangleAlert size={16} class="mt-0.5 shrink-0 text-density-medium" aria-hidden="true" />
    {t('ask.needsAdmin')}
  </p>
{/if}

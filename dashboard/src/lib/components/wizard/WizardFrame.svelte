<script lang="ts">
  import { type Stage, type Status } from '$lib/api/status';
  import { t } from '$lib/i18n/i18n.svelte';
  import { STEPS, completion, visibleStage } from '$lib/wizard';
  import CalibrationStep from '$components/wizard/CalibrationStep.svelte';
  import CompleteStep from '$components/wizard/CompleteStep.svelte';
  import NetworkStep from '$components/wizard/NetworkStep.svelte';
  import PairingStep from '$components/wizard/PairingStep.svelte';
  import SiteStep from '$components/wizard/SiteStep.svelte';
  import WizardIllustration from '$components/wizard/WizardIllustration.svelte';
  import WizardStepper from '$components/wizard/WizardStepper.svelte';

  let { status, onupdated }: { status: Status; onupdated: (status: Status) => void } = $props();

  /* Which step is shown stays a function of the appliance's readiness plus
     this one intent, rather than a cursor the browser advances: an
     installation interrupted mid-step resumes where the stored facts say it
     stands. */
  let revisiting = $state<Stage | null>(null);

  const stage = $derived(visibleStage(status, revisiting));
  const done = $derived(completion(status));
  const position = $derived(stage === 'complete' ? STEPS.length : STEPS.indexOf(stage) + 1);

  function accept(next: Status) {
    revisiting = null;
    onupdated(next);
  }
</script>

<!-- No navigation during installation beyond the stepper: one question at a
     time, full frame. The tabbed shell appears once the installer closes it. -->
<div class="flex min-h-dvh flex-col bg-ink-50">
  <header class="border-b border-ink-200 bg-white">
    <div class="mx-auto flex max-w-5xl items-baseline justify-between gap-4 px-4 py-3 sm:px-6">
      <p class="truncate text-sm font-semibold text-mariam-600">{t('app.name')}</p>
      <p class="shrink-0 text-xs text-ink-500">
        {t('wizard.progress', { current: position, total: STEPS.length })}
      </p>
    </div>
    <div class="mx-auto max-w-5xl px-4 pb-4 sm:px-6">
      <WizardStepper {stage} {done} onrevisit={(step) => (revisiting = step)} />
    </div>
  </header>

  <main class="mx-auto w-full max-w-5xl flex-1 px-4 py-8 sm:px-6">
    <!-- One structure, two shapes: the illustration leads on a phone, where it
         is thumbed past first, and sits beside the content on a wide screen
         rather than pushing it down. -->
    <div class="grid gap-8 lg:grid-cols-[minmax(0,1fr)_20rem] lg:items-start lg:gap-12">
      <div class="order-2 lg:order-1">
        <h1 class="text-2xl font-semibold tracking-tight text-ink-900">
          {t(`wizard.stage.${stage}` as const)}
        </h1>
        <p class="mt-2 text-base text-ink-500">{t(`wizard.stage.${stage}.lead` as const)}</p>

        <div class="mt-6 rounded-md bg-white p-5 ring-1 ring-ink-100">
          {#if stage === 'site'}
            <SiteStep initialName={status.site_name} onupdated={accept} />
          {:else if stage === 'nodes'}
            <PairingStep onupdated={accept} />
          {:else if stage === 'network'}
            <NetworkStep
              {status}
              initialMode={status.uplink.mode}
              initialSsid={status.uplink.ssid ?? null}
              onupdated={accept}
            />
          {:else if stage === 'calibration'}
            <CalibrationStep {status} onupdated={accept} />
          {:else if stage === 'complete'}
            <CompleteStep onupdated={accept} />
          {/if}
        </div>
      </div>

      <div class="order-1 mx-auto w-full max-w-56 lg:order-2 lg:max-w-none">
        <WizardIllustration {stage} />
      </div>
    </div>
  </main>
</div>

<script lang="ts">
  import Check from '@lucide/svelte/icons/check';

  import type { Stage, Status } from '$lib/api';
  import { t } from '$lib/i18n/i18n.svelte';

  let { status, stage }: { status: Status; stage: Stage } = $props();

  /** The steps of the guided installation, in order. */
  const STEPS: Stage[] = ['site', 'nodes', 'network', 'calibration'];

  const done = $derived<Record<Stage, boolean>>({
    site: status.readiness.site_named,
    nodes: status.readiness.nodes_paired,
    network: status.readiness.uplink_decided,
    calibration: status.readiness.model_ready,
    complete: false,
  });

  const position = $derived(stage === 'complete' ? STEPS.length : STEPS.indexOf(stage) + 1);
</script>

<!-- During installation there is no navigation at all: one question, full
     frame, the way a guided setup should feel. The tabbed shell only
     appears once the installer closes the installation. -->
<div class="flex min-h-dvh flex-col bg-ink-50">
  <header class="px-6 pt-8">
    <p class="text-xs font-medium uppercase tracking-wide text-ink-500">
      {t('wizard.progress', { current: position, total: STEPS.length })}
    </p>
    <ol class="mt-3 flex gap-2" aria-hidden="true">
      {#each STEPS as step (step)}
        <li
          class="h-1.5 flex-1 rounded-full {done[step]
            ? 'bg-mariam-600'
            : step === stage
              ? 'bg-mariam-200'
              : 'bg-ink-200'}"
        ></li>
      {/each}
    </ol>
  </header>

  <main class="flex flex-1 flex-col justify-center px-6 py-10">
    <h1 class="text-2xl font-semibold tracking-tight text-ink-900">
      {t(`wizard.stage.${stage}` as const)}
    </h1>
    <p class="mt-2 text-base text-ink-500">{t(`wizard.stage.${stage}.lead` as const)}</p>

    <ul class="mt-8 space-y-2">
      {#each STEPS as step (step)}
        <li class="flex items-center gap-3 text-sm">
          <span
            class="flex size-6 shrink-0 items-center justify-center rounded-full {done[step]
              ? 'bg-mariam-600 text-white'
              : 'bg-ink-100 text-ink-500'}"
          >
            {#if done[step]}
              <Check size={14} aria-hidden="true" />
            {/if}
          </span>
          <span class={done[step] ? 'text-ink-900' : 'text-ink-500'}>
            {t(`wizard.stage.${step}` as const)}
          </span>
        </li>
      {/each}
    </ul>

    <p class="mt-8 rounded-lg bg-white p-4 text-sm text-ink-500">{t('wizard.comingNext')}</p>
  </main>
</div>

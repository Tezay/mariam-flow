<script lang="ts">
  import Check from '@lucide/svelte/icons/check';

  import type { Stage } from '$lib/api';
  import { t } from '$lib/i18n/i18n.svelte';
  import { STEPS } from '$lib/wizard';

  let {
    stage,
    done,
    onrevisit,
  }: {
    stage: Stage;
    done: Record<Stage, boolean>;
    onrevisit: (step: Stage) => void;
  } = $props();
</script>

<!-- The stepper is the navigation. A separate row of "back to…" links said
     the same thing a second time, once per finished step. -->
<ol class="flex items-start">
  {#each STEPS as step, index (step)}
    {@const finished = done[step]}
    {@const current = step === stage}
    {@const reachable = finished && !current}
    <li class="flex flex-1 flex-col items-center gap-2 text-center">
      <div class="flex w-full items-center">
        <span
          class="h-0.5 flex-1 {index === 0
            ? 'bg-transparent'
            : done[STEPS[index - 1]]
              ? 'bg-mariam-600'
              : 'bg-ink-200'}"
        ></span>
        <svelte:element
          this={reachable ? 'button' : 'span'}
          role={reachable ? 'button' : undefined}
          tabindex={reachable ? 0 : undefined}
          aria-current={current ? 'step' : undefined}
          aria-label={reachable
            ? t('wizard.back', { step: t(`wizard.stage.${step}` as const) })
            : undefined}
          onclick={reachable ? () => onrevisit(step) : undefined}
          class="flex size-8 shrink-0 items-center justify-center rounded-full text-xs
                 font-semibold transition-colors {finished
            ? 'bg-mariam-600 text-white'
            : current
              ? 'bg-white text-mariam-600 ring-2 ring-mariam-600'
              : 'bg-ink-100 text-ink-500'} {reachable ? 'cursor-pointer hover:bg-mariam-700' : ''}"
        >
          {#if finished}
            <Check size={16} aria-hidden="true" />
          {:else}
            {index + 1}
          {/if}
        </svelte:element>
        <span
          class="h-0.5 flex-1 {index === STEPS.length - 1
            ? 'bg-transparent'
            : finished
              ? 'bg-mariam-600'
              : 'bg-ink-200'}"
        ></span>
      </div>
      <span
        class="hidden px-1 text-xs sm:block {current ? 'font-medium text-ink-900' : 'text-ink-500'}"
      >
        {t(`wizard.stage.${step}` as const)}
      </span>
    </li>
  {/each}
</ol>

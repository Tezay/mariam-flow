<script lang="ts">
  import { untrack } from 'svelte';
  import Circle from '@lucide/svelte/icons/circle';
  import Pencil from '@lucide/svelte/icons/pencil';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';

  import { startCalibration } from '$lib/api/calibration';
  import { type LiveSnapshot } from '$lib/api/live';
  import { type Status } from '$lib/api/status';
  import { defaultEnvironment } from '$lib/calibration';
  import { formattingLocale, t } from '$lib/i18n/i18n.svelte';
  import { allPlaced, answering, knownPositions, quietReceivers } from '$lib/sensors';
  import Button from '$components/ui/Button.svelte';

  let {
    status,
    snapshot,
    dialog = false,
    onstarted,
  }: {
    status: Status;
    snapshot: LiveSnapshot | null;
    /** Shown in a dialog over a site in service, rather than as an installer's step. */
    dialog?: boolean;
    onstarted: (status: Status) => void;
  } = $props();

  // Read once: the form is mounted per recording, so these are a draft from
  // here on, and a recording opened an hour later is not named after this one.
  let environment = $state(
    untrack(() => defaultEnvironment(status.site_name, new Date(), formattingLocale())),
  );
  let positions = $state<Record<string, string>>(untrack(() => knownPositions(status.nodes)));
  /* Stored positions are shown, and edited only on request: a field that is
     always open invites retyping, and one emptied by accident would record a
     sensor with no position. */
  let editing = $state(untrack(() => !allPlaced(status.nodes)));
  let busy = $state(false);
  let failure = $state<string | null>(null);

  const answers = $derived(answering(status.nodes, snapshot?.stream, snapshot?.now_us));
  const quiet = $derived(quietReceivers(status.nodes, snapshot?.stream, snapshot?.now_us));
  const receiving = $derived(
    (snapshot?.stream.running ?? false) &&
      quiet.length < status.nodes.filter((node) => node.role === 'rx').length,
  );

  async function start() {
    busy = true;
    const outcome = await startCalibration({ environment, positions: editing ? positions : {} });
    busy = false;
    if (outcome.kind === 'ok') {
      onstarted(outcome.status);
      return;
    }
    failure =
      outcome.kind === 'refused'
        ? t('wizard.refused', { message: outcome.message })
        : t('wizard.failed');
  }

  function discardEdits() {
    positions = knownPositions(status.nodes);
    editing = false;
  }
</script>

{#snippet answer(nodeId: string)}
  <!-- Nothing is claimed either way until the appliance has spoken. -->
  <span
    class="size-1.5 shrink-0 self-center rounded-full {snapshot === null
      ? 'bg-ink-200'
      : answers[nodeId]
        ? 'bg-density-empty'
        : 'bg-density-saturated'}"
  ></span>
{/snippet}

<div>
  <label class="block text-sm font-medium text-ink-900">
    {t('cal.environment')}
    <input
      type="text"
      bind:value={environment}
      placeholder={t('cal.environmentHint')}
      class="mt-1 block w-full rounded-md border border-ink-200 px-3 py-2 text-base
             font-normal text-ink-900"
    />
  </label>

  <fieldset class="mt-4">
    <legend class="text-sm font-medium text-ink-900">{t('cal.positions')}</legend>
    {#if editing}
      {#if status.nodes.some((node) => node.position)}
        <p
          class="mt-2 flex items-start gap-2 rounded-md bg-ink-50 p-3 text-sm text-ink-700"
          role="note"
        >
          <TriangleAlert size={16} class="mt-0.5 shrink-0" aria-hidden="true" />
          {t('cal.positionsMoved')}
        </p>
      {/if}
      <div class="mt-2 space-y-2">
        {#each status.nodes as node (node.node_id)}
          <label class="block text-sm text-ink-900">
            <span class="flex items-baseline gap-2">
              {@render answer(node.node_id)}
              <span class="font-mono text-xs text-ink-500">{node.node_id}</span>
            </span>
            <input
              type="text"
              bind:value={positions[node.node_id]}
              class="mt-1 block w-full rounded-md border border-ink-200 px-3 py-2 text-base
                     font-normal text-ink-900"
            />
          </label>
        {/each}
      </div>
      <p class="mt-2 text-xs text-ink-500">{t('cal.positionsKept')}</p>
      {#if allPlaced(status.nodes)}
        <div class="mt-2 -ml-2.5">
          <Button variant="quiet" size="sm" onclick={discardEdits}>
            {t('cal.positionsCancel')}
          </Button>
        </div>
      {/if}
    {:else}
      <ul class="mt-2 divide-y divide-ink-100 rounded-md ring-1 ring-ink-200">
        {#each status.nodes as node (node.node_id)}
          <li class="flex items-baseline gap-2 px-3 py-2 text-sm">
            {@render answer(node.node_id)}
            <span class="w-10 shrink-0 font-mono text-xs text-ink-500">{node.node_id}</span>
            <span class="min-w-0 flex-1 {node.position ? 'text-ink-900' : 'text-ink-300'}">
              {node.position || t('sensors.noPosition')}
            </span>
          </li>
        {/each}
      </ul>
      <div class="mt-2 -ml-2.5">
        <Button variant="quiet" size="sm" onclick={() => (editing = true)}>
          <Pencil size={13} aria-hidden="true" />{t('cal.positionsEdit')}
        </Button>
      </div>
    {/if}
  </fieldset>

  {#if dialog}
    <div class="mt-4 rounded-md bg-ink-50 p-3">
      <p class="text-xs font-medium tracking-wide text-ink-500 uppercase">{t('cal.during')}</p>
      <ul class="mt-1.5 list-disc space-y-1 pl-4 text-sm text-ink-700">
        <li>{t('cal.duringEstimate')}</li>
        <li>{t('cal.duringMarks')}</li>
        <li>{t('cal.duringSeal')}</li>
      </ul>
    </div>
  {/if}

  {#if failure}
    <p role="status" class="mt-3 text-sm text-danger">{failure}</p>
  {/if}
  {#if !status.readiness.nodes_paired}
    <p class="mt-3 text-sm text-ink-500">{t('cal.notReady')}</p>
  {:else if snapshot !== null && !receiving}
    <p class="mt-3 text-sm text-density-medium">{t('cal.noStream')}</p>
  {:else if snapshot !== null && quiet.length > 0}
    <p class="mt-3 text-sm text-density-medium">
      {t('cal.quiet', { nodes: quiet.join(', ') })}
    </p>
  {/if}

  <div class="mt-4">
    <Button
      variant="danger"
      size={dialog ? 'block' : 'md'}
      disabled={busy ||
        !status.readiness.nodes_paired ||
        !receiving ||
        environment.trim().length === 0}
      onclick={() => void start()}
    >
      <Circle size={14} class="fill-current" aria-hidden="true" />
      {busy ? t('cal.starting') : t('cal.start')}
    </Button>
  </div>
</div>
